// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { activateReverseDevice, authenticateReverseDevice, prepareReverseDevice, reverseConnectionCurrent } from '../src/reverse-devices.ts';
import { authenticateDevice, deviceKey, enrollDevice, refreshDevice, revokeDevice, rotateDeviceCredential, type DeviceRecord } from '../src/devices.ts';
import { connectorKey, type PairingStore, type Transaction } from '../src/pairing.ts';
import { maintainAuthority } from '../src/cleanup.ts';
import type { ConnectorProbe } from '../src/connector-check.ts';
import { MemoryStore } from './fixtures/memory-store.ts';

const now = 1_800_000_000_000;
const candidate = { backendToken: 'b'.repeat(43), space: 'review' };
const connection = 'c'.repeat(64);
const replacement = 'r'.repeat(64);
const info = { contract_version: 1, server: 'wenlan-mcp', tool_profile: 'query-only', authentication: 'bearer', space: 'review' };
const probe: ConnectorProbe = async (headers, signal) => {
  assert.equal(signal.aborted, false);
  if (!headers.has('authorization')) return new Response(null, { status: 401 });
  assert.equal(headers.get('authorization'), `Bearer ${candidate.backendToken}`);
  return Response.json(info);
};
const options = { clock: () => now, connected: () => true };

function pausedProbe() {
  let started!: () => void;
  let resume!: () => void;
  const ready = new Promise<void>(resolve => { started = resolve; });
  const wait = new Promise<void>(resolve => { resume = resolve; });
  const paused: ConnectorProbe = async (headers, signal) => {
    if (headers.has('authorization')) { started(); await wait; }
    return probe(headers, signal);
  };
  return { probe: paused, ready, resume };
}

test('preparation creates only a disabled pending device, never an OAuth route', async () => {
  const store = new MemoryStore();
  const credential = (await prepareReverseDevice(store, candidate, options.clock))!;
  const device = store.data.get(deviceKey(credential.id)) as DeviceRecord;
  assert.equal(device.enabled, false);
  assert.equal(device.pendingReverse?.expiresAt, now + 300_000);
  assert.notEqual(device.credentialHash, credential.managementToken);
  assert.equal(store.data.has(connectorKey(credential.id)), false);
  assert.equal(await authenticateDevice(store, credential.id, credential.managementToken, options.clock), null);
  assert.equal(await authenticateReverseDevice(store, credential.id, credential.managementToken, options.clock), true);
  await maintainAuthority(store, async () => {}, now);
  assert.equal(store.data.has(deviceKey(credential.id)), true);
});

test('invalid preparation writes nothing and invalid credentials cannot probe', async () => {
  const store = new MemoryStore();
  for (const value of [{ ...candidate, backendToken: 'short' }, { ...candidate, space: '' },
    { ...candidate, space: 'x'.repeat(257) }, { ...candidate, space: 'bad\nspace' }]) {
    assert.equal(await prepareReverseDevice(store, value, options.clock), null);
  }
  assert.equal(store.data.size, 0);
  const credential = (await prepareReverseDevice(store, candidate, options.clock))!;
  const never: ConnectorProbe = async () => { assert.fail('must not probe an unauthenticated device'); };
  assert.equal(await activateReverseDevice(store, credential.id, 'wrong'.repeat(16), connection, never, options), null);
  assert.equal(await activateReverseDevice(store, 'missing'.repeat(8), credential.managementToken, connection, never, options), null);
});

test('verified activation atomically enables a generation-bound reverse route', async () => {
  const store = new MemoryStore();
  const credential = (await prepareReverseDevice(store, candidate, options.clock))!;
  const route = await activateReverseDevice(store, credential.id, credential.managementToken, connection, probe, options);
  assert.equal(route?.reverseConnectionId, connection);
  assert.equal(route?.tunnelOrigin, undefined);
  assert.equal(route?.generation, 0);
  const device = store.data.get(deviceKey(credential.id)) as DeviceRecord;
  assert.equal(device.enabled, true); assert.equal(device.revision, 1);
  assert.equal(device.pendingReverse, undefined);
  assert(await authenticateDevice(store, credential.id, credential.managementToken, options.clock));
  assert.equal(await reverseConnectionCurrent(store, credential.id, connection, 0, options.clock), true);
  assert.equal(await reverseConnectionCurrent(store, credential.id, replacement, 0, options.clock), false);
  assert.equal(await activateReverseDevice(store, credential.id, credential.managementToken, connection, probe, options), null);
});

