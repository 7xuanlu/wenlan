// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { selectTests, testEnvironment, runCommand } from './run-ci.mjs';

const runCiPath = fileURLToPath(new URL('./run-ci.mjs', import.meta.url));

function writeTreeScripts(directory, mode) {
  const grandchild = join(directory, 'grandchild.mjs');
  const child = join(directory, `${mode}.mjs`);
  writeFileSync(grandchild, `${mode === 'timeout' ? "process.on('SIGTERM', () => {});" : ''}
setInterval(() => {}, 1000);\n`);
  writeFileSync(child, `import { spawn } from 'node:child_process';
import { writeFileSync } from 'node:fs';
${mode === 'timeout' ? "process.on('SIGTERM', () => {});" : ''}
const grandchild = spawn(process.execPath, [${JSON.stringify(grandchild)}], { stdio: 'ignore' });
writeFileSync(process.argv[2], String(grandchild.pid));
${mode === 'nonzero' ? 'process.exit(23);' : 'setInterval(() => {}, 1000);'}
`);
  return child;
}

async function waitForFile(path, timeoutMs = 2_000) {
  const deadline = Date.now() + timeoutMs;
  while (!existsSync(path)) {
    if (Date.now() >= deadline) throw new Error(`Timed out waiting for ${path}`);
    await new Promise(resolve => setTimeout(resolve, 10));
  }
  return readFileSync(path, 'utf8').trim();
}

async function waitForPidGone(pid, timeoutMs = 2_000) {
  const deadline = Date.now() + timeoutMs;
  while (true) {
    let alive = false;
    try {
      process.kill(Number(pid), 0);
      alive = true;
    } catch (error) {
      if (error.code !== 'ESRCH') throw error;
    }
    if (!alive) return;
    if (Date.now() >= deadline) throw new Error(`PID ${pid} remained alive`);
    await new Promise(resolve => setTimeout(resolve, 20));
  }
}

function helperSource(childPath, runCiModule, pidPath) {
  return `import { runCommand } from ${JSON.stringify(pathToFileURL(runCiModule).href)};
const result = await runCommand(process.execPath, [${JSON.stringify(childPath)}, ${JSON.stringify(pidPath)}], {
  stdio: 'ignore', timeoutMs: 10_000, termGraceMs: 50, killGraceMs: 300,
});
console.log(JSON.stringify({ timedOut: result.timedOut, cancelled: result.cancelled,
  cancelledSignal: result.cancelledSignal, cleanup: result.cleanup }));
process.exitCode = result.cancelledSignal === 'SIGTERM' ? 143 : 1;
`;
}

test('CI selects test files, not opted-in native or public probes', () => {
  const names = ['proxy.test.ts', 'oauth-runtime.test.mjs', 'deployment-config.test.mjs',
    'reverse-connection-errors.test.mjs', 'native-app-relay.test.mjs',
    'public-native-check.mjs', 'native-reviewer-check.mjs', 'preview.mjs'];
  assert.deepEqual(selectTests(names, 'unit'), ['native-app-relay.test.mjs', 'proxy.test.ts']);
  assert.deepEqual(selectTests(names, 'runtime'), ['deployment-config.test.mjs',
    'oauth-runtime.test.mjs', 'reverse-connection-errors.test.mjs']);
  assert.equal(selectTests(names).length, 5);
  assert.throws(() => selectTests(names, 'live'), /Unknown test lane/);
});

test('CI fixes its own toolchain and removes inherited live-test opt-ins', () => {
  const env = testEnvironment({ PATH: '/bin', WENLAN_NO_AUTOSTART: '0',
    WENLAN_RELAY_TOOLCHAIN: '/legacy', WENLAN_TEST_REAL_PAIRING_EXPIRY: '1',
    WENLAN_RELAY_BUNDLE_DIR: '/old-bundle', CLOUDFLARE_API_TOKEN: 'synthetic',
    MINIFLARE_WORKERD_PATH: '/old-runtime',
    CF_API_KEY: 'synthetic', WRANGLER_SEND_METRICS: 'true' });
  assert.deepEqual(env, { PATH: '/bin', WENLAN_NO_AUTOSTART: '1',
    WENLAN_RELAY_TOOLCHAIN: fileURLToPath(new URL('../', import.meta.url)),
    WRANGLER_SEND_METRICS: 'false' });
});

