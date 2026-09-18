// SPDX-License-Identifier: Apache-2.0
// Explicit diagnostic only: exposes an empty 401 responder, never a library.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { lookup, Resolver } from 'node:dns/promises';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, isAbsolute } from 'node:path';
import { createHash } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';

assert.equal(process.env.WENLAN_TEST_TUNNEL_DNS, '1', 'explicit diagnostic opt-in required');
const binary = process.env.WENLAN_CLOUDFLARED_BIN;
assert(binary && isAbsolute(binary));
const home = await mkdtemp(join(tmpdir(), 'wenlan-tunnel-dns-'));
const server = createServer((_request, response) => response.writeHead(401).end());
let child, exit, failed = false;
try {
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  child = spawn(binary, ['tunnel', '--no-autoupdate', '--url', `http://127.0.0.1:${server.address().port}`], {
    env: { PATH: process.env.PATH, HOME: home, USERPROFILE: home },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  exit = new Promise(resolve => {
    child.once('exit', resolve);
    child.once('error', () => { failed = true; resolve(); });
  });
  let buffer = '', hostname, connected = false;
  for (const stream of [child.stdout, child.stderr]) stream.on('data', data => {
    buffer = (buffer + data).slice(-16384);
    // Match the complete printed banner line, not arbitrary diagnostic URLs.
    hostname ??= /\|\s+https:\/\/([a-z0-9-]+\.trycloudflare\.com)\s+\|/.exec(buffer)?.[1];
    connected ||= buffer.includes('Registered tunnel connection');
  });
  const deadline = Date.now() + 30_000;
  while ((!hostname || !connected) && Date.now() < deadline) {
    assert(!failed && child.exitCode === null && child.signalCode === null, 'tunnel exited');
    await delay(100);
  }
  assert(hostname && connected, 'complete banner and registered connection required');
  const resolver = new Resolver({ timeout: 1500, tries: 1 });
  const code = async operation => {
    try { await operation(); return 'resolved'; }
    catch (error) { return /^[A-Z0-9_]+$/.test(error.code ?? '') ? error.code : 'failed'; }
  };
  const identify = createHash('sha256').update(hostname).digest('hex').slice(0, 12);
  for (const pause of [0, 10_000, 20_000]) {
    await delay(pause);
    const observed = { hostnameHash: identify, connected,
      lookup: await code(() => lookup(hostname)),
      resolve4: await code(() => resolver.resolve4(hostname)) };
    try {
      const response = await fetch(`https://cloudflare-dns.com/dns-query?name=${hostname}&type=A`, {
        headers: { accept: 'application/dns-json' }, signal: AbortSignal.timeout(5000), redirect: 'error',
      });
      assert.equal(response.status, 200);
      const body = await response.json();
      observed.publicDnsStatus = body.Status;
      observed.publicDnsAnswerCount = body.Answer?.length ?? 0;
    } catch { observed.publicDns = 'unavailable'; }
    try {
      const response = await fetch(`https://${hostname}/connector-info`, { signal: AbortSignal.timeout(5000), redirect: 'error' });
      observed.httpStatus = response.status;
      await response.body?.cancel();
    } catch { observed.httpStatus = 'unavailable'; }
    process.stdout.write(`${JSON.stringify(observed)}\n`);
  }
} finally {
  if (child && child.exitCode === null && child.signalCode === null) child.kill('SIGTERM');
  if (exit) {
    if (!await Promise.race([exit.then(() => true), delay(5000, false, { ref: false })])) {
      child.kill('SIGKILL');
      assert(await Promise.race([exit.then(() => true), delay(3000, false, { ref: false })]), 'owned tunnel exit unconfirmed');
    }
  }
  server.closeAllConnections();
  await new Promise(resolve => server.close(resolve));
  await rm(home, { recursive: true, force: true });
}