test('scope or contract mismatch leaves the device disabled', async () => {
  const store = new MemoryStore();
  const credential = (await prepareReverseDevice(store, candidate, options.clock))!;
  for (const wrong of [{ ...info, space: 'private' }, { ...info, tool_profile: 'standard' }]) {
    const mismatch: ConnectorProbe = async headers => headers.has('authorization')
      ? Response.json(wrong) : new Response(null, { status: 401 });
    assert.equal(await activateReverseDevice(store, credential.id, credential.managementToken, connection, mismatch, options), null);
  }
  assert.equal((store.data.get(deviceKey(credential.id)) as DeviceRecord).enabled, false);
  assert.equal(store.data.has(connectorKey(credential.id)), false);
});

test('revocation and cleanup during verification prevent delayed resurrection', async () => {
  const store = new MemoryStore();
  const credential = (await prepareReverseDevice(store, candidate, options.clock))!;
  const gate = pausedProbe();
  const result = activateReverseDevice(store, credential.id, credential.managementToken, connection, gate.probe, options);
  await gate.ready;
  assert.equal(await revokeDevice(store, credential.id, credential.managementToken, options.clock), true);
  assert.equal((store.data.get(deviceKey(credential.id)) as DeviceRecord).pendingReverse, undefined);
  await maintainAuthority(store, async () => {}, now);
  gate.resume();
  assert.equal(await result, null);
  assert.equal(store.data.has(deviceKey(credential.id)), false);
  assert.equal(store.data.has(connectorKey(credential.id)), false);
});

test('pending expiry during verification rejects activation and permits cleanup', async () => {
  const store = new MemoryStore();
  let time = now;
  const clock = () => time;
  const credential = (await prepareReverseDevice(store, candidate, clock))!;
  const gate = pausedProbe();
  const result = activateReverseDevice(store, credential.id, credential.managementToken, connection, gate.probe, { ...options, clock });
  await gate.ready;
  time = credential.pendingUntil;
  gate.resume();
  assert.equal(await result, null);
  assert.equal(await authenticateReverseDevice(store, credential.id, credential.managementToken, clock), false);
  await maintainAuthority(store, async () => {}, time);
  assert.equal(store.data.has(deviceKey(credential.id)), false);
});

test('socket loss before commit cannot activate a pending route', async () => {
  const store = new MemoryStore();
  const credential = (await prepareReverseDevice(store, candidate, options.clock))!;
  const gate = pausedProbe();
  let connected = true;
  const result = activateReverseDevice(store, credential.id, credential.managementToken, connection, gate.probe,
    { ...options, connected: () => connected });
  await gate.ready; connected = false; gate.resume();
  assert.equal(await result, null);
  assert.equal(store.data.has(connectorKey(credential.id)), false);
});

test('competing activations commit only one connection from a shared revision', async () => {
  const store = new MemoryStore();
  const credential = (await prepareReverseDevice(store, candidate, options.clock))!;
  const a = pausedProbe(); const b = pausedProbe();
  const first = activateReverseDevice(store, credential.id, credential.managementToken, connection, a.probe, options);
  const second = activateReverseDevice(store, credential.id, credential.managementToken, replacement, b.probe, options);
  await a.ready; await b.ready;
  a.resume(); assert(await first);
  b.resume(); assert.equal(await second, null);
  assert.equal(await reverseConnectionCurrent(store, credential.id, connection, 0, options.clock), true);
});

