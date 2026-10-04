// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtemp, mkdir, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  ATTENDED_MARKERS,
  ATTENDED_PHASES,
  runAttendedRecovery,
  statAttendedMarker,
  validateAttendedRecoveryEnv,
} from './fixtures/attended-web-recovery.mjs';

const validEnv = {
  WENLAN_TEST_ATTENDED_WEB_RECOVERY: '1',
  WENLAN_PORTAL_PAIRING_ID: 'b'.repeat(64),
  WENLAN_PORTAL_CLIENT_ID: 'client',
  WENLAN_PORTAL_WINDOW_SECONDS: '180',
};

function harness({ present = {}, invalid = {}, nowValue = 1000, deadlineMs = 2000, onOffline, onReconnect } = {}) {
  let nowCalls = 0;
  const now = () => { nowCalls += 1; return nowValue; };
  const sleeps = [];
  const sleep = async ms => { sleeps.push(ms); assert(ms <= 250, 'poll must be <=250ms'); };
  const stats = [];
  const statMarker = async (dir, name) => {
    stats.push([dir, name]);
    assert.equal(dir, '/markers');
    if (invalid[name]) throw new Error(`invalid ${name}`);
    return { present: Boolean(present[name]) };
  };
  const calls = [];
  return {
    sleeps, stats, calls,
    args: {
      dir: '/markers', deadlineMs, now, sleep, statMarker,
      onOffline: onOffline ?? (async () => { calls.push('offline'); }),
      onReconnect: onReconnect ?? (async () => { calls.push('reconnect'); }),
    },
  };
}

test('static phase and marker names are frozen', () => {
  assert.deepEqual(ATTENDED_PHASES, ['online', 'offline', 'reconnected', 'finished']);
  assert.deepEqual(ATTENDED_MARKERS, ['offline.request', 'reconnect.request', 'finish.request']);
});

test('valid ordered completion runs offline then reconnect', async () => {
  const present = { 'offline.request': true };
  const h = harness({ present,
    onOffline: async () => { present['reconnect.request'] = true; },
    onReconnect: async () => { present['finish.request'] = true; } });
  h.args.onOffline = async () => { h.calls.push('offline'); present['reconnect.request'] = true; };
  h.args.onReconnect = async () => { h.calls.push('reconnect'); present['finish.request'] = true; };
  const phases = [];
  h.args.onPhase = phase => phases.push(phase);
  await runAttendedRecovery(h.args);
  assert.deepEqual(h.calls, ['offline', 'reconnect']);
  assert.deepEqual(phases, ATTENDED_PHASES);
});

test('timeout in each phase', async () => {
  // Phase 0: nothing present, deadline already reached.
  await assert.rejects(runAttendedRecovery(harness({ nowValue: 5000, deadlineMs: 5000 }).args), /deadline/);
  // Phase 1: offline done, reconnect marker never arrives.
  {
    let advanced = false;
    const h = harness({ present: { 'offline.request': true } });
    h.args.now = () => (advanced ? 9999 : 1000);
    h.args.sleep = async () => { advanced = true; };
    await assert.rejects(runAttendedRecovery(h.args), /deadline/);
    assert.deepEqual(h.calls, ['offline']);
  }
  // Phase 2: finish marker never arrives.
  {
    let advanced = false;
    const present = { 'offline.request': true };
    const h = harness({ present });
    h.args.onOffline = async () => { h.calls.push('offline'); present['reconnect.request'] = true; };
    h.args.now = () => (advanced ? 9999 : 1000);
    h.args.sleep = async () => { advanced = true; };
    await assert.rejects(runAttendedRecovery(h.args), /deadline/);
    assert.deepEqual(h.calls, ['offline', 'reconnect']);
  }
});

test('filesystem probe rejects symlinks, directories and oversized markers', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'wenlan-attended-marker-test-'));
  try {
    assert.deepEqual(await statAttendedMarker(dir, 'offline.request'), { present: false });
    await writeFile(join(dir, 'offline.request'), 'go\n', { mode: 0o600 });
    assert.deepEqual(await statAttendedMarker(dir, 'offline.request'), { present: true });
    await symlink(join(dir, 'offline.request'), join(dir, 'reconnect.request'));
    await assert.rejects(statAttendedMarker(dir, 'reconnect.request'), /symlink/);
    await mkdir(join(dir, 'finish.request'));
    await assert.rejects(statAttendedMarker(dir, 'finish.request'), /regular file/);
    await writeFile(join(dir, 'offline.request'), 'x'.repeat(65));
    await assert.rejects(statAttendedMarker(dir, 'offline.request'), /64 bytes/);
  } finally { await rm(dir, { recursive: true, force: true }); }
});

