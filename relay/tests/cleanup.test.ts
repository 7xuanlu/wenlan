// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { maintainAuthority } from '../src/cleanup.ts';
import { MemoryStore } from './fixtures/memory-store.ts';
import { hashSecret } from '../src/secrets.ts';
import { authorizationGrantActive, claimAuthorizationGrant, replaceClientAuthorization, clientKey } from '../src/grants.ts';
import { CleanupBatchIncomplete } from '../src/cleanup-kv.ts';

const now = 1_800_000_000_000;
const noop = async () => {};
const id = 'd'.repeat(64);
const grantId = 'g'.repeat(16);
const auth = 'a'.repeat(64);
const identity = { grantId, clientId: 'client' };
const permission = { subject: id, connectorId: id, space: 'review', generation: 0 };

async function managed(t: any) {
  t.mock.timers.enable({ apis: ['Date'], now });
  const store = new MemoryStore();
  store.data.set(`device:${id}`, { id, subject: id, enabled: true, expiresAt: now + 100_000 });
  store.data.set(`connector:${id}`, { ...permission, id, enabled: true, expiresAt: now + 1000 });
  await replaceClientAuthorization(store, id, identity.clientId, auth);
  await claimAuthorizationGrant(store, identity, permission, auth);
  return store;
}

test('expired transient records are removed, live and unknown records are preserved', async () => {
  const store = new MemoryStore();
  for (const prefix of ['pairing:', 'authorization-pairing:', 'oauth-request:', 'oauth-client:']) {
    store.data.set(`${prefix}expired`, { expiresAt: now });
    store.data.set(`${prefix}live`, { expiresAt: now + 1 });
  }
  store.data.set('future-schema:do-not-delete', { expiresAt: 0 });
  await maintainAuthority(store, noop, now);
  for (const key of store.data.keys()) assert(!key.endsWith('expired'));
  assert(store.data.has('future-schema:do-not-delete'));
  assert.equal([...store.data.keys()].filter(k => k.endsWith('live')).length, 4);
});

test('offline routes survive while device credentials can renew; revoked or expired devices do not', async () => {
  const store = new MemoryStore();
  for (const [id, enabled, expiresAt] of [['offline', true, now + 1], ['expired', true, now], ['revoked', false, now + 1]] as const) {
    store.data.set(`device:${id}`, { id, subject: id, enabled, expiresAt });
    store.data.set(`connector:${id}`, { id, subject: id, enabled, expiresAt: now - 1, backendToken: 'secret' });
  }
  store.data.set('connector:orphan', { expiresAt: now + 1 });
  await maintainAuthority(store, noop, now);
  assert(store.data.has('device:offline'));
  assert(store.data.has('connector:offline'));
  for (const id of ['expired', 'revoked', 'orphan']) {
    assert(!store.data.has(`device:${id}`));
    assert(!store.data.has(`connector:${id}`));
  }
});

test('session cleanup deletes stale reverse claims without deleting a replacement session claim', async () => {
  const store = new MemoryStore();
  const reverse = `mcp-session-owner:${await hashSecret(JSON.stringify(['route', 'backend']))}`;
  store.data.set('mcp-session:old', { id: 'old', route: 'route', backendId: 'backend', active: false, expiresAt: now + 1 });
  store.data.set('mcp-session:new', { id: 'new', route: 'route', backendId: 'backend', active: true, expiresAt: now + 1 });
  store.data.set(reverse, 'new');
  store.data.set('mcp-session-owner:orphan', 'missing');
  await maintainAuthority(store, noop, now);
  assert(!store.data.has('mcp-session:old'));
  assert(!store.data.has('mcp-session-owner:orphan'));
  assert.equal(store.data.get(reverse), 'new');
  await maintainAuthority(store, noop, now + 2);
  assert(!store.data.has(reverse));
  assert(!store.data.has('mcp-session:new'));
});

test('bounded persistent cursor reaches records beyond live entries and resumes after restart', async () => {
  const store = new MemoryStore();
  for (let i = 0; i < 90; i++) store.data.set(`pairing:${String(i).padStart(3, '0')}`, { expiresAt: i < 70 ? now + 1 : now });
  const first = await maintainAuthority(store, noop, now);
  assert(first.checked <= 32);
  assert(store.data.has('pairing:089'));
  const restarted = new MemoryStore();
  restarted.data = structuredClone(store.data);
  for (let i = 0; i < 4; i++) assert((await maintainAuthority(restarted, noop, now)).checked <= 32);
  assert(!restarted.data.has('pairing:089'));
  assert(restarted.data.has('pairing:000'));
});

