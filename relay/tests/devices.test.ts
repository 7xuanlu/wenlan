// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { enrollDevice, authenticateDevice, refreshDevice, revokeDevice, rotateDeviceCredential } from '../src/devices.ts';
import { beginPairing, approvePairing, consumePairing, connectorKey } from '../src/pairing.ts';
import type { ConnectorRoute } from '../src/proxy.ts';
import { maintainAuthority } from '../src/cleanup.ts';
import { MemoryStore } from './fixtures/memory-store.ts';

const candidate = { tunnelOrigin: 'https://synthetic.trycloudflare.com', backendToken: 'b'.repeat(43), space: 'review' };
const clock = () => 1000;
function backend(space = candidate.space): typeof fetch {
  return (async (_input, init) => new Headers(init?.headers).has('authorization')
    ? Response.json({ contract_version: 1, server: 'wenlan-mcp', tool_profile: 'query-only', authentication: 'bearer', space })
    : new Response(null, { status: 401 })) as typeof fetch;
}
async function setup() {
  const store = new MemoryStore();
  const credential = await enrollDevice(store, candidate, { fetch: backend(), clock });
  assert.ok(credential);
  return { store, credential };
}
async function pairing(store: MemoryStore) {
  const intent = { authorizationId: 'a'.repeat(43), clientId: 'synthetic-client',
    resource: 'https://wenlan-relay.example/mcp', scopes: ['wenlan:query'] };
  const pair = await beginPairing(store, intent, intent.resource, 1000);
  return { pair, consent: { clientId: intent.clientId, resource: intent.resource, space: candidate.space } };
}

test('enrollment creates separate hashed management credentials, not an email identity', async () => {
  const { store, credential } = await setup();
  assert.notEqual(credential.managementToken, candidate.backendToken);
  assert.notEqual(credential.managementToken, credential.id);
  assert(!JSON.stringify([...store.data.values()]).includes(credential.managementToken));
  const identity = await authenticateDevice(store, credential.id, credential.managementToken, clock);
  assert.equal(identity?.subject, credential.id);
  for (const token of [candidate.backendToken, credential.id, 'wrong']) {
    assert.equal(await authenticateDevice(store, credential.id, token, clock), null);
  }
});

test('failed connector checks create no enrollment state', async () => {
  const store = new MemoryStore();
  assert.equal(await enrollDevice(store, candidate, { fetch: backend('other'), clock }), null);
  assert.equal(store.data.size, 0);
});

test('conditional renewal rejects stale generations and boundary changes without probing', async () => {
  const { store, credential: c } = await setup();
  const before = structuredClone([...store.data]);
  let probes = 0;
  for (const [next, expectedGeneration] of [
    [candidate, 1], [candidate, -1], [candidate, NaN],
    [{ ...candidate, space: 'other' }, 0],
    [{ ...candidate, backendToken: 'c'.repeat(43) }, 0],
  ] as const) {
    assert.equal(await refreshDevice(store, c.id, c.managementToken, next, {
      clock, expectedGeneration, fetch: (async (...args: Parameters<typeof fetch>) => {
        probes++; return backend()(...args);
      }) as typeof fetch,
    }), false);
  }
  assert.equal(probes, 0);
  assert.deepEqual([...store.data], before);
  assert.equal(await refreshDevice(store, c.id, c.managementToken,
    { ...candidate, tunnelOrigin: 'https://replacement.trycloudflare.com' },
    { clock, expectedGeneration: 0, fetch: backend() }), true);
  assert.equal((store.data.get(connectorKey(c.id)) as ConnectorRoute).generation, 0);
});

