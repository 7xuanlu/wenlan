// SPDX-License-Identifier: Apache-2.0
// Opt-in real macOS App probe. No public enrollment or personal runtime access.
import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, realpath, writeFile } from 'node:fs/promises';
import { createServer, createConnection } from 'node:net';
import { tmpdir } from 'node:os';
import { isAbsolute, join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { promisify } from 'node:util';
import test from 'node:test';
import { readPrivateJson } from '../scripts/private-json.mjs';
import { seedReviewerViaApi } from './fixtures/reviewer-api-seed.mjs';
import {
  OwnedDeviceTracker,
  observeRelayWindow,
  relayConnectionPath,
  revokeOwnedDevices,
  sampleRelayProfile,
} from './fixtures/native-app-relay.mjs';

const exec = promisify(execFile);

async function freePort() {
  const server = createServer();
  await new Promise((resolve, reject) => server.once('error', reject).listen(0, '127.0.0.1', resolve));
  const port = server.address().port;
  await new Promise(resolve => server.close(resolve));
  assert(![7878, 1420, 18080, 18081, 18082, 18083].includes(port));
  return port;
}

async function portClosed(port) {
  return new Promise((resolve, reject) => {
    const socket = createConnection({ host: '127.0.0.1', port });
    socket.once('connect', () => { socket.destroy(); resolve(false); });
    socket.once('error', error => error.code === 'ECONNREFUSED' ? resolve(true) : reject(error));
    socket.setTimeout(1000, () => { socket.destroy(); reject(new Error('listener probe timeout')); });
  });
}

// Default mode is the startup-only lifecycle check. With WENLAN_APP_TEST_RELAY=1
// (which also requires WENLAN_TEST_PUBLIC_RELAY=1) each cycle additionally keeps
// its bounded manual window open for the normal in-App relay connection flow,
// observes only the scratch connection.dat across the restart, then revokes
// every owned device. Relay evidence is local persistence observation, never
// remote connectivity or OAuth proof. No enrollment happens here.
test('isolated native App starts, restarts and releases its owned daemon on SIGTERM', {
  timeout: process.env.WENLAN_APP_TEST_RELAY === '1' ? 1_500_000 : 330_000,
}, async t => {
  assert.equal(process.env.WENLAN_TEST_NATIVE_APP, '1', 'explicit foreground isolated App approval required');
  const relayMode = process.env.WENLAN_APP_TEST_RELAY === '1';
  if (relayMode) {
    assert.equal(process.env.WENLAN_TEST_PUBLIC_RELAY, '1', 'relay mode also requires the public-relay approval gate');
  }
  assert.equal(process.platform, 'darwin', 'this probe only establishes macOS evidence');
  const app = process.env.WENLAN_APP_TEST_BIN;
  const seed = process.env.WENLAN_REVIEWER_SEED_BIN;
  const cache = process.env.WENLAN_TEST_FASTEMBED_CACHE;
  const apiFixture = process.env.WENLAN_APP_FIXTURE_MODE === 'api';
  assert([undefined, 'api'].includes(process.env.WENLAN_APP_FIXTURE_MODE), 'unknown fixture mode');
  for (const path of [app, cache, ...(apiFixture ? [] : [seed])]) assert(path && isAbsolute(path), 'explicit absolute paths required');
  const hash = createHash('sha256').update(await readFile(app)).digest('hex');
  assert.match(process.env.WENLAN_APP_TEST_SHA256 ?? '', /^[a-f0-9]{64}$/);
  assert.equal(hash, process.env.WENLAN_APP_TEST_SHA256, 'binary must match the isolated-build receipt');
  const inspectMs = Number(process.env.WENLAN_APP_INSPECTION_MS ?? 0);
  // Startup-only window stays capped at 180s; relay mode allows up to 600s per manual UI/OAuth window.
  assert(Number.isInteger(inspectMs) && inspectMs >= 0 && inspectMs <= (relayMode ? 600_000 : 180_000));
  if (relayMode) assert(inspectMs >= 1, 'relay mode requires a bounded manual UI window each cycle');
  const offlineMs = Number(process.env.WENLAN_APP_OFFLINE_MS ?? 0);
  assert(Number.isInteger(offlineMs) && offlineMs >= 0 && offlineMs <= 90_000);
  if (offlineMs > 0) assert(relayMode, 'offline window requires relay mode and the public-relay approval gate');
  const scratch = await realpath(await mkdtemp(join(tmpdir(), 'wenlan-app-lifecycle-')));
  const root = join(scratch, 'library');
  const daemonPort = await freePort();
  const uiPort = await freePort();
  const env = {
    PATH: process.env.PATH, LANG: 'en_US.UTF-8', RUST_LOG: 'warn,wenlan_lib=info',
    HOME: join(root, 'home'), USERPROFILE: join(root, 'home'),
    ...(process.env.TMPDIR ? { TMPDIR: process.env.TMPDIR } : {}),
    WENLAN_NO_AUTOSTART: '1', WENLAN_DATA_DIR: root,
    WENLAN_DEV_STATE_DIR: scratch, WENLAN_DEV_APP_ID: 'com.wenlan.desktop.dev.relay-review',
    WENLAN_DEV_TAURI_MCP_SOCKET: join(scratch, 'tauri-mcp.sock'),
    WENLAN_PORT: String(daemonPort), WENLAN_DEV_UI_PORT: String(uiPort),
    WENLAN_DEV_REMOTE_PORT_START: '26120',
    WENLAN_SPACE: 'atlas-review', WENLAN_TEST_FASTEMBED_CACHE: cache,
    WENLAN_RERANKER_MODE: 'off', WENLAN_MCP_CACHE_DIR: join(scratch, 'mcp-cache'),
  };
  // Relay mode monitors only the newly created scratch profile; no fallback
  // to personal paths exists.
  const connectionPath = relayMode ? relayConnectionPath(scratch) : null;
  if (relayMode) assert(connectionPath.startsWith(`${scratch}/`), 'relay profile must stay inside scratch');
  const tracker = new OwnedDeviceTracker();
  const sample = () => sampleRelayProfile(readPrivateJson, connectionPath);
  if (apiFixture) {
    for (const path of [root, join(root, 'home'), join(root, 'pages')]) await mkdir(path, { mode: 0o700 });
    await writeFile(join(root, 'config.json'), JSON.stringify({ knowledge_path: join(root, 'pages'),
      setup_completed: true, reranker_mode: 'off' }), { mode: 0o600, flag: 'wx' });
  } else {
    await exec(seed, [root], { env, timeout: 45_000, maxBuffer: 16_384 });
  }
  await mkdir(join(scratch, 'logs'), { mode: 0o700 });
  const receipts = [];
  let owned = null;
  let ownedExited = null;
  let firstDeviceId = null;
  try {
    for (let cycle = 1; cycle <= 2; cycle++) {
      assert(await portClosed(daemonPort), 'never reuse an occupied daemon port');
      const child = spawn(app, [], { env, cwd: scratch, stdio: ['ignore', 'pipe', 'pipe'] });
      owned = child;
      let output = '';
      for (const pipe of [child.stdout, child.stderr]) pipe.on('data', bytes => {
        output = (output + bytes).slice(-128 * 1024);
      });
      const exited = new Promise((resolve, reject) => {
        child.once('error', reject);
        child.once('exit', (code, signal) => resolve({ code, signal }));
      });
      ownedExited = exited;
      try {
        const deadline = Date.now() + 40_000;
        let healthy = false;
        while (Date.now() < deadline) {
          assert.equal(child.exitCode, null, 'App exited before readiness');
          assert.equal(child.signalCode, null, 'App signalled before readiness');
          try {
            const response = await fetch(`http://127.0.0.1:${daemonPort}/api/knowledge/path`, {
              signal: AbortSignal.timeout(1000),
            });
            if (response.ok) {
              assert.deepEqual(await response.json(), { path: join(root, 'pages') });
              healthy = true;
              break;
            }
          } catch (error) {
            if (error instanceof assert.AssertionError) throw error;
          }
          await delay(200);
        }
        assert(healthy, `App daemon readiness failed; inspect ${scratch}/cycle-${cycle}.log`);
        if (apiFixture && cycle === 1) {
          const fixture = await seedReviewerViaApi({ daemonUrl: `http://127.0.0.1:${daemonPort}`,
            expectedKnowledgePath: join(root, 'pages') });
          await writeFile(join(scratch, 'fixture-mapping.json'), JSON.stringify(fixture, null, 2),
            { mode: 0o600, flag: 'wx' });
        }
        console.log(`WENLAN_APP_READY ${JSON.stringify({ cycle, pid: child.pid, scratch, daemonPort, hash })}`);
        let deviceId = null;
        if (relayMode) {
          // Sample the whole manual window; absent/disabled/no-device are
          // normal pre-connection states, unsafe samples fail the test.
          const observation = await observeRelayWindow({ sample, tracker, windowMs: inspectMs });
          assert.deepEqual(observation.invalidReasons, [], 'unsafe relay profile sample observed');
          assert(observation.enabledSeen, 'an enabled relay device must be observed during the manual window');
          deviceId = observation.lastEnabled.id;
          if (cycle === 1) {
            firstDeviceId = deviceId;
          } else {
            assert.equal(deviceId, firstDeviceId, 'same persisted device identity required after App restart');
          }
        } else if (cycle === 1 && inspectMs) {
          await delay(inspectMs);
        }
        const started = Date.now();
        assert(child.kill('SIGTERM'), 'failed to signal owned App');
        const result = await Promise.race([
          exited, delay(15_000, null, { ref: false }),
        ]);
        assert(result, 'App did not exit within 15 seconds');
        assert.deepEqual(result, { code: 0, signal: null }, 'graceful App exit required');
        assert(output.includes('SIGTERM received'), 'actual App SIGTERM handler must execute');
        assert(output.includes('the sidecar daemon ended'), 'actual App-owned sidecar cleanup must be observed');
        assert(await portClosed(daemonPort), 'App-owned daemon listener survived App exit');
        receipts.push({ cycle, appPid: child.pid, stopMs: Date.now() - started, result, ...(relayMode ? { deviceId } : {}) });
      } finally {
        if (child.exitCode === null && child.signalCode === null) {
          child.kill('SIGTERM');
          if (!await Promise.race([exited, delay(15_000, null, { ref: false })])) {
            child.kill('SIGKILL');
            await exited;
          }
        }
        await writeFile(join(scratch, `cycle-${cycle}.log`), output, { mode: 0o600, flag: 'wx' });
        owned = null;
        ownedExited = null;
      }
      if (cycle === 1 && offlineMs > 0) {
        console.log(`WENLAN_APP_OFFLINE ${JSON.stringify({ scratch, daemonPort, durationMs: offlineMs })}`);
        await delay(offlineMs);
        assert(await portClosed(daemonPort), 'daemon listener reappeared during offline window');
      }
    }
    if (!relayMode) {
      await writeFile(join(scratch, 'receipt.json'), JSON.stringify({ hash, daemonPort, receipts }, null, 2),
        { mode: 0o600, flag: 'wx' });
      t.diagnostic(`Native App lifecycle evidence: ${JSON.stringify({ scratch, hash, receipts })}`);
    }
  } finally {
    if (relayMode) {
      const failures = [];
      try {
        if (owned && owned.exitCode === null && owned.signalCode === null) {
          owned.kill('SIGTERM');
          if (!await Promise.race([ownedExited, delay(15_000, null, { ref: false })])) {
            owned.kill('SIGKILL');
            await ownedExited;
          }
        }
      } catch (error) { failures.push(error); }
      owned = null;
      // One final safe sample picks up a credential rotated just before exit.
      // An unsafe final sample fails the test but never discards tracked
      // credentials needed for cleanup.
      try {
        const final = await sample();
        if (final.kind === 'invalid') failures.push(new Error(`unsafe final relay profile sample: ${final.reason}`));
        else if (final.device) tracker.observe(final.device);
      } catch (error) { failures.push(error); }
      let revocations = [];
      try {
        revocations = await revokeOwnedDevices(tracker.list(), fetch);
      } catch (error) { failures.push(error); }
      try {
        // Device ids only; tokens and profile bodies are never persisted here.
        await writeFile(join(scratch, 'receipt.json'), JSON.stringify({
          hash, daemonPort, relayMode: true, firstDeviceId, trackedDeviceIds: tracker.ids(), revocations, receipts,
        }, null, 2), { mode: 0o600, flag: 'wx' });
      } catch (error) { failures.push(error); }
      if (failures.length) {
        throw new AggregateError(failures, `Relay cleanup unconfirmed; scratch retained at ${scratch}`);
      }
      t.diagnostic(`Native App relay local-persistence evidence: ${JSON.stringify({ scratch, firstDeviceId, trackedDeviceIds: tracker.ids(), revocations })}`);
    }
  }
  if (relayMode) {
    assert(firstDeviceId, 'a first enabled device must have been observed');
    assert.deepEqual(tracker.ids(), [firstDeviceId], 'exactly the observed device identity must be tracked');
  }
});
