// SPDX-License-Identifier: Apache-2.0
// Bounded attended Web recovery control. Dependency-injectable for unit tests.
import assert from 'node:assert/strict';
import { lstat } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

export const ATTENDED_PHASES = ['online', 'offline', 'reconnected', 'finished'];
export const ATTENDED_MARKERS = ['offline.request', 'reconnect.request', 'finish.request'];
export const ATTENDED_POLL_MS = 250;
export const ATTENDED_MAX_MARKER_BYTES = 64;

// Opt-in validation runs before public enrollment (before any public mutation).
// Returns true when attended recovery is enabled.
export function validateAttendedRecoveryEnv(env = process.env, { transport } = {}) {
  const optIn = env.WENLAN_TEST_ATTENDED_WEB_RECOVERY;
  if (optIn === undefined) return false;
  assert.equal(optIn, '1', 'WENLAN_TEST_ATTENDED_WEB_RECOVERY must be absent or exactly 1');
  assert.equal(transport, 'reverse', 'attended recovery requires the reverse transport');
  assert.match(env.WENLAN_PORTAL_PAIRING_ID ?? '', /^[a-f0-9]{64}$/,
    'attended recovery requires an explicit portal pairing');
  assert(typeof env.WENLAN_PORTAL_CLIENT_ID === 'string' && env.WENLAN_PORTAL_CLIENT_ID.length > 0
    && env.WENLAN_PORTAL_CLIENT_ID.length <= 2048,
    'attended recovery requires an explicit portal client');
  assert.equal(env.WENLAN_PORTAL_WINDOW_SECONDS, '180',
    'attended recovery requires an explicit 180-second window');
  assert.equal(env.WENLAN_CODEX_BIN, undefined, 'attended recovery and Codex lanes are separate runs');
  assert.equal(env.WENLAN_CODEX_MODEL_HOME, undefined, 'attended recovery and Codex lanes are separate runs');
  return true;
}

// Default filesystem marker probe. Never reads marker content.
export async function statAttendedMarker(dir, name) {
  const path = join(dir, name);
  let link;
  try {
    link = await lstat(path);
  } catch (error) {
    if (error?.code === 'ENOENT') return { present: false };
    throw error;
  }
  assert(!link.isSymbolicLink(), `attended marker ${name} must not be a symlink`);
  assert(link.isFile(), `attended marker ${name} must be a regular file`);
  assert(link.size <= ATTENDED_MAX_MARKER_BYTES, `attended marker ${name} exceeds 64 bytes`);
  return { present: true };
}

function checkDeadline(now, deadlineMs) {
  assert(now() < deadlineMs, 'attended recovery deadline reached');
}

async function waitForMarker({ dir, index, deadlineMs, now, sleep, statMarker }) {
  for (;;) {
    checkDeadline(now, deadlineMs);
    // Out-of-order: a later marker must not appear before the current one.
    for (let later = index + 1; later < ATTENDED_MARKERS.length; later += 1) {
      const probe = await statMarker(dir, ATTENDED_MARKERS[later]);
      assert(!probe.present, `attended marker ${ATTENDED_MARKERS[later]} arrived out of order`);
    }
    const current = await statMarker(dir, ATTENDED_MARKERS[index]);
    if (current.present) return;
    checkDeadline(now, deadlineMs);
    await sleep(Math.min(ATTENDED_POLL_MS, Math.max(0, deadlineMs - now())));
  }
}

// Ordered online -> offline -> reconnected -> finished within one monotonic deadline.
// Sequential awaits only: no detached promises, no races leaving callbacks unawaited.
export async function runAttendedRecovery({
  dir,
  deadlineMs,
  now = () => performance.now(),
  sleep = delay,
  statMarker = statAttendedMarker,
  onOffline,
  onReconnect,
  onPhase = () => {},
} = {}) {
  assert(typeof dir === 'string' && dir.length > 0, 'attended recovery dir required');
  assert(Number.isFinite(deadlineMs), 'attended recovery deadline required');
  assert(typeof onOffline === 'function', 'offline callback required');
  assert(typeof onReconnect === 'function', 'reconnect callback required');
  checkDeadline(now, deadlineMs);
  onPhase('online');
  await waitForMarker({ dir, index: 0, deadlineMs, now, sleep, statMarker });
  checkDeadline(now, deadlineMs);
  await onOffline();
  checkDeadline(now, deadlineMs);
  onPhase('offline');
  await waitForMarker({ dir, index: 1, deadlineMs, now, sleep, statMarker });
  checkDeadline(now, deadlineMs);
  await onReconnect();
  checkDeadline(now, deadlineMs);
  onPhase('reconnected');
  await waitForMarker({ dir, index: 2, deadlineMs, now, sleep, statMarker });
  checkDeadline(now, deadlineMs);
  onPhase('finished');
}
