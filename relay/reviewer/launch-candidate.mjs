// SPDX-License-Identifier: Apache-2.0
// Bounded isolated full-App candidate launcher; not public installation proof.

import { createHash } from 'node:crypto';
import { chmodSync, closeSync, fstatSync, lstatSync, mkdirSync, openSync, readFileSync, readSync, realpathSync, statSync, writeFileSync, accessSync, constants } from 'node:fs';
import { createServer, createConnection } from 'node:net';
import { basename, dirname, isAbsolute, join, resolve } from 'node:path';
import { spawn as nodeSpawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { prepare as prepareReviewer, validateTemplate, defaultTemplatePath } from './prepare.mjs';
import { acquireProfileLock, readProfileState, saveProfileState } from './profile-state.mjs';

export { prepareReviewer };
export const DEFAULT_DURATION_SECONDS = 900;
export const RESERVED_PORTS = new Set([7878, 1420, 18080, 18081, 18082, 18083]);
const SHA_RE = /^[a-f0-9]{64}$/;
const APP_SUFFIX = '.app/Contents/MacOS/wenlan-app';
const KNOWN_FLAGS = new Set(['--app', '--sha256', '--profile', '--cache', '--duration-seconds', '--confirm-synthetic-library', '--resume', '--help']);

export function renderHelp() {
  return [
    'Usage: node relay/reviewer/launch-candidate.mjs --app <abs .app/Contents/MacOS/wenlan-app> --sha256 <64lowerhex> --profile <abs-dir> --cache <abs-dir> [--duration-seconds 1..3600] [--resume] --confirm-synthetic-library',
    '',
    'Internal candidate entry only; verify signing and distribution separately.',
    '  --resume  Reopen a successfully stopped profile without re-seeding. Never clears a stale lock.',
    '            Without --resume the profile directory must not exist.',
    '  --help  Print this help. Performs no writes and spawns nothing.',
  ].join('\n');
}

export function parseArgs(argv) {
  const raw = Array.isArray(argv) ? argv : [];
  const seen = new Set();
  const values = {};
  let help = false;
  for (let i = 0; i < raw.length; i++) {
    const token = raw[i];
    const eq = token.indexOf('=');
    const name = eq === -1 ? token : token.slice(0, eq);
    const inline = eq === -1 ? undefined : token.slice(eq + 1);
    if (!name.startsWith('--')) throw new Error(`unknown argument: ${token}`);
    if (!KNOWN_FLAGS.has(name)) throw new Error(`unknown argument: ${name}`);
    if (seen.has(name)) throw new Error(`duplicate argument: ${name}`);
    seen.add(name);
    if (name === '--help') {
      if (inline !== undefined) throw new Error('--help takes no value');
      help = true;
      continue;
    }
    if (name === '--confirm-synthetic-library') {
      if (inline !== undefined) throw new Error('--confirm-synthetic-library takes no value');
      values.confirm = true;
      continue;
    }
    if (name === '--resume') {
      if (inline !== undefined) throw new Error('--resume takes no value');
      values.resume = true;
      continue;
    }
    let v = inline;
    if (v === undefined) {
      v = raw[i + 1];
      i += 1;
      if (v === undefined || v.startsWith('--')) throw new Error(`missing value for ${name}`);
    }
    if (!v) throw new Error(`missing value for ${name}`);
    if (name === '--app') values.app = v;
    else if (name === '--sha256') values.sha256 = v;
    else if (name === '--profile') values.profile = v;
    else if (name === '--cache') values.cache = v;
    else if (name === '--duration-seconds') values.dur = v;
  }
  if (help) {
    if (raw.length !== 1) throw new Error('--help takes no other arguments');
    return { help: true };
  }
  const m = [];
  if (!values.app) m.push('--app');
  if (!values.sha256) m.push('--sha256');
  if (!values.profile) m.push('--profile');
  if (!values.cache) m.push('--cache');
  if (!values.confirm) m.push('--confirm-synthetic-library');
  if (m.length) throw new Error(`missing required arguments: ${m.join(', ')}`);
  let durationSeconds = DEFAULT_DURATION_SECONDS;
  if (values.dur !== undefined) {
    if (!/^[0-9]+$/.test(values.dur)) throw new Error('--duration-seconds must be an integer 1..3600');
    durationSeconds = Number(values.dur);
    if (!Number.isInteger(durationSeconds) || durationSeconds < 1 || durationSeconds > 3600) throw new Error('--duration-seconds must be an integer 1..3600');
  }
  return { app: values.app, sha256: values.sha256, profile: values.profile, cache: values.cache, durationSeconds, confirmSyntheticLibrary: true, resume: values.resume === true };
}

// Canonicalize existing paths so /var matches /private/var readbacks.
function canonExisting(p, deps, label) {
  const rp = (deps.realpathSync ?? realpathSync)(p);
  if (typeof rp !== 'string' || !isAbsolute(rp)) throw new Error(`${label} did not resolve to an absolute path`);
  return rp;
}

export function preflight(parsed, deps = {}) {
  if ((deps.platform ?? process.platform) !== 'darwin') throw new Error('refusing to launch: macOS (darwin) only');
  if (!parsed || parsed.confirmSyntheticLibrary !== true) throw new Error('missing --confirm-synthetic-library consent');
  const { sha256, durationSeconds } = parsed;
  let { app, profile, cache } = parsed;
  if (typeof app !== 'string' || !isAbsolute(app)) throw new Error('--app must be an absolute path');
  if (typeof profile !== 'string' || !isAbsolute(profile)) throw new Error('--profile must be an absolute path');
  if (typeof cache !== 'string' || !isAbsolute(cache)) throw new Error('--cache must be an absolute path');
  if (!SHA_RE.test(sha256 ?? '')) throw new Error('--sha256 must be 64 lowercase hex');
  if (!Number.isInteger(durationSeconds) || durationSeconds < 1 || durationSeconds > 3600) throw new Error('--duration-seconds must be an integer 1..3600');
  if (!app.endsWith(APP_SUFFIX)) throw new Error('--app must be the exact .app/Contents/MacOS/wenlan-app executable path');
  const lstat = deps.lstatSync ?? lstatSync;
  const stat = deps.statSync ?? statSync;
  const readFile = deps.readFileSync ?? readFileSync;
  // Existing profiles are allowed only through explicit resume validation.
  try {
    const info = lstat(profile);
    if (!parsed.resume) throw new Error('refusing existing profile: use --resume for a verified profile');
    if (!info.isDirectory() || info.isSymbolicLink()) throw new Error('resume profile must be a real directory');
  } catch (err) {
    if (err?.code !== 'ENOENT' || parsed.resume) throw err;
  }
  // Parent must already exist and be a directory.
  const parent = dirname(profile);
  let pst;
  try {
    pst = stat(parent);
  } catch {
    throw new Error('--profile parent must already exist');
  }
  if (!pst.isDirectory()) throw new Error('--profile parent must already exist');
  // Resolve existing app/cache and the profile parent; store canonical paths.
  app = canonExisting(app, deps, '--app');
  if (!app.endsWith(APP_SUFFIX)) throw new Error('--app resolves outside an App bundle executable path');
  cache = canonExisting(cache, deps, '--cache');
  const canonParent = canonExisting(parent, deps, '--profile parent');
  profile = join(canonParent, basename(profile));
  // App must be a regular file with execute bits and access.
  let ast;
  try {
    ast = stat(app);
  } catch {
    throw new Error('--app is not a valid executable file');
  }
  if (!ast.isFile()) throw new Error('--app is not a valid executable file');
  if ((ast.mode & 0o111) === 0) throw new Error('--app is not executable (missing execute bits)');
  try {
    (deps.accessSync ?? accessSync)(app, (deps.constants ?? constants).X_OK);
  } catch {
    throw new Error('--app is not executable (access denied)');
  }
  let cst;
  try {
    cst = stat(cache);
  } catch {
    throw new Error('--cache must be an existing directory');
  }
  if (!cst.isDirectory()) throw new Error('--cache must be an existing directory');
  const hashFile = deps.hashFile ?? ((p) => createHash('sha256').update(readFile(p)).digest('hex'));
  if (hashFile(app) !== sha256) throw new Error('app hash mismatch');
  const templatePath = deps.templatePath ?? defaultTemplatePath();
  let template;
  try {
    template = JSON.parse(readFile(templatePath, 'utf8'));
  } catch (err) {
    throw new Error(`cannot read submission template: ${err?.message ?? err}`);
  }
  validateTemplate(template);
  return { app, profile, cache, templatePath };
}

export function profileLayout(profile) {
  const library = join(profile, 'library');
  return { profile, library, home: join(library, 'home'), pages: join(library, 'pages'), config: join(library, 'config.json'), logs: join(profile, 'logs'), socket: join(profile, 'tauri-mcp.sock'), mcpCache: join(profile, 'mcp-cache'), preparationReceipt: join(profile, 'reviewer-preparation.json'), launchReceipt: join(profile, 'reviewer-launch-receipt.json'), log: join(profile, 'logs', 'app.log') };
}

export function createProfile(canon, deps = {}) {
  const mkdir = deps.mkdirSync ?? mkdirSync;
  const writeFile = deps.writeFileSync ?? writeFileSync;
  const chmod = deps.chmodSync ?? chmodSync;
  const layout = profileLayout(canon.profile);
  mkdir(layout.profile, { mode: 0o700, recursive: false });
  chmod(layout.profile, 0o700);
  for (const d of [layout.library, layout.home, layout.pages, layout.logs, layout.mcpCache]) {
    mkdir(d, { mode: 0o700, recursive: false });
    chmod(d, 0o700);
  }
  writeFile(layout.config, `${JSON.stringify({ knowledge_path: layout.pages, setup_completed: true, reranker_mode: 'off' }, null, 2)}\n`, { mode: 0o600, flag: 'wx' });
  chmod(layout.config, 0o600);
  return layout;
}

export function buildEnv(canon, layout, ports, parentEnv = process.env) {
  const env = {};
  if (parentEnv.PATH !== undefined) env.PATH = parentEnv.PATH;
  if (parentEnv.LANG !== undefined) env.LANG = parentEnv.LANG;
  if (parentEnv.TMPDIR !== undefined) env.TMPDIR = parentEnv.TMPDIR;
  env.HOME = layout.home;
  env.USERPROFILE = layout.home;
  env.WENLAN_DATA_DIR = layout.library;
  env.WENLAN_DEV_STATE_DIR = layout.profile;
  env.WENLAN_DEV_APP_ID = 'com.wenlan.desktop.dev.relay-review';
  env.WENLAN_DEV_TAURI_MCP_SOCKET = layout.socket;
  env.WENLAN_PORT = String(ports.daemonPort);
  env.WENLAN_DEV_UI_PORT = String(ports.uiPort);
  env.WENLAN_DEV_REMOTE_PORT_START = '26120';
  env.WENLAN_SPACE = 'atlas-review';
  env.WENLAN_MCP_CACHE_DIR = layout.mcpCache;
  env.WENLAN_TEST_FASTEMBED_CACHE = canon.cache;
  env.WENLAN_RERANKER_MODE = 'off';
  env.WENLAN_NO_AUTOSTART = '1';
  return env;
}

export function checkPorts(ports) {
  if (!ports || !Number.isInteger(ports.daemonPort) || !Number.isInteger(ports.uiPort)) throw new Error('invalid allocated ports');
  if (ports.daemonPort === ports.uiPort) throw new Error('daemon and UI ports must be distinct');
  for (const p of [ports.daemonPort, ports.uiPort]) {
    if (p <= 0 || p > 65535 || RESERVED_PORTS.has(p)) throw new Error(`allocated port ${p} is reserved or out of range`);
  }
  return ports;
}

export function allocatePorts(deps = {}) {
  const listenPort = deps.listenPort ?? (async () => {
    const server = createServer();
    await new Promise((res, rej) => {
      server.once('error', rej);
      server.listen(0, '127.0.0.1', res);
    });
    const port = server.address().port;
    await new Promise((res) => server.close(res));
    return port;
  });
  return (async () => {
    for (let i = 0; i < 50; i++) {
      const daemonPort = await listenPort();
      const uiPort = await listenPort();
      if (daemonPort !== uiPort && !RESERVED_PORTS.has(daemonPort) && !RESERVED_PORTS.has(uiPort)) return { daemonPort, uiPort };
    }
    throw new Error('could not allocate distinct non-reserved loopback ports');
  })();
}

export function portClosed(port, deps = {}) {
  const connect = deps.connect ?? ((p) => new Promise((resolve, reject) => {
    const socket = createConnection({ host: '127.0.0.1', port: p });
    socket.once('connect', () => {
      socket.destroy();
      resolve(false);
    });
    socket.once('error', (err) => (err?.code === 'ECONNREFUSED' ? resolve(true) : reject(err)));
    socket.setTimeout(2000, () => {
      socket.destroy();
      reject(new Error('listener probe timeout'));
    });
  }));
  return connect(port);
}

// Single bounded readiness implementation. isExited() lets the caller report
// early child exit instead of waiting out the full timeout.
export async function pollReadiness({ daemonUrl, expectedPath, fetchImpl = globalThis.fetch, timeoutMs = 40_000, intervalMs = 200, isExited = () => null, sleepFn = null }) {
  const sleep = sleepFn ?? ((ms) => new Promise((res) => setTimeout(res, ms)));
  const deadline = Date.now() + timeoutMs;
  let lastError = 'not attempted';
  while (Date.now() < deadline) {
    const term = isExited();
    if (term) throw new Error(`app exited before readiness: ${JSON.stringify(term)}`);
    try {
      const res = await fetchImpl(`${daemonUrl}/api/knowledge/path`, { redirect: 'error', signal: AbortSignal.timeout(5000) });
      if (res?.ok) {
        let body;
        try {
          body = await res.json();
        } catch {
          throw new Error('readiness returned non-JSON');
        }
        if (typeof body?.path !== 'string' || body.path.length === 0) throw new Error('readiness returned malformed path');
        if (body.path !== expectedPath) throw new Error(`knowledge path mismatch: expected ${expectedPath} got ${body.path}`);
        return true;
      }
      lastError = `status ${res?.status}`;
    } catch (err) {
      if (/knowledge path mismatch|non-JSON|malformed path/.test(err?.message ?? '')) throw err;
      const term2 = isExited();
      if (term2) throw new Error(`app exited before readiness: ${JSON.stringify(term2)}`);
      lastError = err?.message ?? String(err);
    }
    await sleep(intervalMs);
  }
  const term = isExited();
  if (term) throw new Error(`app exited before readiness: ${JSON.stringify(term)}`);
  throw new Error(`daemon readiness timeout: ${lastError}`);
}

// Cancellable wait for child terminal state; timer cleared on early exit.
function waitExit(exitPromise, ms) {
  return new Promise((resolve) => {
    const t = setTimeout(() => resolve(null), ms);
    exitPromise.then((r) => {
      clearTimeout(t);
      resolve(r);
    });
  });
}

export async function launch(parsed, deps = {}) {
  const canon = preflight(parsed, deps);
  const layout = parsed.resume ? profileLayout(canon.profile) : createProfile(canon, deps);
  const lock = acquireProfileLock(layout.profile);
  try {
    const prior = parsed.resume ? validateResume(canon, parsed, layout) : null;
    const ports = checkPorts(prior?.ports ?? await (deps.allocatePorts ?? (() => allocatePorts(deps)))());
    const recordStoppedProfile = (receipt) => {
      try {
        const fixture = privateJson(layout.preparationReceipt);
        if (fixture.data.status !== 'prepared') throw new Error('preparation receipt is not prepared');
        saveProfileState(layout.profile, { schema: 1, kind: 'wenlan-isolated-review',
          profile: canon.profile, app: canon.app, cache: canon.cache, appSha256: parsed.sha256,
          preparationSha256: fixture.sha256, ports });
        lock.release();
      } catch (error) {
        receipt.status = 'failed';
        receipt.persistenceError = error.message;
        (deps.writeFileSync ?? writeFileSync)(layout.launchReceipt, JSON.stringify(receipt, null, 2), { mode: 0o600 });
        throw error;
      }
    };
    let receipt;
    try {
      receipt = await runCandidate(parsed, canon, layout, ports, deps);
    } catch (error) {
      if (error.code === 'cancelled' && error.receipt) recordStoppedProfile(error.receipt);
      throw error;
    }
    recordStoppedProfile(receipt);
    return receipt;
  } finally {
    lock.retain();
  }
}

function privateJson(path) {
  const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const info = fstatSync(fd);
    const limit = 128 * 1024;
    if (!info.isFile() || info.uid !== process.getuid() || (info.mode & 0o077) || info.size > limit) {
      throw new Error(`unsafe profile file: ${path}`);
    }
    const buffer = Buffer.alloc(limit + 1);
    const count = readSync(fd, buffer, 0, buffer.length, 0);
    if (count > limit) throw new Error('profile file too large');
    const bytes = buffer.subarray(0, count);
    return { data: JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)),
      sha256: createHash('sha256').update(bytes).digest('hex') };
  } finally {
    closeSync(fd);
  }
}

