// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { boundedStore, CAPACITY_KEY, AUTHORITY_VALUE_BYTES, AuthorityCapacityError } from '../src/bounded-store.ts';
import { MemoryStore } from './fixtures/memory-store.ts';

test('capacity rejects a whole transaction, not just its final insert', async () => {
  const raw = new MemoryStore();
  const store = boundedStore(raw, 2);
  await store.transaction(tx => tx.put('existing', { enabled: true }));
  await assert.rejects(store.transaction(async tx => {
    await tx.put('existing', { enabled: false });
    await tx.put('device:new', {});
    await tx.put('connector:new', {});
  }), AuthorityCapacityError);
  assert.deepEqual([...raw.data], [['maintenance:capacity', 1], ['existing', { enabled: true }]]);
});

test('full capacity permits reads, revocation updates, cleanup and replacement', async () => {
  const raw = new MemoryStore();
  const store = boundedStore(raw, 1);
  await store.transaction(tx => tx.put('device:old', { enabled: true }));
  await store.transaction(async tx => {
    assert.deepEqual(await tx.get('device:old'), { enabled: true });
    await tx.put('device:old', { enabled: false });
    await tx.put('maintenance:cursor', { after: '', hadRecords: true });
  });
  assert.equal(raw.data.get(CAPACITY_KEY), 1);
  await store.transaction(async tx => {
    await tx.put('device:new', {});
    assert.equal(await tx.delete('device:old'), true);
    assert.equal(await tx.delete('missing'), false);
  });
  assert.equal(raw.data.get(CAPACITY_KEY), 1);
  await store.transaction(tx => tx.delete('device:new'));
  assert.equal(raw.data.get(CAPACITY_KEY), 0);
});

test('repeated and concurrent operations on one key count its final existence once', async () => {
  const raw = new MemoryStore();
  const store = boundedStore(raw, 1);
  await store.transaction(async tx => {
    await Promise.all([tx.put('same', 1), tx.put('same', 2)]);
    await tx.delete('same');
    await tx.put('same', 3);
  });
  assert.equal(raw.data.get(CAPACITY_KEY), 1);
  assert.equal(raw.data.get('same'), 3);
  const results = await Promise.allSettled(Array.from({ length: 12 }, (_, n) =>
    store.transaction(tx => tx.put(`other:${n}`, {}))));
  assert(results.every(result => result.status === 'rejected' && result.reason instanceof AuthorityCapacityError));
  assert.equal(raw.data.size, 2);
});

test('bootstrap counts unknown records in bounded pages; restart uses durable count', async () => {
  const raw = new MemoryStore();
  for (let n = 0; n < 260; n++) raw.data.set(`unknown:${n.toString().padStart(3, '0')}`, {});
  raw.data.set('maintenance:cursor', { after: '' });
  const store = boundedStore(raw, 261);
  await store.transaction(tx => tx.put('device:new', {}));
  assert.equal(raw.data.get(CAPACITY_KEY), 261);
  const restarted = boundedStore(raw, 261);
  await assert.rejects(restarted.transaction(tx => tx.put('extra', {})), AuthorityCapacityError);
  assert(raw.data.has('unknown:259'));
});

test('missing or invalid accounting cannot silently admit an oversized legacy store', async () => {
  const raw = new MemoryStore();
  raw.data.set('old:1', {});
  raw.data.set('old:2', {});
  await assert.rejects(boundedStore(raw, 1).transaction(tx => tx.get('old:1')), AuthorityCapacityError);
  assert.equal(raw.data.has(CAPACITY_KEY), false);
  for (const invalid of [-1, 1.5, NaN, '2', 3]) {
    raw.data.set(CAPACITY_KEY, invalid);
    await assert.rejects(boundedStore(raw, 2).transaction(tx => tx.put('new', {})), AuthorityCapacityError);
    assert.equal(raw.data.has('new'), false);
  }
});

test('record bytes and accounting metadata are guarded on both insertion and update', async () => {
  const raw = new MemoryStore();
  const store = boundedStore(raw, 2);
  await store.transaction(tx => tx.put('device', 'small'));
  for (const value of ['x'.repeat(AUTHORITY_VALUE_BYTES), '\u4e2d'.repeat(AUTHORITY_VALUE_BYTES / 2), undefined]) {
    await assert.rejects(store.transaction(tx => tx.put('device', value)), AuthorityCapacityError);
    assert.equal(raw.data.get('device'), 'small');
  }
  await assert.rejects(store.transaction(tx => tx.put(CAPACITY_KEY, 0)), AuthorityCapacityError);
  await assert.rejects(store.transaction(tx => tx.delete(CAPACITY_KEY)), AuthorityCapacityError);
  assert.equal(raw.data.get(CAPACITY_KEY), 1);
});