test('rotation rejects an in-flight reconnect and invalidates the old channel generation', async () => {
  const store = new MemoryStore();
  const credential = (await prepareReverseDevice(store, candidate, options.clock))!;
  assert(await activateReverseDevice(store, credential.id, credential.managementToken, connection, probe, options));
  const gate = pausedProbe();
  const result = activateReverseDevice(store, credential.id, credential.managementToken, replacement, gate.probe, options);
  await gate.ready;
  const rotated = (await rotateDeviceCredential(store, credential.id, credential.managementToken, options.clock))!;
  gate.resume(); assert.equal(await result, null);
  assert.equal(await reverseConnectionCurrent(store, credential.id, connection, 0, options.clock), false);
  assert.equal(await authenticateReverseDevice(store, credential.id, credential.managementToken, options.clock), false);
  const route = await activateReverseDevice(store, credential.id, rotated.managementToken, replacement, probe, options);
  assert.equal(route?.generation, 1);
});

test('expired routes can reconnect using a live device credential without broadening scope', async () => {
  const store = new MemoryStore();
  let time = now;
  const local = { ...options, clock: () => time };
  const credential = (await prepareReverseDevice(store, candidate, local.clock))!;
  const first = (await activateReverseDevice(store, credential.id, credential.managementToken, connection, probe, local))!;
  time = first.expiresAt + 1;
  assert.equal(await reverseConnectionCurrent(store, credential.id, connection, 0, local.clock), false);
  const next = await activateReverseDevice(store, credential.id, credential.managementToken, replacement, probe, local);
  assert.equal(next?.space, candidate.space); assert.equal(next?.generation, first.generation);
  assert(next!.expiresAt > time);
});

test('existing tunnel devices migrate only after a verified reverse handshake', async () => {
  const store = new MemoryStore();
  const credential = (await enrollDevice(store, { ...candidate, tunnelOrigin: 'https://old.trycloudflare.com' }, {
    clock: options.clock, fetch: async (_url, init) => probe(new Headers(init?.headers), init!.signal!),
  }))!;
  const route = await activateReverseDevice(store, credential.id, credential.managementToken, connection, probe, options);
  assert.equal(route?.tunnelOrigin, undefined);
  assert.equal(route?.reverseConnectionId, connection);
});

test('legacy tunnel refresh and renewal cannot corrupt an active reverse route', async () => {
  for (const expectedGeneration of [undefined, 0]) {
    const store = new MemoryStore();
    const credential = (await prepareReverseDevice(store, candidate, options.clock))!;
    const route = await activateReverseDevice(store, credential.id, credential.managementToken, connection, probe, options);
    assert(route);
    const device = structuredClone(store.data.get(deviceKey(credential.id)));
    let probes = 0;
    const updated = await refreshDevice(store, credential.id, credential.managementToken,
      { ...candidate, tunnelOrigin: 'https://legacy.trycloudflare.com' }, {
        clock: options.clock, expectedGeneration,
        fetch: async (_url, init) => {
          probes++;
          return probe(new Headers(init?.headers), init!.signal!);
        },
      });
    assert.equal(updated, false);
    assert.equal(probes, 0);
    assert.deepEqual(store.data.get(connectorKey(credential.id)), route);
    assert.deepEqual(store.data.get(deviceKey(credential.id)), device);
    assert.equal(await reverseConnectionCurrent(store, credential.id, connection, 0, options.clock), true);
  }
});

test('route write failure rolls the activation back without losing pending ownership', async () => {
  const store = new MemoryStore();
  const credential = (await prepareReverseDevice(store, candidate, options.clock))!;
  const failing: PairingStore = {
    transaction<T>(operation: (tx: Transaction) => Promise<T>): Promise<T> {
      return store.transaction(tx => operation({ ...tx, put: async (key, value) => {
        if (key === connectorKey(credential.id)) throw new Error('injected storage failure');
        await tx.put(key, value);
      } }));
    },
  };
  await assert.rejects(activateReverseDevice(failing, credential.id, credential.managementToken, connection, probe, options));
  const device = store.data.get(deviceKey(credential.id)) as DeviceRecord;
  assert.equal(device.enabled, false); assert(device.pendingReverse);
  assert.equal(store.data.has(connectorKey(credential.id)), false);
});
