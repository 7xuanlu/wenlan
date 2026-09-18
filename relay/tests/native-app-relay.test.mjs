// SPDX-License-Identifier: Apache-2.0
// Synthetic unit coverage for the native App relay lifecycle probe.
// All relay I/O is injected; no external requests, no App launch.
import assert from 'node:assert/strict';
import test from 'node:test';
import {
  NATIVE_APP_RELAY_ORIGIN,
  OwnedDeviceTracker,
  observeRelayWindow,
  relayConnectionPath,
  revokeOwnedDevices,
  sampleRelayProfile,
} from './fixtures/native-app-relay.mjs';

const ID_A = 'a'.repeat(32);
const ID_B = 'b'.repeat(32);
const TOKEN_A = 'c'.repeat(64);
const TOKEN_B = 'd'.repeat(64);
const REV = 'e'.repeat(32);
const BACKEND = 'f'.repeat(64);

function profile(overrides = {}) {
  return {
    version: 1,
    revision: REV,
    relay_origin: NATIVE_APP_RELAY_ORIGIN,
    space: 'atlas-review',
    backend_token: BACKEND,
    enabled: false,
    device: null,
    ...overrides,
  };
}

function enabledProfile(id, token, overrides = {}) {
  return profile({
    enabled: true,
    device: { id, managementToken: token, expiresAt: Date.now() + 3_600_000 },
    ...overrides,
  });
}

function missingFile() {
  const error = new Error('not found');
  error.code = 'ENOENT';
  throw error;
}

function jsonResponse(status, body) {
  const text = JSON.stringify(body);
  return {
    status,
    body: {
      getReader() {
        let done = false;
        return {
          async read() {
            if (done) return { done: true, value: undefined };
            done = true;
            return { done: false, value: new TextEncoder().encode(text) };
          },
          async cancel() {},
        };
      },
    },
  };
}

function fakeClock() {
  let now = 0;
  return { now: () => now, delay: async ms => { now += ms; } };
}

test('absent file transitions through disabled to an enrolled device', async () => {
  const states = [
    () => missingFile(),
    async () => profile(),
    async () => profile({ enabled: true }),
    async () => enabledProfile(ID_A, TOKEN_A),
  ];
  let step = 0;
  const read = () => states[Math.min(step++, states.length - 1)]();
  const tracker = new OwnedDeviceTracker();
  assert.equal((await sampleRelayProfile(read, '/scratch/connection.dat')).kind, 'absent');
  assert.equal((await sampleRelayProfile(read, '/scratch/connection.dat')).kind, 'disabled');
  assert.equal((await sampleRelayProfile(read, '/scratch/connection.dat')).kind, 'no-device');
  const enrolled = await sampleRelayProfile(read, '/scratch/connection.dat');
  assert.equal(enrolled.kind, 'enabled-device');
  assert.equal(enrolled.device.id, ID_A);
  assert(tracker.observe(enrolled.device));
  assert.deepEqual(tracker.ids(), [ID_A]);
  assert.deepEqual(Object.keys(tracker.list()[0]).sort(), ['expiresAt', 'id', 'managementToken']);
});

test('restart observes the same persisted device identity enabled', async () => {
  const tracker = new OwnedDeviceTracker();
  const first = await sampleRelayProfile(async () => enabledProfile(ID_A, TOKEN_A), '/scratch/connection.dat');
  tracker.observe(first.device);
  const second = await sampleRelayProfile(
    async () => enabledProfile(ID_A, TOKEN_A, { revision: 'a'.repeat(32), backend_token: 'b'.repeat(64) }),
    '/scratch/connection.dat',
  );
  assert.equal(second.kind, 'enabled-device');
  assert.equal(second.device.id, first.device.id);
  tracker.observe(second.device);
  assert.equal(tracker.size, 1);
});

test('window sampling covers the entire window and keeps the latest credential', async () => {
  const tracker = new OwnedDeviceTracker();
  const clock = fakeClock();
  const script = [
    { kind: 'absent' },
    { kind: 'disabled' },
    { kind: 'enabled-device', device: { id: ID_A, managementToken: TOKEN_A, expiresAt: 1 } },
    { kind: 'enabled-device', device: { id: ID_A, managementToken: TOKEN_B, expiresAt: 2 } },
  ];
  const result = await observeRelayWindow({
    sample: async () => script.shift() ?? { kind: 'enabled-device', device: { id: ID_A, managementToken: TOKEN_B, expiresAt: 2 } },
    tracker,
    windowMs: 3000,
    intervalMs: 1000,
    delayImpl: clock.delay,
    nowImpl: clock.now,
  });
  assert.equal(result.enabledSeen, true);
  assert.equal(result.samples, 4);
  assert.deepEqual(result.invalidReasons, []);
  assert.equal(result.lastEnabled.managementToken, TOKEN_B);
  assert.deepEqual(tracker.list(), [{ id: ID_A, managementToken: TOKEN_B, expiresAt: 2 }]);
  const empty = await observeRelayWindow({
    sample: async () => ({ kind: 'absent' }),
    tracker: new OwnedDeviceTracker(),
    windowMs: 0,
    delayImpl: clock.delay,
    nowImpl: clock.now,
  });
  assert.equal(empty.samples, 1);
  assert.equal(empty.enabledSeen, false);
});

