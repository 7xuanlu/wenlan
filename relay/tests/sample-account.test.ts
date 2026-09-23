// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { approveSamplePairing, issueSampleAccount, parseSampleAccount, prepareNewDeviceSampleAccount } from '../src/sample-account.ts';
import { authenticateDevice, enrollDevice, refreshDevice, revokeDevice, rotateDeviceCredential } from '../src/devices.ts';
import { beginPairing, cancelPairing, consumePairing, connectorKey, type PairingStore } from '../src/pairing.ts';
import { MemoryStore } from './fixtures/memory-store.ts';

const resource = 'https://wenlan-relay.example/mcp';
const candidate = { tunnelOrigin: 'https://synthetic.trycloudflare.com', backendToken: 'b'.repeat(64), space: 'atlas-review' };
const now = 1000;
const clock = () => now;
const backend = (space = candidate.space) => (async (_input: unknown, init?: RequestInit) =>
  new Headers(init?.headers).has('authorization')
    ? Response.json({ contract_version: 1, server: 'wenlan-mcp', tool_profile: 'query-only', authentication: 'bearer', space })
    : new Response(null, { status: 401 })) as typeof fetch;

test('offline preparation uses fresh enrollment but still requires live matching authority', async () => {
  const { store, device, input } = await setup();
  const options = { username: 'reviewer', resource, space: candidate.space };
  const prepared = await prepareNewDeviceSampleAccount(device, options, clock);
  assert.ok(prepared);
  assert.equal(prepared.account.generation, 0);
  assert.equal(prepared.account.expiresAt, device.expiresAt);
  assert.notEqual(prepared.password, device.managementToken);
  const login = { ...input, password: prepared.password };
  const forged = await prepareNewDeviceSampleAccount({ ...device, managementToken: 'f'.repeat(64) }, options, clock);
  assert.ok(forged, 'offline preparation does not claim remote authentication');
  const before = JSON.stringify([...store.data]);
  assert.equal(await approveSamplePairing(store, forged.account, { ...login, password: forged.password }, clock), false);
  assert.equal(JSON.stringify([...store.data]), before);
  assert.equal(await approveSamplePairing(store, prepared.account, login, clock), true);
});

test('offline preparation rejects malformed enrollment and bounds expiry without reviving changed devices', async () => {
  const { store, device, input } = await setup();
  const options = { username: 'reviewer', resource, space: candidate.space };
  for (const enrollment of [null, [], {}, { ...device, extra: true }, { ...device, managementToken: 'short' },
    { ...device, expiresAt: now }, { ...device, expiresAt: NaN }, { ...device, id: '' }]) {
    assert.equal(await prepareNewDeviceSampleAccount(enrollment, options, clock), null);
  }
  for (const delta of [{ space: '' }, { space: ' padded ' }, { username: '' }, { resource: 'http://local/mcp' }]) {
    assert.equal(await prepareNewDeviceSampleAccount(device, { ...options, ...delta }, clock), null);
  }
  const capped = await prepareNewDeviceSampleAccount({ ...device, expiresAt: now + 90 * 86_400_000 }, options, clock);
  assert.equal(capped?.account.expiresAt, now + 30 * 86_400_000);
  await refreshDevice(store, device.id, device.managementToken, { ...candidate, space: 'changed' }, { fetch: backend('changed'), clock });
  const stale = await prepareNewDeviceSampleAccount(device, { ...options, space: 'changed' }, clock);
  assert.ok(stale);
  assert.equal(await approveSamplePairing(store, stale.account, { ...input, space: 'changed', password: stale.password }, clock), false);
  await revokeDevice(store, device.id, device.managementToken, clock);
  assert.equal(await approveSamplePairing(store, stale.account, { ...input, space: 'changed', password: stale.password }, clock), false);
});