test('conditional renewal cannot overwrite a concurrent scope change during probe', async () => {
  const { store, credential: c } = await setup();
  let changed = false;
  const fetcher = (async (...args: Parameters<typeof fetch>) => {
    if (!changed) {
      changed = true;
      assert.equal(await refreshDevice(store, c.id, c.managementToken,
        { ...candidate, space: 'other' }, { clock, fetch: backend('other') }), true);
    }
    return backend()(...args);
  }) as typeof fetch;
  assert.equal(await refreshDevice(store, c.id, c.managementToken, candidate,
    { clock, expectedGeneration: 0, fetch: fetcher }), false);
  assert.equal((store.data.get(connectorKey(c.id)) as ConnectorRoute).space, 'other');
});

test('unauthenticated updates never contact the supplied tunnel', async () => {
  const { store, credential } = await setup();
  let called = false;
  assert.equal(await refreshDevice(store, credential.id, candidate.backendToken, candidate, {
    clock, fetch: (async () => { called = true; throw new Error('must not probe'); }) as typeof fetch,
  }), false);
  assert.equal(called, false);
});

test('tunnel refresh preserves grants; scope change requires new consent', async () => {
  const { store, credential: c } = await setup();
  const identity = await authenticateDevice(store, c.id, c.managementToken, clock);
  assert.ok(identity);
  const { pair, consent } = await pairing(store);
  assert.equal(await approvePairing(store, pair.pairingId, identity, consent, clock), true);
  assert.equal(await refreshDevice(store, c.id, c.managementToken,
    { ...candidate, tunnelOrigin: 'https://replacement.trycloudflare.com' }, { fetch: backend(), clock }), true);
  assert.equal((store.data.get(connectorKey(c.id)) as ConnectorRoute).generation, 0);
  assert.equal(await refreshDevice(store, c.id, c.managementToken,
    { ...candidate, space: 'other' }, { fetch: backend('other'), clock }), true);
  assert.equal(await consumePairing(store, pair.pairingId, pair.browserSecret, clock), null);
});

test('revocation during the connector probe prevents a late refresh', async () => {
  const { store, credential: c } = await setup();
  let revoked = false;
  const fetcher = (async (...args: Parameters<typeof fetch>) => {
    if (!revoked) {
      revoked = true;
      assert.equal(await revokeDevice(store, c.id, c.managementToken, clock), true);
    }
    return backend()(...args);
  }) as typeof fetch;
  assert.equal(await refreshDevice(store, c.id, c.managementToken, candidate, { fetch: fetcher, clock }), false);
  assert.equal(await authenticateDevice(store, c.id, c.managementToken, clock), null);
});

test('device revocation retries survive a lost response and authority cleanup', async () => {
  const { store, credential: c } = await setup();
  assert.equal(await revokeDevice(store, c.id, c.managementToken, clock), true);
  assert.equal(await revokeDevice(store, c.id, c.managementToken, clock), true);
  assert.equal(await revokeDevice(store, c.id, 'x'.repeat(43), clock), false);
  await maintainAuthority(store, async () => {}, clock());
  assert.equal(store.data.has(`device:${c.id}`), false);
  assert.equal(store.data.has(connectorKey(c.id)), false);
  assert.equal(await revokeDevice(store, c.id, c.managementToken, clock), true);
  assert.equal(await authenticateDevice(store, c.id, c.managementToken, clock), null);
  assert.equal(await refreshDevice(store, c.id, c.managementToken, candidate,
    { fetch: backend(), clock }), false);
});

test('expired credentials can only close their own existing device', async () => {
  const { store, credential: c } = await setup();
  const expired = () => c.expiresAt + 1;
  assert.equal(await revokeDevice(store, c.id, 'x'.repeat(43), expired), false);
  assert.equal(await revokeDevice(store, c.id, c.managementToken, expired), true);
  assert.equal((store.data.get(connectorKey(c.id)) as ConnectorRoute).enabled, false);
  assert.equal(await authenticateDevice(store, c.id, c.managementToken, expired), null);
  assert.equal(await rotateDeviceCredential(store, c.id, c.managementToken, expired), null);
});

