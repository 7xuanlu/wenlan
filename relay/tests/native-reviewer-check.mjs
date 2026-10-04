// SPDX-License-Identifier: Apache-2.0
// Opt-in native-binary check; never starts an installed service or public tunnel.
import assert from 'node:assert/strict';
import test from 'node:test';
import { execFile, spawn } from 'node:child_process';
import { mkdtemp, realpath, rm } from 'node:fs/promises';
import { createServer, createConnection } from 'node:net';
import { tmpdir } from 'node:os';
import { isAbsolute, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { setTimeout as delay } from 'node:timers/promises';

const exec = promisify(execFile);
const bin = process.env.WENLAN_NATIVE_BIN_DIR;
const cache = process.env.WENLAN_TEST_FASTEMBED_CACHE;
assert(bin && isAbsolute(bin) && cache && isAbsolute(cache), 'explicit native binaries and existing model cache required');
assert.notEqual(process.platform, 'win32', 'this POSIX signal/process driver is not Windows evidence');

async function freePort() {
  const server = createServer();
  await new Promise((resolve, reject) => server.once('error', reject).listen(0, '127.0.0.1', resolve));
  const port = server.address().port;
  await new Promise(resolve => server.close(resolve));
  return port;
}

async function closed(port) {
  const connected = await new Promise(resolve => {
    const socket = createConnection({ host: '127.0.0.1', port });
    socket.once('connect', () => { socket.destroy(); resolve(true); });
    socket.once('error', () => resolve(false));
    socket.setTimeout(1000, () => { socket.destroy(); resolve(true); });
  });
  assert.equal(connected, false, 'owned listener must be closed');
}

function start(binary, args, env) {
  const child = spawn(binary, args, { env, stdio: ['ignore', 'pipe', 'pipe'] });
  let tail = '';
  let error;
  for (const pipe of [child.stdout, child.stderr]) pipe.on('data', data => { tail = (tail + data).slice(-8192); });
  const exited = new Promise(resolve => {
    child.once('error', value => { error = value; resolve({ error }); });
    child.once('exit', (code, signal) => resolve({ code, signal }));
  });
  return { child, exited, tail: () => tail, error: () => error };
}

async function stop(task) {
  if (!task) return;
  if (task.child.exitCode === null && task.child.signalCode === null) task.child.kill('SIGTERM');
  const stopped = await Promise.race([task.exited, delay(5000, null, { ref: false })]);
  if (!stopped) {
    task.child.kill('SIGKILL');
    await task.exited;
    throw new Error('Native shutdown needed forced termination');
  }
}

async function ready(task, url) {
  const deadline = Date.now() + 20_000;
  while (Date.now() < deadline) {
    assert.equal(task.error(), undefined, 'native process must start');
    assert.equal(task.child.exitCode, null, task.tail());
    assert.equal(task.child.signalCode, null, task.tail());
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(1000) });
      if (response.ok) { await response.arrayBuffer(); return; }
      await response.body?.cancel();
    } catch {}
    await delay(100);
  }
  throw new Error(`Native readiness deadline: ${task.tail()}`);
}

test('fresh reviewer library survives actual daemon and MCP process restart with OAuth queries', { timeout: 120_000 }, async () => {
  const scratch = await mkdtemp(join(tmpdir(), 'wenlan-native-reviewer-'));
  const root = join(await realpath(scratch), 'library');
  const env = { PATH: process.env.PATH, LANG: 'en_US.UTF-8',
    ...(process.env.TMPDIR ? { TMPDIR: process.env.TMPDIR } : {}),
    HOME: join(root, 'home'), USERPROFILE: join(root, 'home'), WENLAN_DATA_DIR: root,
    WENLAN_NO_AUTOSTART: '1', WENLAN_SPACE: 'atlas-review',
    WENLAN_RERANKER_MODE: 'off', WENLAN_TEST_FASTEMBED_CACHE: cache,
    WENLAN_MCP_CACHE_DIR: join(root, 'mcp-cache'), WENLAN_TEST_TOKEN: 'b'.repeat(64) };
  let daemon, mcp;
  try {
    await exec(join(bin, 'examples/seed_reviewer_library'), [root], { env, timeout: 45_000, maxBuffer: 16_384 });
    for (let pass = 0; pass < 2; pass++) {
      const daemonPort = await freePort();
      const mcpPort = await freePort();
      daemon = start(join(bin, 'wenlan-server'), [], { ...env, WENLAN_PORT: String(daemonPort),
        WENLAN_BIND_ADDR: `127.0.0.1:${daemonPort}` });
      const daemonUrl = `http://127.0.0.1:${daemonPort}`;
      await ready(daemon, `${daemonUrl}/api/health`);
      const knowledge = await fetch(`${daemonUrl}/api/knowledge/path`, { signal: AbortSignal.timeout(2000) });
      assert.deepEqual(await knowledge.json(), { path: join(root, 'pages') });
      mcp = start(join(bin, 'wenlan-mcp'), ['--origin-url', daemonUrl, '--agent-name', 'reviewer-native',
        'serve', '--tool-profile', 'query-only', '--host', '127.0.0.1', '--port', String(mcpPort),
        '--token-env', 'WENLAN_TEST_TOKEN'], env);
      const mcpUrl = `http://127.0.0.1:${mcpPort}`;
      await ready(mcp, `${mcpUrl}/health`);
      const result = await exec(process.execPath, [fileURLToPath(new URL('./real-backend-check.mjs', import.meta.url))], {
        env: { ...env, WENLAN_TEST_MCP_URL: mcpUrl, WENLAN_RELAY_TOOLCHAIN: process.env.WENLAN_RELAY_TOOLCHAIN },
        timeout: 50_000, maxBuffer: 16_384,
      });
      assert(result.stdout.includes('pass 1'), 'real OAuth check must complete');
      await stop(mcp); mcp = undefined;
      await closed(mcpPort);
      await stop(daemon); daemon = undefined;
      await closed(daemonPort);
    }
  } finally {
    try { await stop(mcp); } finally {
      await stop(daemon);
      await rm(scratch, { recursive: true, force: true });
    }
  }
});