export function validateResume(canon, parsed, layout) {
  const state = readProfileState(layout.profile);
  for (const [key, value] of Object.entries({ profile: canon.profile, app: canon.app,
    cache: canon.cache, appSha256: parsed.sha256 })) {
    if (state[key] !== value) throw new Error(`profile ${key} does not match this candidate`);
  }
  for (const path of [layout.library, layout.home, layout.pages, layout.logs, layout.mcpCache]) {
    const info = lstatSync(path);
    if (!info.isDirectory() || info.isSymbolicLink() || realpathSync(path) !== path
      || info.uid !== process.getuid() || (info.mode & 0o077)) throw new Error(`unsafe profile directory: ${path}`);
  }
  const config = privateJson(layout.config).data;
  if (config.knowledge_path !== layout.pages) throw new Error('profile knowledge path changed');
  const fixture = privateJson(layout.preparationReceipt);
  if (fixture.sha256 !== state.preparationSha256 || fixture.data.status !== 'prepared') {
    throw new Error('profile preparation receipt changed or incomplete');
  }
  const previous = privateJson(layout.launchReceipt).data;
  if (!['completed', 'cancelled'].includes(previous.status) || previous.exit?.code !== 0
    || previous.exit.signal != null || previous.forcedKill || previous.profile !== layout.profile
    || previous.hash !== parsed.sha256) throw new Error('previous launch did not stop cleanly');
  return state;
}