test('an empty final page does not stop alarms while earlier pages still contain live records', async () => {
  const store = new MemoryStore();
  for (let i = 0; i < 32; i++) store.data.set(`pairing:${String(i).padStart(3, '0')}`, { expiresAt: now + 1 });
  assert.equal((await maintainAuthority(store, noop, now)).more, true);
  assert.equal((await maintainAuthority(store, noop, now)).more, true);
  for (let i = 0; i < 4; i++) await maintainAuthority(store, noop, now + 1);
  assert.equal((await maintainAuthority(store, noop, now + 1)).more, false);
  assert(![...store.data.keys()].some(key => key.startsWith('pairing:')));
});

test('revoked receipts remain replay tombstones while failed cleanup backs off and later succeeds', async t => {
  const store = await managed(t);
  assert.equal(await claimAuthorizationGrant(store, identity, permission, auth), false);
  let calls = 0;
  const failing = async () => {
    calls++;
    assert.equal(await authorizationGrantActive(store, identity, permission), false);
    throw new Error('private backend error');
  };
  await maintainAuthority(store, failing, now);
  assert.equal(calls, 1);
  await maintainAuthority(store, failing, now + 1);
  assert.equal(calls, 1);
  await maintainAuthority(store, async () => { calls++; }, now + 60_000);
  assert.equal(calls, 2);
  const receipt = store.data.get(`oauth-grant:${id}:${grantId}`) as any;
  assert.equal(receipt.cleanupPending, false);
  assert.equal(receipt.active, false);
  assert.equal(await claimAuthorizationGrant(store, identity, permission, auth), false);
  assert(!JSON.stringify(receipt).includes('private backend error'));
});

test('expiry, device rotation and replacement consent schedule cleanup, not offline tunnel expiry', async t => {
  const store = await managed(t);
  let calls = 0;
  const clean = async () => { calls++; };
  await maintainAuthority(store, clean, now + 2000);
  assert.equal(calls, 0, 'offline tunnel is recoverable');
  const route = store.data.get(`connector:${id}`) as any;
  store.data.set(`connector:${id}`, { ...route, generation: 1 });
  await maintainAuthority(store, clean, now + 2000);
  assert.equal(calls, 1);
  assert.equal(await authorizationGrantActive(store, identity, permission), false);
});

test('replacement consent and device expiry invalidate receipts before external cleanup', async t => {
  const store = await managed(t);
  await replaceClientAuthorization(store, id, identity.clientId, 'b'.repeat(64));
  let cleaned = 0;
  await maintainAuthority(store, async () => {
    assert.equal(await authorizationGrantActive(store, identity, permission), false);
    cleaned++;
  }, now);
  assert.equal(cleaned, 1);
  const next = { ...identity, grantId: 'h'.repeat(16) };
  await claimAuthorizationGrant(store, next, permission, 'b'.repeat(64));
  await maintainAuthority(store, async () => {
    assert.equal(await authorizationGrantActive(store, next, permission), false);
    cleaned++;
  }, now + 100_000);
  assert.equal(cleaned, 2);
});

test('partial batches continue without exponential failure backoff', async t => {
  const store = await managed(t);
  await claimAuthorizationGrant(store, identity, permission, auth);
  let calls = 0;
  for (let minute = 0; minute < 4; minute++) await maintainAuthority(store, async () => {
    calls++;
    throw new CleanupBatchIncomplete();
  }, now + minute * 60_000);
  assert.equal(calls, 4);
  assert.equal((store.data.get(`oauth-grant:${id}:${grantId}`) as any).cleanupFailures, 0);
});

test('only one external grant cleanup runs per pass and live consent prevents tombstone deletion', async t => {
  const store = await managed(t);
  const key = `oauth-grant:${id}:${grantId}`;
  const receipt = store.data.get(key) as any;
  store.data.set(key, { ...receipt, active: false, expiresAt: now, cleanupPending: false });
  await maintainAuthority(store, noop, now);
  assert(store.data.has(key), 'still-issuable consent must keep the replay tombstone');
  store.data.set(await clientKey(id, identity.clientId), { authorizationId: auth, expiresAt: now });
  await maintainAuthority(store, noop, now);
  assert(!store.data.has(key));
  for (let i = 0; i < 4; i++) store.data.set(`${key}${i}`, { ...receipt, id: `${grantId}${i}`, active: false });
  let calls = 0;
  await maintainAuthority(store, async () => { calls++; }, now);
  assert.equal(calls, 1);
});
