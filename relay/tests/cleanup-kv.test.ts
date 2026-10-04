// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { cleanupKV } from '../src/cleanup-kv.ts';

test('OAuth cleanup view bounds list size and all external operations per batch', async () => {
  let operations = 0;
  const kv = {
    async list(options: KVNamespaceListOptions) { operations++; assert.equal(options.limit, 4); return { keys: [], list_complete: true }; },
    async delete(_key: string) { operations++; },
  } as unknown as KVNamespace;
  const view = cleanupKV(kv);
  await view.list({ prefix: 'token:', limit: 1000 });
  const results = await Promise.allSettled(Array.from({ length: 20 }, (_, i) => view.delete(String(i))));
  assert.equal(operations, 12);
  assert.equal(results.filter(result => result.status === 'rejected').length, 9);
  assert.throws(() => view.get('private'), /Unsupported OAuth cleanup operation/);
  assert.equal(operations, 12);
});

test('stalled cleanup cannot initiate another external operation after its deadline', async t => {
  t.mock.timers.enable({ apis: ['Date'], now: 1000 });
  let calls = 0;
  const view = cleanupKV({ delete: async () => { calls++; } } as unknown as KVNamespace);
  await view.delete('first');
  t.mock.timers.setTime(2500);
  await assert.rejects(view.delete('late'), /batch incomplete/);
  assert.equal(calls, 1);
});