async function runCandidate(parsed, canon, layout, ports, deps) {
  const noTrap = deps.noSignalTrap === true;
  const stat = deps.statSync ?? statSync;
  const writeFile = deps.writeFileSync ?? writeFileSync;
  const chmod = deps.chmodSync ?? chmodSync;
  const daemonUrl = `http://127.0.0.1:${ports.daemonPort}`;
  const env = buildEnv(canon, layout, ports, deps.parentEnv ?? process.env);
  const closedFn = deps.portClosedFn ?? ((p) => portClosed(p, deps));
  const emit = deps.emit ?? ((l) => console.log(l));
  const fetchImpl = deps.fetchImpl ?? globalThis.fetch;
  const prepareFn = deps.prepareFn ?? prepareReviewer;
  const graceMs = deps.graceMs ?? 15_000;
  const killMs = deps.killMs ?? 10_000;
  const pollMs = deps.pollMs ?? 40_000;
  const pollIntervalMs = deps.pollIntervalMs ?? 200;
  const sleepFn = deps.sleep ?? ((ms) => new Promise((res) => setTimeout(res, ms)));

  if (!(await closedFn(ports.daemonPort))) throw new Error('refusing to reuse an occupied daemon port');

  let logBuf = '';
  const spawnFn = deps.spawnFn ?? nodeSpawn;
  let child;
  try {
    child = spawnFn(canon.app, [], { env, cwd: layout.profile, stdio: ['ignore', 'pipe', 'pipe'] });
  } catch (err) {
    const error = `spawn failed: ${err?.message ?? err}`;
    writeFile(layout.launchReceipt, JSON.stringify({ status: 'failed', error, pid: null,
      hash: parsed.sha256, profile: layout.profile, daemonUrl, exit: null }), { mode: 0o600, flag: 'wx' });
    throw new Error(error);
  }
  if (!child || typeof child.kill !== 'function' || typeof child.once !== 'function') throw new Error('spawn failed: invalid child');
  const pid = child.pid ?? null;
  let exitInfo = null;
  let childError = null;
  let resolveExit;
  const exitPromise = new Promise((res) => {
    resolveExit = res;
  });
  // No unhandled rejections: error event sets terminal state and resolves.
  child.once('error', (err) => {
    childError = err;
    if (!exitInfo && !child.pid) {
      exitInfo = { code: null, signal: null, spawnError: err?.message ?? String(err) };
      resolveExit(exitInfo);
    }
  });
  child.once('exit', (code, signal) => {
    if (!exitInfo) {
      exitInfo = { code, signal };
      resolveExit(exitInfo);
    }
  });
  for (const s of [child.stdout, child.stderr]) {
    if (s?.on) {
      s.on('data', (b) => {
        logBuf = `${logBuf}${String(b)}`.slice(-128 * 1024);
      });
      s.on('error', () => {});
    }
  }
  const isExited = () => exitInfo ?? (childError ? { error: childError.message } : null);
  let cancelled = false;
  const onSig = () => {
    cancelled = true;
    if (!exitInfo) {
      try {
        child.kill('SIGTERM');
      } catch {}
    }
  };
  if (!noTrap) {
    try {
      process.once('SIGINT', onSig);
      process.once('SIGTERM', onSig);
    } catch {}
  }
  let forcedKill = false;
  let phaseError = null;
  let prepared = false;
  try {
    await pollReadiness({ daemonUrl, expectedPath: layout.pages, fetchImpl, timeoutMs: pollMs, intervalMs: pollIntervalMs, isExited, sleepFn });
    if (cancelled) throw new Error('cancelled');
    if (exitInfo) throw new Error(`app exited before readiness: ${JSON.stringify(exitInfo)}`);
    if (!parsed.resume) {
      await prepareFn({ daemonUrl, knowledgePath: layout.pages, output: layout.preparationReceipt, confirmSyntheticLibrary: true }, { fetchImpl, templatePath: canon.templatePath });
    }
    if (cancelled) throw new Error('cancelled');
    if (isExited()) throw new Error(`app exited during preparation: ${JSON.stringify(isExited())}`);
    stat(layout.preparationReceipt); // must exist before READY
    prepared = true;
    await emit(`READY ${JSON.stringify({ profile: layout.profile, daemonUrl, preparationReceipt: layout.preparationReceipt, resumed: parsed.resume === true })}`);
    const windowMs = canon.durationSeconds ?? parsed.durationSeconds;
    const endAt = Date.now() + windowMs * 1000;
    while (Date.now() < endAt) {
      if (cancelled) break;
      if (isExited()) throw new Error(`app exited during run window: ${JSON.stringify(isExited())}`);
      const r = await waitExit(exitPromise, Math.min(deps.tickMs ?? 250, Math.max(1, endAt - Date.now())));
      if (cancelled) break;
      if (r) throw new Error(`app exited during run window: ${JSON.stringify(r)}`);
    }
    if (cancelled) throw new Error('cancelled');
    if (exitInfo) throw new Error(`app exited during run window: ${JSON.stringify(exitInfo)}`);
  } catch (err) {
    phaseError = cancelled && /^app exited before readiness/.test(err.message) ? new Error('cancelled') : err;
  } finally {
    // Single cleanup path: graceful stop, forced kill if needed, always await terminal.
    let cleanupError = null;
    if (!exitInfo) {
      try {
        child.kill('SIGTERM');
      } catch (err) {
        cleanupError = `SIGTERM signal failed: ${err?.message ?? err}`;
      }
      const r = await waitExit(exitPromise, graceMs);
      if (!exitInfo && !r) {
        forcedKill = true;
        cleanupError = [cleanupError, 'graceful shutdown failed; forced kill required'].filter(Boolean).join('; ');
        try {
          child.kill('SIGKILL');
        } catch (err) {
          cleanupError = [`SIGKILL signal failed: ${err?.message ?? err}`, cleanupError].filter(Boolean).join('; ');
        }
        await waitExit(exitPromise, killMs);
      }
    }
    if (!exitInfo) cleanupError = [cleanupError, 'owned child never reached terminal state'].filter(Boolean).join('; ');
    let portOpen = false;
    try {
      portOpen = !(await closedFn(ports.daemonPort));
    } catch (err) {
      cleanupError = [cleanupError, `daemon port probe failed: ${err?.message ?? err}`].filter(Boolean).join('; ');
    }
    if (portOpen) cleanupError = [cleanupError, 'daemon port still open after shutdown'].filter(Boolean).join('; ');
    const finalExit = exitInfo; // never synthesized
    const phaseFailed = phaseError && !(cancelled && phaseError.message === 'cancelled');
    const cleanExit = finalExit?.code === 0 && finalExit.signal == null;
    const status = phaseFailed || cleanupError || forcedKill || !cleanExit
      ? 'failed' : cancelled ? 'cancelled' : 'completed';
    const receipt = {
      status,
      pid,
      hash: parsed.sha256,
      app: canon.app,
      profile: layout.profile,
      daemonUrl,
      daemonPort: ports.daemonPort,
      uiPort: ports.uiPort,
      preparationReceipt: prepared ? layout.preparationReceipt : null,
      log: layout.log,
      exit: finalExit,
      forcedKill: forcedKill || undefined,
      durationSeconds: parsed.durationSeconds,
      verification: { relay: 'unchecked', oauth: 'unchecked' },
      notes: ['No grant creation or revocation is claimed by this launcher.'],
    };
    if (phaseError) receipt.error = phaseError.message === 'cancelled' && cancelled ? 'cancelled by signal' : (phaseError?.message ?? String(phaseError));
    if (cleanupError) receipt.cleanupError = cleanupError;
    // Log + receipt writes failing fail the run (never silent success).
    let writeError = null;
    try {
      writeFile(layout.log, logBuf, { mode: 0o600 });
    } catch (err) {
      writeError = `log write failed: ${err?.message ?? err}`;
    }
    try {
      chmod(layout.log, 0o600);
    } catch (err) {
      writeError = [writeError, `log chmod failed: ${err?.message ?? err}`].filter(Boolean).join('; ');
    }
    if (writeError) {
      receipt.status = 'failed';
      receipt.evidenceError = writeError;
    }
    try {
      writeFile(layout.launchReceipt, `${JSON.stringify(receipt, null, 2)}\n`, { mode: 0o600 });
    } catch (err) {
      writeError = [writeError, `receipt write failed: ${err?.message ?? err}`].filter(Boolean).join('; ');
    }
    try {
      chmod(layout.launchReceipt, 0o600);
    } catch (err) {
      writeError = [writeError, `receipt chmod failed: ${err?.message ?? err}`].filter(Boolean).join('; ');
    }
    if (!noTrap) {
      try {
        process.removeListener('SIGINT', onSig);
        process.removeListener('SIGTERM', onSig);
      } catch {}
    }
    const combined = [phaseError && phaseError.message !== 'cancelled' ? phaseError.message : (cancelled && phaseError ? 'cancelled by signal' : null), cleanupError, writeError].filter(Boolean).join(' | cleanup: ');
    // Cancelled-by-signal with clean shutdown yields cancelled status (throw to signal caller).
    if (writeError) throw new Error(combined);
    if (status === 'cancelled') {
      const err = new Error(combined || 'cancelled by signal');
      err.code = 'cancelled';
      err.receipt = receipt;
      throw err;
    }
    if (status === 'failed') {
      throw new Error(combined || `app lifecycle failed: ${JSON.stringify(finalExit)}`);
    }
    return receipt;
  }
}

async function main(argv) {
  let parsed;
  try {
    parsed = parseArgs(argv);
  } catch (err) {
    console.error(`launch-candidate: ${err?.message ?? err}`);
    console.error(renderHelp());
    process.exitCode = 2;
    return;
  }
  if (parsed?.help) {
    console.log(renderHelp());
    return;
  }
  try {
    await launch(parsed);
    console.log('launch-candidate: lifecycle complete');
  } catch (err) {
    if (err?.code === 'cancelled') {
      console.error(`launch-candidate cancelled: ${err?.message ?? err}`);
      process.exitCode = 130;
      return;
    }
    console.error(`launch-candidate failed: ${err?.message ?? err}`);
    process.exitCode = 1;
  }
}

const invokedAsMain = (() => {
  try {
    const entry = process.argv[1];
    if (!entry) return false;
    return resolve(entry) === fileURLToPath(import.meta.url);
  } catch {
    return false;
  }
})();

if (invokedAsMain) {
  await main(process.argv.slice(2));
}
