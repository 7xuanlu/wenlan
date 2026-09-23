// SPDX-License-Identifier: Apache-2.0
// Helpers for the opt-in native App relay lifecycle probe. Unit tests inject
// fakes; nothing here enrolls, launches the App, or touches personal paths.
import { isAbsolute, join, relative, sep } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

export const NATIVE_APP_RELAY_ORIGIN = 'https://relay.wenlan.app';
const RELAY_SPACE = 'atlas-review';
const REVOKE_TIMEOUT_MS = 15_000;
const RESPONSE_LIMIT_BYTES = 64 * 1024;

const ID_PATTERN = /^[A-Za-z0-9_-]+$/;

export function isRelayId(value, minimum = 32) {
  return typeof value === 'string' && value.length >= minimum && value.length <= 128 && ID_PATTERN.test(value);
}

function rejectTraversal(value) {
  if (String(value).split(/[\\/]+/).includes('..')) {
    throw new Error('relay connection path escapes its root');
  }
}

function requireScratch(scratch) {
  if (typeof scratch !== 'string' || scratch.length === 0 || !isAbsolute(scratch)) {
    throw new Error('relay connection scratch must be an absolute path');
  }
  rejectTraversal(scratch);
}

// Frozen build-profile routing for the synthetic probe. Debug mirrors the
// debug-only WENLAN_DEV_STATE_DIR override (scratch/mcp-config); release
// mirrors home_base() + .config/wenlan-mcp. Pure helper: no HOME inference,
// search/fallback, symlink following, or credential reads. The caller owns a
// fresh scratch and its isolated home; no OS sandbox is claimed here.
export function relayConnectionPath(scratch, { buildProfile = 'debug', home } = {}) {
  if (buildProfile !== 'debug' && buildProfile !== 'release') {
    throw new Error(`unknown build profile: ${String(buildProfile)}`);
  }
  requireScratch(scratch);
  if (buildProfile === 'debug') {
    return join(scratch, 'mcp-config', 'relay-v1', 'connection.dat');
  }
  if (typeof home !== 'string' || home.length === 0 || !isAbsolute(home)) {
    throw new Error('release build profile requires an absolute home');
  }
  rejectTraversal(home);
  const rel = relative(scratch, home);
  if (isAbsolute(rel) || rel === '..' || rel.startsWith(`..${sep}`)) {
    throw new Error('release home must stay inside scratch');
  }
  return join(home, '.config', 'wenlan-mcp', 'relay-v1', 'connection.dat');
}

function validDevice(device) {
  return !!device && isRelayId(device.id) && isRelayId(device.managementToken)
    && Number.isInteger(device.expiresAt) && device.expiresAt > 0;
}

export async function sampleRelayProfile(read, path) {
  let profile;
  try {
    profile = await read(path);
  } catch (error) {
    return error?.code === 'ENOENT' ? { kind: 'absent' } : { kind: 'invalid', reason: 'unreadable-profile' };
  }
  if (!profile || typeof profile !== 'object'
    || profile.version !== 1
    || profile.relay_origin !== NATIVE_APP_RELAY_ORIGIN
    || !isRelayId(profile.revision)
    || profile.space !== RELAY_SPACE
    || !isRelayId(profile.backend_token)
    || typeof profile.enabled !== 'boolean'
    || (profile.device != null && !validDevice(profile.device))) {
    return { kind: 'invalid', reason: 'shape' };
  }
  if (!profile.enabled) return profile.device == null ? { kind: 'disabled' } : { kind: 'disabled', device: { ...profile.device } };
  if (profile.device == null) return { kind: 'no-device' };
  return { kind: 'enabled-device', device: { ...profile.device } };
}

export class OwnedDeviceTracker {
  #devices = new Map();
  observe(device) {
    if (!validDevice(device)) return false;
    this.#devices.set(device.id, { ...device });
    return true;
  }
  ids() {
    return [...this.#devices.keys()];
  }
  list() {
    return [...this.#devices.values()];
  }
  get size() {
    return this.#devices.size;
  }
}

// Samples throughout the entire bounded window so manual UI inspection is
// never cut short. Tracks every valid credential; reports enabled sightings
// and unsafe samples for the caller to assert on after the window.
export async function observeRelayWindow({ sample, tracker, windowMs, intervalMs = 1000, delayImpl = delay, nowImpl = Date.now }) {
  const end = nowImpl() + windowMs;
  const result = { enabledSeen: false, samples: 0, invalidReasons: [], lastEnabled: null };
  for (;;) {
    const observed = await sample();
    result.samples++;
    if (observed.kind === 'invalid') result.invalidReasons.push(observed.reason);
    else if (observed.device) tracker.observe(observed.device);
    if (observed.kind === 'enabled-device') {
      result.enabledSeen = true;
      result.lastEnabled = observed.device;
    }
    if (nowImpl() >= end) break;
    await delayImpl(Math.min(intervalMs, Math.max(end - nowImpl(), 0)));
  }
  return result;
}

async function readBounded(body) {
  const reader = body?.getReader?.();
  if (!reader) return null;
  const chunks = [];
  let length = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      length += next.value.length;
      if (length > RESPONSE_LIMIT_BYTES) throw new Error('response exceeds test boundary');
      chunks.push(Buffer.from(next.value));
    }
  } finally {
    await reader.cancel().catch(() => {});
  }
  return Buffer.concat(chunks).toString('utf8');
}

async function revokeOne(device, fetchImpl) {
  let response;
  try {
    response = await fetchImpl(`${NATIVE_APP_RELAY_ORIGIN}/devices/revoke`, {
      method: 'POST',
      headers: {
        authorization: `Bearer ${device.managementToken}`,
        'x-wenlan-device-id': device.id,
        'content-type': 'application/json',
      },
      body: '{}',
      redirect: 'error',
      signal: AbortSignal.timeout(REVOKE_TIMEOUT_MS),
    });
  } catch (error) {
    throw new Error(`revoke request failed for device ${device.id}: ${error?.name ?? 'fetch-error'}`);
  }
  if (response.status === 401) return { id: device.id, status: 'already-invalid' };
  if (response.status !== 200) throw new Error(`revoke rejected for device ${device.id}: HTTP ${response.status}`);
  const text = await readBounded(response.body);
  if (text == null || Buffer.byteLength(text) > RESPONSE_LIMIT_BYTES) throw new Error('response exceeds test boundary');
  let success = false;
  try {
    success = JSON.parse(text).success === true;
  } catch {
    success = false;
  }
  if (!success) throw new Error(`revoke unconfirmed for device ${device.id}: success flag missing`);
  return { id: device.id, status: 'revoked' };
}

// Attempts every tracked device, then reports all unconfirmed cleanups
// together. The production origin is fixed here, never caller-overridable.
export async function revokeOwnedDevices(devices, fetchImpl) {
  const unique = new Map();
  for (const device of devices ?? []) {
    if (validDevice(device)) unique.set(device.id, device);
  }
  const outcomes = [];
  const failures = [];
  for (const device of unique.values()) {
    try {
      outcomes.push(await revokeOne(device, fetchImpl));
    } catch (error) {
      failures.push(error);
    }
  }
  if (failures.length) throw new AggregateError(failures, 'relay cleanup unconfirmed');
  return outcomes;
}
