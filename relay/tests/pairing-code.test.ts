// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { beginPairing, approvePairing, consumePairing, cancelPairingRequest, denyPairing, lookupPairing,
  inspectPairing, browserPairingView, connectorKey, generateUserCode, normalizeUserCode, formatUserCode,
  PAIRING_TTL_MS, USER_CODE_ALPHABET, USER_CODE_LENGTH, LOOKUP_MISS_LIMIT, LOOKUP_MISS_WINDOW_MS } from '../src/pairing.ts';
import type { ConnectorRoute } from '../src/proxy.ts';
import { MemoryStore } from './fixtures/memory-store.ts';

const resource = 'https://wenlan-relay.example/mcp';
const device = { id: 'd'.repeat(43), subject: 'device-owner', generation: 3, credentialExpiresAt: 10_000_000 };
const other = 'o'.repeat(43);
const route: ConnectorRoute = { id: device.id, subject: device.subject, space: 'review', generation: 3, enabled: true,
  expiresAt: 10_000_000, tunnelOrigin: 'https://fixture.trycloudflare.com', backendToken: 'b'.repeat(43) };
let serial = 0;
const intent = () => ({ authorizationId: String(++serial).padStart(43, 'i'), clientId: 'verified-client', resource,
  scopes: ['wenlan:query'] });
const claude = { redirectUri: 'https://claude.ai/api/mcp/auth_callback', clientName: 'Claude' };

async function setup(client: { redirectUri?: string; clientName?: string } = claude, now = 1000) {
  const store = new MemoryStore();
  store.data.set(connectorKey(device.id), route);
  const pair = await beginPairing(store, intent(), resource, now, client);
  return { store, pair };
}
const codeKeys = (store: MemoryStore) => [...store.data.keys()].filter(key => key.startsWith('pairing-code:'));

test('pairing TTL is ten minutes and the code expires with it', async () => {
  assert.equal(PAIRING_TTL_MS, 600_000);
  const { store, pair } = await setup();
  assert.equal(pair.expiresAt, 1000 + 600_000);
  assert.equal((await lookupPairing(store, device.id, pair.userCode, pair.expiresAt - 1)).status, 'found');
  assert.deepEqual(await lookupPairing(store, device.id, pair.userCode, pair.expiresAt), { status: 'missing' });
  assert.equal(await inspectPairing(store, pair.pairingId, pair.expiresAt), null);
});

test('user codes are 8 characters from the unambiguous alphabet', () => {
  assert.equal(USER_CODE_ALPHABET, '23456789ABCDEFGHJKMNPQRSTVWXYZ');
  const seen = new Set<string>();
  for (let n = 0; n < 500; n++) {
    const code = generateUserCode();
    assert.equal(code.length, USER_CODE_LENGTH);
    for (const character of code) assert(USER_CODE_ALPHABET.includes(character), code);
    seen.add(code);
  }
  assert(seen.size > 490, 'codes are random');
  // Bytes >= 240 are rejected so the mapping stays uniform.
  let calls = 0;
  const forced = generateUserCode(size => { calls++; return new Uint8Array(size).fill(calls === 1 ? 255 : 0); });
  assert.equal(forced, '22222222');
  assert.equal(calls, 2);
  assert.equal(formatUserCode('ABCD2345'), 'ABCD-2345');
});

test('code normalization accepts dashes, spaces and lowercase and rejects anything else', () => {
  assert.equal(normalizeUserCode('abcd-2345'), 'ABCD2345');
  assert.equal(normalizeUserCode(' ab cd 23 45 '), 'ABCD2345');
  assert.equal(normalizeUserCode('ABCD2345'), 'ABCD2345');
  for (const bad of ['ABCD234', 'ABCD23456', 'ABCD-23I5', 'ABCD-2310', 'ABCD-23O5', 'ABCDU345', '', 'x'.repeat(40), null, 12345678]) {
    assert.equal(normalizeUserCode(bad), null, String(bad));
  }
});