async function setup() {
  const store = new MemoryStore();
  const device = await enrollDevice(store, candidate, { fetch: backend(), clock });
  assert.ok(device);
  const before = JSON.stringify([...store.data]);
  const issued = await issueSampleAccount(store, device.id, device.managementToken,
    { username: 'reviewer', resource, expiresAt: 50_000 }, clock);
  assert.ok(issued);
  assert.equal(JSON.stringify([...store.data]), before, 'provisioning must not enroll or alter devices');
  const pair = await beginPairing(store, { authorizationId: 'a'.repeat(64), clientId: 'synthetic-client',
    resource, scopes: ['wenlan:query'] }, resource, now);
  return { store, device, issued, pair, input: { username: issued.account.username, password: issued.password,
    pairingId: pair.pairingId, browserSecret: pair.browserSecret, approved: true,
    clientId: 'synthetic-client', resource, space: candidate.space } };
}

test('generated sample credentials are separate and approve only a live consented pairing', async () => {
  const { store, device, issued, pair, input } = await setup();
  assert.match(issued.password, /^[a-f0-9]{64}$/);
  assert.notEqual(issued.password, device.managementToken);
  assert.notEqual(issued.password, candidate.backendToken);
  assert(!JSON.stringify(issued.account).includes(issued.password));
  assert(!JSON.stringify([...store.data]).includes(issued.password));
  assert.equal(await authenticateDevice(store, device.id, issued.password, clock), null);
  assert.equal(await consumePairing(store, pair.pairingId, pair.browserSecret, clock), null);
  assert.equal(await approveSamplePairing(store, issued.account, input, clock), true);
  const consumed = await consumePairing(store, pair.pairingId, pair.browserSecret, clock);
  assert.equal(consumed?.grant.connectorId, device.id);
  assert.equal(consumed?.grant.space, candidate.space);
  assert.deepEqual(consumed?.grant.scopes, ['wenlan:query']);
  assert.equal(consumed?.clientId, input.clientId);
  assert.equal(await approveSamplePairing(store, issued.account, input, clock), false);
  assert.equal(await consumePairing(store, pair.pairingId, pair.browserSecret, clock), null);
});

test('wrong login, cookie, consent and scope cannot mutate a pairing', async () => {
  const { store, issued, input } = await setup();
  const before = JSON.stringify([...store.data]);
  for (const delta of [
    { username: 'other' }, { password: 'c'.repeat(64) }, { password: 'short-password' },
    { password: issued.account.passwordHash }, { approved: false },
    { browserSecret: 'd'.repeat(64) }, { pairingId: 'e'.repeat(64) },
    { clientId: 'other-client' }, { space: 'private' }, { resource: 'https://other.example/mcp' },
  ]) {
    assert.equal(await approveSamplePairing(store, issued.account, { ...input, ...delta }, clock), false);
    assert.equal(JSON.stringify([...store.data]), before);
  }
  assert.equal(await approveSamplePairing(store, { ...issued.account, username: 'renamed' }, input, clock), false);
  assert.equal(await approveSamplePairing(store, issued.account, input, clock), true);
});

test('provisioning requires live device ownership and bounded account lifetime', async () => {
  const { store, device } = await setup();
  const options = { username: 'reviewer', resource, expiresAt: 50_000 };
  assert.equal(await issueSampleAccount(store, device.id, candidate.backendToken, options, clock), null);
  for (const delta of [
    { username: '' }, { username: 'reviewer@example.com' }, { expiresAt: now },
    { expiresAt: NaN }, { expiresAt: now + 31 * 86_400_000 },
    { resource: 'http://wenlan-relay.example/mcp' },
    { resource: 'https://user:secret@wenlan-relay.example/mcp' },
    { resource: `${resource}?token=secret` }, { resource: `${resource}#ignored` },
  ]) assert.equal(await issueSampleAccount(store, device.id, device.managementToken, { ...options, ...delta }, clock), null);
  assert.equal(await issueSampleAccount(store, device.id, device.managementToken,
    { ...options, expiresAt: 86_405_000 }, () => 86_401_001), null);
  store.data.set(connectorKey(device.id), { ...store.data.get(connectorKey(device.id)) as object, expiresAt: Infinity });
  assert.equal(await issueSampleAccount(store, device.id, device.managementToken, options, clock), null);
});