test('Windows runner refuses before creating an unowned process tree', async () => {
  const result = await runCommand('unused', [], { platform: 'win32',
    spawnProcess() { assert.fail('must not spawn'); } });
  assert.match(result.error.message, /requires POSIX process groups/);
  assert.equal(result.cleanup.attempted, false);
});

const posixOnly = { timeout: 10_000, skip: process.platform === 'win32' };

test('a nonzero child returns its status and cleans up an orphaned grandchild', posixOnly, async () => {
  const directory = mkdtempSync(join(tmpdir(), 'wenlan-run-ci-'));
  try {
    const pidPath = join(directory, 'grandchild.pid');
    const childPath = writeTreeScripts(directory, 'nonzero');
    const result = await runCommand(process.execPath, [childPath, pidPath], {
      cwd: directory, stdio: 'ignore', timeoutMs: 2_000, termGraceMs: 50, killGraceMs: 300,
    });
    assert.equal(result.code, 23);
    assert.equal(result.timedOut, false);
    assert.equal(result.cancelled, false);
    assert.equal(result.cleanup.ok, true);
    await waitForPidGone(await waitForFile(pidPath));
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test('an outer timeout escalates from TERM to KILL for the whole process group', posixOnly, async () => {
  const directory = mkdtempSync(join(tmpdir(), 'wenlan-run-ci-'));
  try {
    const pidPath = join(directory, 'grandchild.pid');
    const childPath = writeTreeScripts(directory, 'timeout');
    const running = runCommand(process.execPath, [childPath, pidPath], {
      cwd: directory, stdio: 'ignore', timeoutMs: 1_000, termGraceMs: 50, killGraceMs: 1_000,
    });
    const grandchildPid = await waitForFile(pidPath);
    const result = await running;
    assert.equal(result.timedOut, true);
    assert.equal(result.cancelled, false);
    assert.equal(result.cleanup.ok, true);
    assert.equal(result.cleanup.forced, true);
    await waitForPidGone(grandchildPid);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});

test('SIGTERM to the runner cleans up its same-group grandchild before exiting', posixOnly, async () => {
  const directory = mkdtempSync(join(tmpdir(), 'wenlan-run-ci-'));
  let helper;
  try {
    const pidPath = join(directory, 'grandchild.pid');
    const childPath = writeTreeScripts(directory, 'signal');
    const helperPath = join(directory, 'runner.mjs');
    writeFileSync(helperPath, helperSource(childPath, runCiPath, pidPath));
    helper = spawn(process.execPath, [helperPath], { cwd: directory, stdio: ['ignore', 'pipe', 'pipe'] });
    const grandchildPid = await waitForFile(pidPath);
    process.kill(helper.pid, 0);
    process.kill(helper.pid, 'SIGTERM');
    const [code, stdout, stderr] = await new Promise(resolve => {
      let output = '';
      let errors = '';
      helper.stdout.on('data', chunk => { output += chunk; });
      helper.stderr.on('data', chunk => { errors += chunk; });
      helper.once('close', (exitCode, signal) => resolve([exitCode, output, errors || signal || '']));
    });
    assert.equal(code, 143, stderr);
    const result = JSON.parse(stdout.trim());
    assert.equal(result.cancelled, true);
    assert.equal(result.cancelledSignal, 'SIGTERM');
    assert.equal(result.cleanup.ok, true);
    await waitForPidGone(grandchildPid);
  } finally {
    if (helper && helper.exitCode === null && helper.signalCode === null) helper.kill('SIGKILL');
    rmSync(directory, { recursive: true, force: true });
  }
});
