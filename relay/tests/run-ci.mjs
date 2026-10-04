// SPDX-License-Identifier: Apache-2.0
import { readdirSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { join, resolve } from 'node:path';

const relayRoot = fileURLToPath(new URL('../', import.meta.url));
const repoRoot = resolve(relayRoot, '..');
export const TEST_TIMEOUT_MS = 90_000;
export const OUTER_TIMEOUT_MS = 12 * 60 * 1000;
export const TERM_GRACE_MS = 2_000;
export const KILL_GRACE_MS = 2_000;

const SIGNAL_EXIT_CODES = { SIGINT: 130, SIGTERM: 143 };
const TIMEOUT = Symbol('timeout');
const INTERRUPTED = Symbol('interrupted');

export function selectTests(names, lane = 'all') {
  if (!['all', 'unit', 'runtime'].includes(lane)) throw new Error(`Unknown test lane: ${lane}`);
  return names.filter(name => {
    if (!/\.test\.(ts|mjs)$/.test(name)) return false;
    const runtime = name.includes('runtime.test.') ||
      ['deployment-config.test.mjs', 'reverse-connection-errors.test.mjs'].includes(name);
    return lane === 'all' || (lane === 'runtime') === runtime;
  }).sort();
}

export function testEnvironment(parent) {
  const env = Object.fromEntries(Object.entries(parent).filter(([key]) =>
    !key.startsWith('WENLAN_') && !key.startsWith('MINIFLARE_') &&
    !key.startsWith('CLOUDFLARE_') && !key.startsWith('CF_')));
  return { ...env, WENLAN_NO_AUTOSTART: '1', WENLAN_RELAY_TOOLCHAIN: relayRoot,
    WRANGLER_SEND_METRICS: 'false' };
}

function delay(ms) {
  return new Promise(resolveDelay => setTimeout(resolveDelay, ms));
}

function probeProcess(kill, pid) {
  try {
    kill(pid, 0);
    return true;
  } catch (error) {
    if (error?.code === 'EPERM') return true;
    if (error?.code === 'ESRCH') return false;
    throw error;
  }
}

async function waitForProcessGone(kill, pid, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (probeProcess(kill, pid)) {
    const remaining = deadline - Date.now();
    if (remaining <= 0) return false;
    await delay(Math.min(25, remaining));
  }
  return true;
}

function cleanupResult(platform, attempted, forced, ok, error = null) {
  return { platform, attempted, forced, ok, error: error?.message ?? error };
}

const WINDOWS_RUNNER_ERROR = 'Relay CI runner requires POSIX process groups; run under WSL or Linux CI. Native Windows lifecycle validation is separate.';

async function terminatePosixGroup(child, {
  kill,
  termGraceMs,
  killGraceMs,
}) {
  const pgid = child.pid;
  if (!Number.isInteger(pgid) || pgid <= 0 || !probeProcess(kill, -pgid)) {
    return cleanupResult('posix', false, false, true);
  }

  const signalGroup = signal => {
    // Probe immediately before each signal. The group id is the pid returned
    // by spawn(detached: true), so this never searches by command or name.
    if (!probeProcess(kill, -pgid)) return false;
    try {
      kill(-pgid, signal);
      return true;
    } catch (error) {
      if (error?.code === 'ESRCH') return false;
      throw error;
    }
  };

  signalGroup('SIGTERM');
  if (await waitForProcessGone(kill, -pgid, termGraceMs)) {
    return cleanupResult('posix', true, false, true);
  }

  signalGroup('SIGKILL');
  const gone = await waitForProcessGone(kill, -pgid, killGraceMs);
  return cleanupResult('posix', true, true, gone);
}

export async function terminateOwnedProcess(child, {
  platform = process.platform,
  kill = process.kill,
  termGraceMs = TERM_GRACE_MS,
  killGraceMs = KILL_GRACE_MS,
} = {}) {
  if (platform === 'win32') {
    return cleanupResult(platform, false, false, false, WINDOWS_RUNNER_ERROR);
  }
  return terminatePosixGroup(child, { kill, termGraceMs, killGraceMs });
}

export async function runCommand(command, args, {
  cwd = repoRoot,
  env = process.env,
  stdio = 'inherit',
  timeoutMs = OUTER_TIMEOUT_MS,
  platform = process.platform,
  kill = process.kill,
  spawnProcess = spawn,
  termGraceMs = TERM_GRACE_MS,
  killGraceMs = KILL_GRACE_MS,
  handleSignals = true,
} = {}) {
  // taskkill cannot establish ownership of orphans after their parent exits.
  // Refuse before spawning instead of claiming unverified Windows tree cleanup.
  if (platform === 'win32') {
    return { code: null, signal: null, timedOut: false, cancelled: false,
      cancelledSignal: null, error: new Error(WINDOWS_RUNNER_ERROR),
      cleanup: cleanupResult(platform, false, false, true) };
  }
  let child;
  try {
    child = spawnProcess(command, args, {
      cwd, env, stdio,
      detached: platform !== 'win32',
      windowsHide: platform === 'win32',
    });
  } catch (error) {
    return {
      code: null, signal: null, timedOut: false, cancelled: false,
      cancelledSignal: null, error, cleanup: cleanupResult(platform, false, false, true),
    };
  }

  let spawnError = null;
  const closePromise = new Promise(resolveClose => {
    child.once('error', error => { spawnError = error; });
    child.once('close', (code, signal) => resolveClose({ code, signal }));
  });
  let cleanupPromise;
  const stop = () => {
    if (!cleanupPromise) {
      cleanupPromise = terminateOwnedProcess(child, {
        platform, kill, spawnProcess, termGraceMs, killGraceMs,
      }).catch(error => cleanupResult(platform, true, true, false, error));
    }
    return cleanupPromise;
  };

  let cancelledSignal = null;
  let resolveInterrupt;
  const interruptPromise = new Promise(resolveInterruptPromise => {
    resolveInterrupt = resolveInterruptPromise;
  });
  const signalHandlers = new Map();
  if (handleSignals) {
    for (const signal of ['SIGINT', 'SIGTERM']) {
      const handler = () => {
        if (!cancelledSignal) {
          cancelledSignal = signal;
          resolveInterrupt(INTERRUPTED);
        }
        void stop();
      };
      signalHandlers.set(signal, handler);
      process.on(signal, handler);
    }
  }

  let timeoutHandle;
  const timeoutPromise = new Promise(resolveTimeout => {
    timeoutHandle = setTimeout(() => resolveTimeout(TIMEOUT), timeoutMs);
  });
  let outcome;
  let timedOut = false;
  try {
    const first = await Promise.race([
      closePromise,
      timeoutPromise,
      ...(handleSignals ? [interruptPromise] : []),
    ]);
    if (first === TIMEOUT || first === INTERRUPTED) {
      timedOut = first === TIMEOUT;
      await stop();
      // The process-group/PID cleanup is bounded independently of the child's
      // close event. A killed child normally closes immediately, but a close
      // event must not make the outer runner wait forever.
      outcome = await Promise.race([closePromise, delay(killGraceMs)]);
      if (!outcome || typeof outcome !== 'object') outcome = { code: null, signal: null };
    } else {
      outcome = first;
      // A completed child can leave an orphan in its owned process group.
      await stop();
    }
  } finally {
    clearTimeout(timeoutHandle);
    for (const [signal, handler] of signalHandlers) process.removeListener(signal, handler);
  }

  const cleanup = await stop();
  return {
    code: outcome.code,
    signal: outcome.signal,
    timedOut,
    cancelled: cancelledSignal !== null,
    cancelledSignal,
    error: spawnError,
    cleanup,
  };
}

export async function runCi(lane = 'all') {
  if (Number(process.versions.node.split('.')[0]) < 24) throw new Error('Node 24 or newer required');
  const files = selectTests(readdirSync(join(relayRoot, 'tests')), lane);
  if (!files.length) throw new Error('No relay tests selected');
  // Deliberately excludes opt-in public/native/model *-check.mjs probes.
  // Serial file execution bounds local workerd processes and deadline contention.
  const result = await runCommand(process.execPath,
    ['--test', '--test-concurrency=1', `--test-timeout=${TEST_TIMEOUT_MS}`,
      ...files.map(name => join(relayRoot, 'tests', name))],
    { cwd: repoRoot, env: testEnvironment(process.env), stdio: 'inherit', timeoutMs: OUTER_TIMEOUT_MS });
  if (result.error) console.error(result.error.message);
  if (!result.cleanup.ok) console.error(`Failed to clean up relay test process tree: ${result.cleanup.error ?? 'unknown error'}`);
  if (result.timedOut) {
    console.error(`Relay test suite exceeded its ${OUTER_TIMEOUT_MS / 60_000}-minute outer timeout`);
    return 124;
  }
  if (result.cancelled) {
    console.error(`Relay test suite cancelled by ${result.cancelledSignal}`);
    return SIGNAL_EXIT_CODES[result.cancelledSignal] ?? 1;
  }
  return result.error || !result.cleanup.ok ? 1 : (result.code ?? 1);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  if (process.argv.length > 3) {
    console.error('Usage: node tests/run-ci.mjs [all|unit|runtime]');
    process.exitCode = 1;
  } else {
    runCi(process.argv[2]).then(code => {
      process.exitCode = code;
    }).catch(error => {
      console.error(error.stack ?? error.message ?? String(error));
      process.exitCode = 1;
    });
  }
}