test('lookup hit returns the inspect body including pairingId and identity', async () => {
  const { store, pair } = await setup();
  assert.match(pair.userCode, /^[23456789ABCDEFGHJKMNPQRSTVWXYZ]{8}$/);
  const dashed = `${pair.userCode.slice(0, 4)}-${pair.userCode.slice(4)}`.toLowerCase();
  const found = await lookupPairing(store, device.id, dashed, 2000);
  assert.equal(found.status, 'found');
  assert.deepEqual(found.status === 'found' && found.view, await inspectPairing(store, pair.pairingId, 2000));
  assert.deepEqual(found.status === 'found' && found.view, { pairingId: pair.pairingId, clientId: 'verified-client',
    resource, scopes: ['wenlan:query'], expiresAt: pair.expiresAt,
    clientName: 'Claude', redirectHost: 'claude.ai', knownClient: true });
  assert.deepEqual(await lookupPairing(store, device.id, ` ${pair.userCode.toLowerCase()} `, 2000),
    found, 'space and lowercase also hit');
  assert.equal(store.data.has(`pairing-lookup-miss:${device.id}`), false, 'a hit is not counted');
});

test('lookup misses are limited per device: the eleventh lookup is refused with a retry delay', async () => {
  const { store, pair } = await setup();
  for (let n = 0; n < LOOKUP_MISS_LIMIT; n++) {
    const wrong = n % 2 ? 'nonsense' : '22222222';
    assert.deepEqual(await lookupPairing(store, device.id, wrong, 2000 + n), { status: 'missing' }, `miss ${n + 1}`);
  }
  const limited = await lookupPairing(store, device.id, pair.userCode, 3000);
  assert.equal(limited.status, 'limited', 'even a correct code is refused once limited');
  assert.equal(limited.status === 'limited' && limited.retryAfter, Math.ceil((2000 + LOOKUP_MISS_WINDOW_MS - 3000) / 1000));
  assert.equal((await lookupPairing(store, other, pair.userCode, 3000)).status, 'found', 'another device has its own budget');
  const reopened = 2000 + LOOKUP_MISS_WINDOW_MS;
  const later = await beginPairing(store, intent(), resource, reopened - 100, claude);
  assert.equal((await lookupPairing(store, device.id, later.userCode, reopened - 1)).status, 'limited');
  assert.equal((await lookupPairing(store, device.id, later.userCode, reopened)).status, 'found', 'the window ends');
});

test('lookup misses below the limit still allow a later hit', async () => {
  const { store, pair } = await setup();
  for (let n = 0; n < LOOKUP_MISS_LIMIT - 1; n++) await lookupPairing(store, device.id, '22222222', 2000);
  assert.equal((await lookupPairing(store, device.id, pair.userCode, 2000)).status, 'found');
});

test('a consumed, cancelled or denied pairing no longer resolves and drops its code index', async () => {
  for (const end of ['consume', 'cancel', 'deny'] as const) {
    const { store, pair } = await setup();
    assert.equal(codeKeys(store).length, 1);
    if (end === 'consume') {
      assert.equal(await approvePairing(store, pair.pairingId, device,
        { clientId: 'verified-client', resource, space: 'review' }, () => 2000), true);
      assert.notEqual(await consumePairing(store, pair.pairingId, pair.browserSecret, () => 2000), null);
    } else if (end === 'cancel') {
      assert.notEqual(await cancelPairingRequest(store, pair.pairingId, pair.browserSecret, 2000), null);
    } else {
      assert.equal(await denyPairing(store, pair.pairingId, device, 2000), true);
    }
    assert.deepEqual(codeKeys(store), [], end);
    assert.deepEqual(await lookupPairing(store, device.id, pair.userCode, 2000), { status: 'missing' }, end);
  }
});

test('deny marks a pending request denied for the browser, once, and blocks approval', async () => {
  const { store, pair } = await setup();
  assert.equal(await denyPairing(store, 'short', device, 2000), false);
  assert.equal(await denyPairing(store, pair.pairingId, device, pair.expiresAt), false, 'expired');
  assert.equal(await denyPairing(store, pair.pairingId, device, 2000), true);
  assert.equal(await denyPairing(store, pair.pairingId, device, 2000), false, 'already denied');
  const view = await browserPairingView(store, pair.pairingId, pair.browserSecret, () => 2000);
  assert.equal(view?.status, 'denied');
  assert.equal(view && 'userCode' in view, false, 'no code once denied');
  assert.equal(await inspectPairing(store, pair.pairingId, 2000), null);
  assert.equal(await approvePairing(store, pair.pairingId, device,
    { clientId: 'verified-client', resource, space: 'review' }, () => 2000), false);
  // The page's cancel turns the denial into the client's access_denied.
  assert.equal(typeof await cancelPairingRequest(store, pair.pairingId, pair.browserSecret, 2000), 'string');
  assert.equal(await browserPairingView(store, pair.pairingId, pair.browserSecret, () => 2000), null);
});

