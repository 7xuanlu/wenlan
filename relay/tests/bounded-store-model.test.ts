// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { boundedStore, CAPACITY_KEY, AuthorityCapacityError } from '../src/bounded-store.ts';
import { MemoryStore } from './fixtures/memory-store.ts';

type Op = { type: 'put'; key: string; value: string } | { type: 'delete'; key: string };

function mulberry32(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let mixed = state;
    mixed = Math.imul(mixed ^ (mixed >>> 15), mixed | 1);
    mixed ^= mixed + Math.imul(mixed ^ (mixed >>> 7), mixed | 61);
    return ((mixed ^ (mixed >>> 14)) >>> 0) / 4294967296;
  };
}

function appEntries(raw: MemoryStore): Map<string, unknown> {
  const out = new Map<string, unknown>();
  for (const [key, value] of raw.data) {
    if (key === CAPACITY_KEY || key === 'maintenance:cursor') continue;
    out.set(key, value);
  }
  return out;
}

function assertModelMatches(raw: MemoryStore, model: Map<string, unknown>): void {
  const actual = appEntries(raw);
  assert.equal(actual.size, model.size, `record count mismatch: actual=${actual.size} model=${model.size}`);
  for (const [key, value] of model) {
    assert.ok(actual.has(key), `missing key ${key}`);
    assert.deepEqual(actual.get(key), value);
  }
  assert.equal(raw.data.get(CAPACITY_KEY), model.size, 'durable capacity count mismatch');
}

async function runModelSweep(limit: number, seed: number, transactions: number): Promise<{ committed: number; rejected: number }> {
  const raw = new MemoryStore();
  const store = boundedStore(raw, limit);
  const model = new Map<string, unknown>();
  const rand = mulberry32(seed);
  const domain = limit + 2; // small key space forces collisions, updates, replacements
  let committed = 0;
  let rejected = 0;

  for (let t = 0; t < transactions; t++) {
    const length = 1 + Math.floor(rand() * 5); // 1..5 ops per transaction
    const ops: Op[] = [];
    for (let o = 0; o < length; o++) {
      const key = `rec:${Math.floor(rand() * domain)}`;
      if (rand() < 0.6) ops.push({ type: 'put', key, value: `v${seed}:${t}:${o}` });
      else ops.push({ type: 'delete', key });
    }

    // Independent reference: apply ops to a clone to predict commit/reject.
    const predicted = new Map(model);
    for (const op of ops) {
      if (op.type === 'put') predicted.set(op.key, op.value);
      else predicted.delete(op.key);
    }
    const shouldCommit = predicted.size <= limit;

    const before = new Map(raw.data);
    let ok = true;
    try {
      await store.transaction(async tx => {
        for (const op of ops) {
          if (op.type === 'put') await tx.put(op.key, op.value);
          else await tx.delete(op.key);
        }
      });
    } catch (error) {
      ok = false;
      assert.ok(error instanceof AuthorityCapacityError, `txn ${t} rejected with unexpected error: ${error}`);
    }
    assert.equal(ok, shouldCommit, `txn ${t} commit=${ok} predicted=${shouldCommit} ops=${JSON.stringify(ops)}`);

    if (ok) {
      committed++;
      model.clear();
      for (const [k, v] of predicted) model.set(k, v);
    } else {
      rejected++;
      // Over-limit must preserve the complete prior state, including values.
      assert.deepEqual([...raw.data.entries()], [...before.entries()], `txn ${t} mutated state on reject`);
    }
    assertModelMatches(raw, model);
  }
  return { committed, rejected };
}

test('bounded store model: deterministic mixed sweep matches reference (limit 5)', async () => {
  const result = await runModelSweep(5, 1234, 100);
  assert.ok(result.committed > 0, 'expected some committed transactions');
  assert.ok(result.rejected > 0, 'expected some rejected over-limit transactions');
});

test('bounded store model: small and larger limits with different seeds', async () => {
  const a = await runModelSweep(3, 7, 50);
  const b = await runModelSweep(8, 42, 40);
  for (const result of [a, b]) {
    assert.ok(result.committed > 0, 'expected commits');
    assert.ok(result.rejected > 0, 'expected rejects');
  }
  // Total ops bounded: each txn holds at most 5 ops: (100 + 50 + 40) * 5 = 950 < 1000.
  assert.ok((100 + 50 + 40) * 5 < 1000, 'operation budget sanity');
});

test('bounded store model: insert-then-delete same key in a full store is net-zero', async () => {
  const raw = new MemoryStore();
  const store = boundedStore(raw, 3);
  await store.transaction(async tx => {
    await tx.put('rec:0', 'a');
    await tx.put('rec:1', 'b');
    await tx.put('rec:2', 'c');
  });
  assert.equal(raw.data.get(CAPACITY_KEY), 3);

  // Full store: insert a new key then delete that same key; net growth is zero.
  await store.transaction(async tx => {
    await tx.put('rec:new', 'tmp');
    assert.equal(await tx.delete('rec:new'), true);
  });
  assert.deepEqual([...appEntries(raw).keys()].sort(), ['rec:0', 'rec:1', 'rec:2']);
  assert.equal(raw.data.get(CAPACITY_KEY), 3);

  // Replacement via delete-then-insert of a different key also fits exactly.
  await store.transaction(async tx => {
    assert.equal(await tx.delete('rec:2'), true);
    await tx.put('rec:new', 'd');
  });
  assert.deepEqual([...appEntries(raw).keys()].sort(), ['rec:0', 'rec:1', 'rec:new']);
  assert.equal(raw.data.get(CAPACITY_KEY), 3);

  // Genuine growth beyond the limit still rolls back completely.
  await assert.rejects(store.transaction(async tx => {
    await tx.put('rec:overflow', 'x');
  }), AuthorityCapacityError);
  assert.deepEqual([...appEntries(raw).keys()].sort(), ['rec:0', 'rec:1', 'rec:new']);
  assert.equal(raw.data.get(CAPACITY_KEY), 3);
});