test('public wiring validates before allocation and enrollment; keeps raw proof unchecked', async () => {
  const source = await readFile(new URL('./public-native-check.mjs', import.meta.url), 'utf8');
  assert(source.indexOf('const attendedRecovery = validateAttendedRecoveryEnv') < source.indexOf('const scratch ='));
  assert(source.includes('process.env, portalWait'));
  assert(source.includes('reverseHelper, ...recoveryTasks, tunnel, mcp, daemon'));
  assert(source.includes("chatgpt: 'unchecked', gui: 'unchecked'"));
});

test('invalid and out-of-order markers fail', async () => {
  await assert.rejects(
    runAttendedRecovery(harness({ invalid: { 'offline.request': true } }).args), /invalid offline/);
  await assert.rejects(
    runAttendedRecovery(harness({ present: { 'reconnect.request': true } }).args), /out of order/);
  await assert.rejects(
    runAttendedRecovery(harness({ present: { 'offline.request': true, 'finish.request': true } }).args),
    /out of order/);
});

test('callback failures propagate and do no further work', async () => {
  const h = harness({
    present: { 'offline.request': true },
    onOffline: async () => { throw new Error('offline boom'); },
    onReconnect: async () => { h.calls.push('reconnect'); },
  });
  await assert.rejects(runAttendedRecovery(h.args), /offline boom/);
  assert.deepEqual(h.calls, []);
  const present2 = { 'offline.request': true };
  const h2 = harness({ present: present2 });
  h2.args.onOffline = async () => { h2.calls.push('offline'); present2['reconnect.request'] = true; };
  h2.args.onReconnect = async () => { throw new Error('reconnect boom'); };
  await assert.rejects(runAttendedRecovery(h2.args), /reconnect boom/);
  assert.deepEqual(h2.calls, ['offline']);
});

test('deadline reached during a callback fails after the callback', async () => {
  let nowValue = 1000;
  const present3 = { 'offline.request': true };
  const h = harness({
    present: present3,
    deadlineMs: 2000,
    onOffline: async () => { nowValue = 9999; },
  });
  h.args.now = () => nowValue;
  await assert.rejects(runAttendedRecovery(h.args), /deadline/);
  assert.deepEqual(h.calls, []);
});

test('validation: absent or exactly 1, reverse, portal fields, 180, no Codex lane', async () => {
  assert.equal(validateAttendedRecoveryEnv({}, { transport: 'reverse' }), false);
  assert.equal(validateAttendedRecoveryEnv(validEnv, { transport: 'reverse' }), true);
  for (const bad of ['0', '2', 'true', '']) {
    assert.throws(() => validateAttendedRecoveryEnv({ ...validEnv, WENLAN_TEST_ATTENDED_WEB_RECOVERY: bad },
      { transport: 'reverse' }), /exactly 1/);
  }
  assert.throws(() => validateAttendedRecoveryEnv(validEnv, { transport: 'tunnel' }), /reverse/);
  assert.throws(() => validateAttendedRecoveryEnv({ ...validEnv, WENLAN_PORTAL_PAIRING_ID: undefined },
    { transport: 'reverse' }), /portal pairing/);
  assert.throws(() => validateAttendedRecoveryEnv({ ...validEnv, WENLAN_PORTAL_CLIENT_ID: '' },
    { transport: 'reverse' }), /portal client/);
  for (const window of [undefined, '90', '60']) {
    assert.throws(() => validateAttendedRecoveryEnv({ ...validEnv, WENLAN_PORTAL_WINDOW_SECONDS: window },
      { transport: 'reverse' }), /180/);
  }
  assert.throws(() => validateAttendedRecoveryEnv({ ...validEnv, WENLAN_CODEX_BIN: '/bin/codex' },
    { transport: 'reverse' }), /Codex/);
  assert.throws(() => validateAttendedRecoveryEnv({ ...validEnv, WENLAN_CODEX_MODEL_HOME: '/tmp/m' },
    { transport: 'reverse' }), /Codex/);
});

test('controls finishing does not mark ChatGPT proof passed', async () => {
  const present = { 'offline.request': true };
  const h = harness({ present });
  h.args.onOffline = async () => { h.calls.push('offline'); present['reconnect.request'] = true; };
  h.args.onReconnect = async () => { h.calls.push('reconnect'); present['finish.request'] = true; };
  const outcome = await runAttendedRecovery(h.args).then(() => 'controls-finished');
  assert.equal(outcome, 'controls-finished');
  assert(!/proof|passed|chatgpt/i.test(outcome), 'control completion is not a proof claim');
});