test('malformed or disabled account bindings fail closed', async () => {
  const { store, issued, input } = await setup();
  for (const value of [undefined, null, [], 'account', {}, { ...issued.account, extra: true },
    { ...issued.account, passwordHash: 'not-a-digest' }, { ...issued.account, generation: -1 },
    { ...issued.account, generation: 0.5 }, { ...issued.account, space: '' },
    { ...issued.account, expiresAt: Infinity }, { ...issued.account, resource: 'https://elsewhere.example/not-mcp' }]) {
    assert.equal(parseSampleAccount(value), null);
    assert.equal(await approveSamplePairing(store, value, input, clock), false);
  }
  assert.equal(await approveSamplePairing(store, { ...issued.account, expiresAt: now }, input, clock), false);
});

test('revocation, rotation, route expiry and scope change invalidate sample authority', async t => {
  for (const action of ['revoke', 'rotate', 'scope', 'expired', 'other-device'] as const) {
    await t.test(action, async () => {
      const { store, device, issued, input } = await setup();
      if (action === 'revoke') await revokeDevice(store, device.id, device.managementToken, clock);
      if (action === 'rotate') await rotateDeviceCredential(store, device.id, device.managementToken, clock);
      if (action === 'scope') await refreshDevice(store, device.id, device.managementToken,
        { ...candidate, space: 'private' }, { fetch: backend('private'), clock });
      if (action === 'expired') store.data.set(connectorKey(device.id),
        { ...store.data.get(connectorKey(device.id)) as object, expiresAt: now });
      if (action === 'other-device') {
        const other = await enrollDevice(store, candidate, { fetch: backend(), clock });
        assert.ok(other);
        issued.account.deviceId = other.id;
      }
      assert.equal(await approveSamplePairing(store, issued.account, input, clock), false);
    });
  }
});

test('same-boundary tunnel renewal preserves sample login without granting management', async () => {
  const { store, device, issued, input } = await setup();
  assert.equal(await refreshDevice(store, device.id, device.managementToken,
    { ...candidate, tunnelOrigin: 'https://replacement.trycloudflare.com' }, { fetch: backend(), clock }), true);
  assert.equal(await approveSamplePairing(store, issued.account, input, clock), true);
  assert.equal(await revokeDevice(store, device.id, issued.password, clock), false);
});

test('state changes after login authentication cannot race pairing approval', async t => {
  for (const action of ['revoke', 'rotate', 'scope', 'cancel', 'expire'] as const) await t.test(action, async () => {
    const { store, device, issued, input } = await setup();
    let transactions = 0;
    let time = now;
    const racing: PairingStore = {
      async transaction(operation) {
        transactions++;
        if (transactions === 3) {
          if (action === 'revoke') await revokeDevice(store, device.id, device.managementToken, () => time);
          if (action === 'rotate') await rotateDeviceCredential(store, device.id, device.managementToken, () => time);
          if (action === 'scope') await refreshDevice(store, device.id, device.managementToken,
            { ...candidate, space: 'private' }, { fetch: backend('private'), clock: () => time });
          if (action === 'cancel') await cancelPairing(store, input.pairingId, input.browserSecret, time);
          if (action === 'expire') time = issued.account.expiresAt;
        }
        return store.transaction(operation);
      },
    };
    assert.equal(await approveSamplePairing(racing, issued.account, input, () => time), false);
    assert.equal(transactions, 3, 'interleave between identity read and the approval write');
    assert.equal(await consumePairing(store, input.pairingId, input.browserSecret, () => time), null);
  });
});

test('one pairing cannot be approved concurrently twice', async () => {
  const { store, issued, input } = await setup();
  const results = await Promise.all([
    approveSamplePairing(store, issued.account, input, clock),
    approveSamplePairing(store, issued.account, input, clock),
  ]);
  assert.deepEqual(results.sort(), [false, true]);
});
