// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { beginPairing, inspectPairing, approvePairing, consumePairing, cancelPairing,
  browserPairingView, connectorKey, PAIRING_TTL_MS } from '../src/pairing.ts';
import type { ConnectorRoute } from '../src/proxy.ts';
import { MemoryStore } from './fixtures/memory-store.ts';
const resource = 'https://wenlan-relay.example/mcp';
const intent = { authorizationId: 'i'.repeat(43), clientId: 'verified-client', resource, scopes: ['wenlan:query'] };
const device = { id: 'd'.repeat(43), subject: 'device-owner', generation: 3, credentialExpiresAt: 1_000_000 };
const route: ConnectorRoute = { id: device.id, subject: device.subject, space: 'review',
  generation: 3, enabled: true, expiresAt: 1_000_000,
  tunnelOrigin: 'https://fixture.trycloudflare.com', backendToken: 'b'.repeat(43) };
const consent = { clientId: intent.clientId, resource, space: route.space };
const clock = () => 1000;
async function setup() {
  const store = new MemoryStore();
  store.data.set(connectorKey(device.id), route);
  const pair = await beginPairing(store, intent, resource, 1000);
  return { store, pair };
}

test('binding uses server-owned intent and requires desktop approval', async () => {
  const {store, pair} = await setup();
  assert.notEqual(pair.pairingId, pair.browserSecret);
  assert.equal(pair.expiresAt, 1000 + PAIRING_TTL_MS);
  assert.equal(await consumePairing(store, pair.pairingId, pair.browserSecret, clock), null);
  const view = await inspectPairing(store, pair.pairingId, 1000);
  assert.deepEqual(view, {pairingId: pair.pairingId, clientId: intent.clientId,
    resource, scopes: ['wenlan:query'], expiresAt: pair.expiresAt});
  assert(!JSON.stringify([...store.data.values()]).includes(pair.browserSecret));
  assert.equal(await approvePairing(store, pair.pairingId, device, consent, clock), true);
  assert.deepEqual(await consumePairing(store, pair.pairingId, pair.browserSecret, clock), {
    authorizationId: intent.authorizationId, clientId: intent.clientId, resource,
    grant: {subject: device.subject, connectorId: device.id, space: route.space,
      generation: 3, scopes: ['wenlan:query']},
  });
});

test('authorization cannot be rebound to a second pairing', async () => {
  const {store} = await setup();
  await assert.rejects(beginPairing(store, intent, resource, 1000), /already paired/);
});

test('authenticated browser status becomes unavailable exactly at expiry', async () => {
  for (const approved of [false, true]) {
    const { store, pair } = await setup();
    if (approved) await approvePairing(store, pair.pairingId, device, consent, clock);
    const view = await browserPairingView(store, pair.pairingId, pair.browserSecret, () => pair.expiresAt - 1);
    assert.equal(view?.status, approved ? 'approved' : 'pending');
    assert.equal(await browserPairingView(store, pair.pairingId, pair.browserSecret, () => pair.expiresAt), null);
    assert.equal(await browserPairingView(store, pair.pairingId, 'x'.repeat(64), clock), null);
  }
});

test('unsupported resource/scopes do not create pending state', async () => {
  for (const bad of [{...intent, resource: 'https://other.example/mcp'},
    {...intent, scopes: ['wenlan:query', 'write']}, {...intent, scopes: []},
    {...intent, authorizationId: '../bad'}]) {
    const store = new MemoryStore();
    await assert.rejects(beginPairing(store, bad, resource, 1000));
    assert.equal(store.data.size, 0);
  }
});

test('client/resource/Space swaps and forged device identity cannot approve', async () => {
  for (const altered of [{clientId: 'attacker'}, {resource: 'https://attacker.example/mcp'}, {space: 'private'}]) {
    const {store, pair} = await setup();
    assert.equal(await approvePairing(store, pair.pairingId, device, {...consent, ...altered}, clock), false);
  }
  const {store, pair} = await setup();
  assert.equal(await approvePairing(store, pair.pairingId, {...device, subject: 'other'}, consent, clock), false);
  assert.equal(await approvePairing(store, pair.pairingId, {...device, id: 'x'.repeat(43)}, consent, clock), false);
});

test('browser secret is required, not the visible pairing id', async () => {
  const {store, pair} = await setup();
  await approvePairing(store, pair.pairingId, device, consent, clock);
  for (const secret of [pair.pairingId, 'x'.repeat(64), '']) {
    assert.equal(await consumePairing(store, pair.pairingId, secret, clock), null);
    assert.equal(await cancelPairing(store, pair.pairingId, secret, 1000), false);
  }
  assert.ok(await consumePairing(store, pair.pairingId, pair.browserSecret, clock));
});

test('simultaneous approvals and consumption each succeed at most once', async () => {
  const {store, pair} = await setup();
  const approvals = await Promise.all(Array.from({length: 8}, () =>
    approvePairing(store, pair.pairingId, device, consent, clock)));
  assert.equal(approvals.filter(Boolean).length, 1);
  const grants = await Promise.all(Array.from({length: 8}, () =>
    consumePairing(store, pair.pairingId, pair.browserSecret, clock)));
  assert.equal(grants.filter(Boolean).length, 1);
});

test('expiry, cancellation and revocation cannot yield an authorization', async () => {
  for (const change of ['expiry', 'cancel', 'disabled', 'generation', 'space', 'owner', 'route-expiry']) {
    const {store, pair} = await setup();
    await approvePairing(store, pair.pairingId, device, consent, clock);
    if (change === 'cancel') assert.equal(await cancelPairing(store, pair.pairingId, pair.browserSecret, 1000), true);
    const modified = {...route};
    if (change === 'disabled') modified.enabled = false;
    if (change === 'generation') modified.generation++;
    if (change === 'space') modified.space = 'private';
    if (change === 'owner') modified.subject = 'other';
    if (change === 'route-expiry') modified.expiresAt = 1000;
    store.data.set(connectorKey(device.id), modified);
    assert.equal(await consumePairing(store, pair.pairingId, pair.browserSecret,
      () => change === 'expiry' ? pair.expiresAt : 1000), null, change);
  }
});

test('expiry after transaction queue wait is checked at execution time', async () => {
  const {store, pair} = await setup();
  await approvePairing(store, pair.pairingId, device, consent, clock);
  let now = 1000;
  const blocker = store.transaction(async () => { now = pair.expiresAt; });
  const result = consumePairing(store, pair.pairingId, pair.browserSecret, () => now);
  await blocker;
  assert.equal(await result, null);
});

test('expired identity and nonfinite route expiry cannot approve', async () => {
  for (const expiry of [NaN, Infinity, -Infinity]) {
    const {store, pair} = await setup();
    store.data.set(connectorKey(device.id), { ...route, expiresAt: expiry });
    assert.equal(await approvePairing(store, pair.pairingId, device, consent, clock), false);
  }
  const {store, pair} = await setup();
  assert.equal(await approvePairing(store, pair.pairingId,
    { ...device, credentialExpiresAt: 1000 }, consent, clock), false);
});