test('approved pairings cannot be denied', async () => {
  const { store, pair } = await setup();
  await approvePairing(store, pair.pairingId, device, { clientId: 'verified-client', resource, space: 'review' }, () => 2000);
  assert.equal(await denyPairing(store, pair.pairingId, device, 2000), false);
  assert.equal((await browserPairingView(store, pair.pairingId, pair.browserSecret, () => 2000))?.status, 'approved');
});

test('browser view shows the allowlisted name, or the host for unknown clients', async () => {
  const known = await setup();
  const knownView = await browserPairingView(known.store, known.pair.pairingId, known.pair.browserSecret, () => 2000);
  assert.deepEqual(knownView, { pairingId: known.pair.pairingId, clientId: 'verified-client', status: 'pending',
    redirectHost: 'claude.ai', knownClient: true, displayName: 'Claude', userCode: known.pair.userCode });
  const hostile = await setup({ redirectUri: 'https://phish.example/cb', clientName: 'Claude' });
  const hostileView = await browserPairingView(hostile.store, hostile.pair.pairingId, hostile.pair.browserSecret, () => 2000);
  assert.equal(hostileView?.knownClient, false);
  assert.equal(hostileView?.displayName, 'phish.example');
  assert(!JSON.stringify(hostileView).includes('Claude'), 'the DCR name is never shown to the browser');
  const inspected = await inspectPairing(hostile.store, hostile.pair.pairingId, 2000);
  assert.equal(inspected?.clientName, 'Claude', 'the app sees the raw name, flagged unknown');
  assert.equal(inspected?.knownClient, false);
});

test('pairing records written before the identity fields read as unknown', async () => {
  const { store, pair } = await setup();
  const key = `pairing:${pair.pairingId}`;
  const record = store.data.get(key) as Record<string, unknown>;
  for (const field of ['clientName', 'redirectHost', 'knownClient', 'userCode']) delete record[field];
  store.data.set(key, record);
  assert.deepEqual(await inspectPairing(store, pair.pairingId, 2000), { pairingId: pair.pairingId, clientId: 'verified-client',
    resource, scopes: ['wenlan:query'], expiresAt: pair.expiresAt, clientName: null, redirectHost: '', knownClient: false });
  const view = await browserPairingView(store, pair.pairingId, pair.browserSecret, () => 2000);
  assert.deepEqual(view, { pairingId: pair.pairingId, clientId: 'verified-client', status: 'pending',
    redirectHost: '', knownClient: false, displayName: null });
  assert.equal(await denyPairing(store, pair.pairingId, device, 2000), true, 'no code index to drop');
});

/** Controls only the 16-byte code draws; 32-byte secrets stay random. */
function withCodeBytes<T>(bytes: number[], run: () => Promise<T>): Promise<T> {
  const original = crypto.getRandomValues.bind(crypto);
  let draw = 0;
  crypto.getRandomValues = (<A extends ArrayBufferView | null>(array: A): A => {
    const view = array as unknown as Uint8Array;
    if (view.length !== 16) return original(array as never) as A;
    view.fill(bytes[Math.min(draw++, bytes.length - 1)]);
    return array;
  }) as typeof crypto.getRandomValues;
  return run().finally(() => { crypto.getRandomValues = original; });
}

test('a live code collision regenerates, an expired index is reused, and attempts are bounded', async () => {
  const store = new MemoryStore();
  const liveIndex = { pairingId: 'p'.repeat(43), expiresAt: 999_999_999 };
  store.data.set('pairing-code:22222222', liveIndex);
  const regenerated = await withCodeBytes([0, 1], () => beginPairing(store, intent(), resource, 1000));
  assert.equal(regenerated.userCode, '33333333');
  assert.deepEqual(store.data.get('pairing-code:22222222'), liveIndex, 'a live index is never overwritten');
  assert.equal((store.data.get('pairing-code:33333333') as { pairingId: string }).pairingId, regenerated.pairingId);
  store.data.set('pairing-code:44444444', { pairingId: 'q'.repeat(43), expiresAt: 500 });
  const reused = await withCodeBytes([2], () => beginPairing(store, intent(), resource, 1000));
  assert.equal(reused.userCode, '44444444');
  assert.equal((store.data.get('pairing-code:44444444') as { pairingId: string }).pairingId, reused.pairingId);
  await assert.rejects(withCodeBytes([0], () => beginPairing(store, intent(), resource, 1000)), /User code unavailable/);
});