test('revoke cannot use a rotated credential or clear inconsistent ownership', async () => {
  const { store, credential: c } = await setup();
  const next = await rotateDeviceCredential(store, c.id, c.managementToken, clock);
  assert.ok(next);
  assert.equal(await revokeDevice(store, c.id, c.managementToken, clock), false);
  assert.ok(await authenticateDevice(store, c.id, next.managementToken, clock));
  const route = store.data.get(connectorKey(c.id)) as ConnectorRoute;
  store.data.set(connectorKey(c.id), { ...route, subject: 'different-owner' });
  assert.equal(await revokeDevice(store, c.id, next.managementToken, clock), false);
  store.data.delete(`device:${c.id}`);
  assert.equal(await revokeDevice(store, c.id, next.managementToken, clock), false);
  assert.equal((store.data.get(connectorKey(c.id)) as ConnectorRoute).enabled, true);
});

test('revoke closes a matching device even after its route was removed', async () => {
  const { store, credential: c } = await setup();
  store.data.delete(connectorKey(c.id));
  assert.equal(await revokeDevice(store, c.id, c.managementToken, clock), true);
  assert.equal((store.data.get(`device:${c.id}`) as { enabled: boolean }).enabled, false);
});

test('concurrent credential rotations have one winner and invalidate old credentials', async () => {
  const { store, credential: c } = await setup();
  const results = await Promise.all(Array.from({ length: 8 }, () => rotateDeviceCredential(store, c.id, c.managementToken, clock)));
  const winners = results.filter(result => result !== null);
  assert.equal(winners.length, 1);
  assert.equal(await authenticateDevice(store, c.id, c.managementToken, clock), null);
  assert.ok(await authenticateDevice(store, c.id, winners[0].managementToken, clock));
});

test('identity authenticated before rotation cannot approve afterward', async () => {
  const { store, credential: c } = await setup();
  const staleIdentity = await authenticateDevice(store, c.id, c.managementToken, clock);
  assert.ok(staleIdentity);
  const { pair, consent } = await pairing(store);
  assert.ok(await rotateDeviceCredential(store, c.id, c.managementToken, clock));
  assert.equal(await approvePairing(store, pair.pairingId, staleIdentity, consent, clock), false);
});

test('expired management credentials cannot refresh or rotate', async () => {
  const { store, credential: c } = await setup();
  const expired = () => c.expiresAt;
  assert.equal(await authenticateDevice(store, c.id, c.managementToken, expired), null);
  assert.equal(await refreshDevice(store, c.id, c.managementToken, candidate, { fetch: backend(), clock: expired }), false);
  assert.equal(await rotateDeviceCredential(store, c.id, c.managementToken, expired), null);
});

test('renewing an expired route preserves management lifetime and consent generation', async () => {
  const { store, credential: c } = await setup();
  const before = structuredClone(store.data.get(`device:${c.id}`));
  const route = store.data.get(connectorKey(c.id)) as ConnectorRoute;
  const wake = () => route.expiresAt + 1;
  assert.ok(await authenticateDevice(store, c.id, c.managementToken, wake));
  assert.equal(await refreshDevice(store, c.id, c.managementToken, candidate,
    { fetch: backend(), clock: wake }), true);
  const renewed = store.data.get(connectorKey(c.id)) as ConnectorRoute;
  assert.equal(renewed.expiresAt, wake() + 24 * 60 * 60 * 1000);
  assert.equal(renewed.generation, route.generation);
  assert.deepEqual(store.data.get(`device:${c.id}`), { ...(before as object), revision: 1 });
});

test('refresh cannot overwrite server-owned route identity through extra fields', async () => {
  const { store, credential: c } = await setup();
  const hostile = { ...candidate, id: 'x'.repeat(43), subject: 'other-owner', enabled: false };
  assert.equal(await refreshDevice(store, c.id, c.managementToken, hostile, { fetch: backend(), clock }), true);
  const saved = store.data.get(connectorKey(c.id)) as ConnectorRoute;
  assert.equal(saved.id, c.id);
  assert.equal(saved.subject, c.id);
  assert.equal(saved.enabled, true);
});