test('revoke uses the exact positional wiring the harness calls', async () => {
  assert.equal(revokeOwnedDevices.length, 2);
  const calls = [];
  const outcomes = await revokeOwnedDevices(
    [{ id: ID_A, managementToken: TOKEN_B, expiresAt: 2 }],
    async (url, init) => {
      calls.push({ url, init });
      return jsonResponse(200, { success: true });
    },
  );
  assert.deepEqual(outcomes, [{ id: ID_A, status: 'revoked' }]);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].url, `${NATIVE_APP_RELAY_ORIGIN}/devices/revoke`);
  assert.equal(calls[0].init.method, 'POST');
  assert.equal(calls[0].init.headers['x-wenlan-device-id'], ID_A);
  assert.equal(calls[0].init.headers.authorization, `Bearer ${TOKEN_B}`);
  assert.equal(calls[0].init.body, '{}');
  assert.equal(calls[0].init.redirect, 'error');
  for (const call of calls) assert(call.url.startsWith('https://relay.wenlan.app/'));
});

test('cleanup revokes owned devices even when assertions fail', async () => {
  const tracker = new OwnedDeviceTracker();
  tracker.observe({ id: ID_A, managementToken: TOKEN_A, expiresAt: 1 });
  let assertionFailure = null;
  let outcomes = null;
  try {
    assert.equal('different-device-id', ID_A, 'same persisted device identity required');
  } catch (error) {
    assertionFailure = error;
  } finally {
    outcomes = await revokeOwnedDevices(tracker.list(), async () => jsonResponse(200, { success: true }));
  }
  assert.match(String(assertionFailure), /same persisted device identity required/);
  assert.deepEqual(outcomes, [{ id: ID_A, status: 'revoked' }]);
});

test('first-device failure still attempts the second, then throws AggregateError', async () => {
  const attempted = [];
  const fetchImpl = async (url, init) => {
    attempted.push(init.headers['x-wenlan-device-id']);
    if (init.headers['x-wenlan-device-id'] === ID_A) return { status: 500 };
    return jsonResponse(200, { success: true });
  };
  await assert.rejects(
    revokeOwnedDevices([
      { id: ID_A, managementToken: TOKEN_A, expiresAt: 1 },
      { id: ID_B, managementToken: TOKEN_B, expiresAt: 2 },
    ], fetchImpl),
    error => error instanceof AggregateError
      && error.errors.length === 1
      && /HTTP 500/.test(error.errors[0].message),
  );
  assert.deepEqual(attempted, [ID_A, ID_B]);
});

test('unsafe origin or space is rejected and never tracked', async () => {
  const tracker = new OwnedDeviceTracker();
  for (const bad of [
    enabledProfile(ID_A, TOKEN_A, { relay_origin: 'https://relay.evil.example' }),
    enabledProfile(ID_A, TOKEN_A, { relay_origin: 'http://relay.wenlan.app' }),
    enabledProfile(ID_A, TOKEN_A, { space: 'personal' }),
    enabledProfile(ID_A, TOKEN_A, { space: ' atlas-review' }),
    enabledProfile('short', TOKEN_A),
    enabledProfile(ID_A, TOKEN_A, { device: { id: ID_A, managementToken: TOKEN_A, expiresAt: 0 } }),
  ]) {
    const sample = await sampleRelayProfile(async () => bad, '/scratch/connection.dat');
    assert.equal(sample.kind, 'invalid');
    assert.equal(sample.device, undefined);
  }
  assert.equal(tracker.size, 0);
});

test('unsafe final sample fails while retaining tracked credentials for cleanup', async () => {
  const tracker = new OwnedDeviceTracker();
  tracker.observe({ id: ID_A, managementToken: TOKEN_A, expiresAt: 1 });
  const failures = [];
  const final = await sampleRelayProfile(async () => enabledProfile(ID_A, TOKEN_A, { space: 'personal' }), '/scratch/connection.dat');
  if (final.kind === 'invalid') failures.push(new Error(`unsafe final relay profile sample: ${final.reason}`));
  else if (final.device) tracker.observe(final.device);
  const revocations = await revokeOwnedDevices(tracker.list(), async () => jsonResponse(200, { success: true }));
  assert.equal(failures.length, 1);
  assert.deepEqual(revocations, [{ id: ID_A, status: 'revoked' }]);
  assert.deepEqual(tracker.ids(), [ID_A]);
});

test('redirect and network failure fail revocation without response logs', async () => {
  const devices = [{ id: ID_A, managementToken: TOKEN_A, expiresAt: 1 }];
  const redirect = new Error('redirects blocked');
  redirect.name = 'TypeError';
  await assert.rejects(
    revokeOwnedDevices(devices, async () => { throw redirect; }),
    error => error instanceof AggregateError && /TypeError/.test(error.errors[0].message),
  );
  await assert.rejects(
    revokeOwnedDevices(devices, async () => ({ status: 500 })),
    error => error instanceof AggregateError && /HTTP 500/.test(error.errors[0].message),
  );
  await assert.rejects(
    revokeOwnedDevices(devices, async () => jsonResponse(200, { success: false })),
    error => error instanceof AggregateError && /success flag missing/.test(error.errors[0].message),
  );
});

test('already-revoked 401 response is accepted', async () => {
  const outcomes = await revokeOwnedDevices(
    [{ id: ID_A, managementToken: TOKEN_A, expiresAt: 1 }],
    async () => ({ status: 401 }),
  );
  assert.deepEqual(outcomes, [{ id: ID_A, status: 'already-invalid' }]);
});

test('real Response bodies remain bounded during cleanup', async () => {
  const devices = [{ id: ID_A, managementToken: TOKEN_A, expiresAt: 1 }];
  assert.equal((await revokeOwnedDevices(devices,
    async () => Response.json({ success: true })))[0].status, 'revoked');
  await assert.rejects(revokeOwnedDevices(devices,
    async () => new Response('x'.repeat(65537))), AggregateError);
});

test('connection path stays inside the given scratch root', () => {
  assert.equal(
    relayConnectionPath('/tmp/wenlan-app-relay-xyz'),
    '/tmp/wenlan-app-relay-xyz/mcp-config/relay-v1/connection.dat',
  );
});
