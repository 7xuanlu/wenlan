// SPDX-License-Identifier: Apache-2.0
// Explicit cross-language integration lane; not part of the fast Node suite.
import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'node:http';
import { spawn } from 'node:child_process';

if (!process.env.WENLAN_RELAY_TOOLCHAIN) throw new Error('WENLAN_RELAY_TOOLCHAIN required');
const requireTool = createRequire(join(resolve(process.env.WENLAN_RELAY_TOOLCHAIN), 'package.json'));
const { Miniflare } = requireTool('miniflare');
const { build } = requireTool('esbuild');
const { WebSocketServer } = requireTool('ws');
const origin = 'https://relay.wenlan.app';
const reverseRuntime = process.env.WENLAN_RELAY_CONTRACT_TRANSPORT === 'reverse-runtime';
const reverse = reverseRuntime || process.env.WENLAN_RELAY_CONTRACT_TRANSPORT === 'reverse';

test('native Rust client exercises the actual standalone Worker contract', { timeout: 120_000 }, async t => {
  const bundle = await build({ entryPoints: [fileURLToPath(new URL('../src/worker.ts', import.meta.url))],
    bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022',
    external: ['cloudflare:workers'], loader: { '.png': 'binary' } });
  let tunnelCalls = 0;
  let localCalls = 0;
  let localToken = 'b'.repeat(64);
  let reverseEnrollments = 0;
  const runtime = new Miniflare({ modules: true, script: bundle.outputFiles[0].text,
    compatibilityDate: '2026-09-08', compatibilityFlags: ['enable_request_signal'], host: '127.0.0.1', port: 0,
    bindings: { PUBLIC_ORIGIN: origin }, kvNamespaces: ['OAUTH_KV'],
    durableObjects: { AUTHORITY: { className: 'RelayAuthority', useSQLite: true } },
    outboundService: request => {
      tunnelCalls++;
      if (reverse) return new Response(null, { status: 502 });
      const url = new URL(request.url);
      if (url.origin !== 'https://synthetic.trycloudflare.com') return new Response(null, { status: 502 });
      if (!request.headers.has('authorization')) return new Response(null, { status: 401 });
      if (request.headers.get('authorization') !== `Bearer ${'b'.repeat(64)}`) return new Response(null, { status: 401 });
      if (url.pathname === '/connector-info') return Response.json({ contract_version: 1, server: 'wenlan-mcp',
        tool_profile: 'query-only', authentication: 'bearer', space: 'review' });
      if (url.pathname === '/mcp') return Response.json({ jsonrpc: '2.0', id: 1, result: { synthetic: true } },
        { headers: { 'mcp-session-id': 'synthetic-native-contract' } });
      return new Response(null, { status: 404 });
    },
  });
  // Only the test transport is loopback HTTP. The Worker sees its real public
  // issuer/resource, and no JSON or authentication result is rewritten.
  let droppedRevocationReply = false;
  const revocationStatuses = [];
  const bridgeFailures = [];
  const localMcp = createServer(async (request, response) => {
    try {
      localCalls++;
      let length = 0;
      for await (const chunk of request) {
        length += chunk.length;
        if (length > 64 * 1024) { response.writeHead(413).end(); return; }
      }
      if (request.headers.authorization !== `Bearer ${localToken}`) { response.writeHead(401).end(); return; }
      response.setHeader('content-type', 'application/json');
      if (request.url === '/connector-info') response.end(JSON.stringify({ contract_version: 1,
        server: 'wenlan-mcp', tool_profile: 'query-only', authentication: 'bearer', space: 'review' }));
      else if (request.url === '/mcp') {
        response.setHeader('mcp-session-id', 'synthetic-native-contract');
        response.end(JSON.stringify({ jsonrpc: '2.0', id: 1, result: { synthetic: true } }));
      } else response.writeHead(404).end();
    } catch (error) {
      bridgeFailures.push({ stage: 'local-mcp', name: error?.name });
      response.destroy();
    }
  });
  const bridge = createServer(async (request, response) => {
    let stage = 'buffer';
    try {
      const chunks = [];
      let length = 0;
      for await (const chunk of request) {
        length += chunk.length;
        if (length > 16 * 1024) { response.writeHead(413).end(); return; }
        chunks.push(chunk);
      }
      const headers = new Headers();
      for (const [key, value] of Object.entries(request.headers)) {
        if (['host', 'connection', 'content-length', 'transfer-encoding'].includes(key) || value === undefined) continue;
        for (const item of Array.isArray(value) ? value : [value]) headers.append(key, item);
      }
      stage = 'dispatch';
      if (reverse && request.method === 'POST' && request.url === '/devices/reverse') {
        reverseEnrollments++;
        // Fixture wiring only: match the backend token created by the real
        // native profile without changing the request delivered to the Worker.
        const candidate = JSON.parse(Buffer.concat(chunks).toString());
        assert.match(candidate.backendToken, /^[a-f0-9]{64}$/);
        localToken = candidate.backendToken;
      }
      const result = await runtime.dispatchFetch(`${origin}${request.url}`, { method: request.method, headers,
        // Auth comes only from explicit native headers. Undici's implicit
        // credential retry cannot replay Miniflare's streamed POST body on 401.
        credentials: 'omit', redirect: 'manual', body: length ? Buffer.concat(chunks) : undefined });
      if (request.url === '/devices/revoke') revocationStatuses.push(result.status);
      if (request.url === '/devices/revoke' && result.status === 200 && !droppedRevocationReply) {
        await result.arrayBuffer();
        droppedRevocationReply = true;
        request.socket.destroy();
        return;
      }
      response.writeHead(result.status, Object.fromEntries(result.headers));
      response.end(Buffer.from(await result.arrayBuffer()));
    } catch (error) {
      bridgeFailures.push({ stage, name: error?.name, message: error?.message,
        cause: error?.cause?.message, code: error?.cause?.code });
      response.writeHead(503).end('Synthetic fixture unavailable');
    }
  });
  const upgrades = new WeakMap();
  const sockets = new WebSocketServer({ noServer: true });
  sockets.on('headers', (headers, request) => {
    headers.push(`x-wenlan-connection-id: ${upgrades.get(request)}`);
  });
  bridge.on('upgrade', async (request, socket, head) => {
    socket.on('error', () => {});
    try {
      const headers = new Headers();
      for (const [key, value] of Object.entries(request.headers)) {
        if (key === 'host' || value === undefined) continue;
        for (const item of Array.isArray(value) ? value : [value]) headers.append(key, item);
      }
      const result = await runtime.dispatchFetch(`${origin}${request.url}`, {
        method: 'GET', headers, credentials: 'omit', redirect: 'manual',
      });
      if (result.status !== 101) {
        socket.end(`HTTP/1.1 ${result.status} Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n`);
        await result.body?.cancel(); return;
      }
      upgrades.set(request, result.headers.get('x-wenlan-connection-id'));
      sockets.handleUpgrade(request, socket, head, client => {
        const peer = result.webSocket;
        peer.addEventListener('message', event => { if (client.readyState === 1) client.send(event.data); });
        peer.addEventListener('close', () => client.close(1000));
        peer.addEventListener('error', () => client.close(1000));
        client.on('message', (data, binary) => { try { peer.send(binary ? data : data.toString()); } catch { client.close(1000); } });
        client.on('close', () => { try { peer.close(1000); } catch { /* Already closed. */ } });
        client.on('error', () => { try { peer.close(1000); } catch { /* Already closed. */ } });
        peer.accept();
      });
    } catch (error) {
      bridgeFailures.push({ stage: 'upgrade', name: error?.name }); socket.destroy();
    }
  });
  let child;
  let completion;
  let timer;
  const cancel = () => {
    if (child && child.exitCode === null && child.signalCode === null) {
      try { process.kill(-child.pid, 'SIGKILL'); } catch (error) { if (error.code !== 'ESRCH') throw error; }
    }
  };
  try {
    if (reverse) await new Promise((resolve, reject) => {
      localMcp.once('error', reject); localMcp.listen(0, '127.0.0.1', resolve);
    });
    await new Promise((resolve, reject) => { bridge.once('error', reject); bridge.listen(0, '127.0.0.1', resolve); });
    const endpoint = `http://127.0.0.1:${bridge.address().port}`;
    child = spawn('cargo', ['test', '-p', 'wenlan-app', '--lib', 'remote_relay::tests::actual_worker_contract', '--offline', '--', '--ignored', '--exact'], {
      cwd: fileURLToPath(new URL('../../', import.meta.url)),
      env: { ...process.env, WENLAN_NO_AUTOSTART: '1', TAURI_CONFIG: '{"bundle":{"externalBin":[]}}', WENLAN_RELAY_CONTRACT_ORIGIN: endpoint,
        ...(reverse ? { WENLAN_RELAY_CONTRACT_MCP_PORT: String(localMcp.address().port) } : {}) },
      stdio: ['ignore', 'pipe', 'pipe'], detached: true,
    });
    let output = '';
    const capture = chunk => { output = (output + chunk.toString()).slice(-10_000); };
    child.stdout.on('data', capture);
    child.stderr.on('data', capture);
    completion = new Promise((resolve, reject) => { child.once('error', reject); child.once('exit', (code, signal) => resolve({ code, signal })); });
    t.signal.addEventListener('abort', cancel, { once: true });
    timer = setTimeout(cancel, 90_000);
    const result = await completion;
    assert.equal(result.code, 0, `${output}\nRevocation HTTP statuses: ${JSON.stringify(revocationStatuses)}\nBridge failures: ${JSON.stringify(bridgeFailures)}`);
    assert.match(output, /1 passed; 0 failed/);
    assert.deepEqual(bridgeFailures, [], 'the fixture must not hide transport failures');
    assert.equal(droppedRevocationReply, true, 'the committed revoke reply must actually be lost');
    if (reverse) { assert.equal(tunnelCalls, 0); assert(localCalls >= 3); assert.equal(reverseEnrollments, 1); }
  } finally {
    clearTimeout(timer);
    t.signal.removeEventListener('abort', cancel);
    cancel();
    if (completion) await completion;
    for (const client of sockets.clients) client.terminate();
    await new Promise(resolve => sockets.close(resolve));
    if (localMcp.listening) {
      localMcp.closeAllConnections(); await new Promise(resolve => localMcp.close(resolve));
    }
    bridge.closeAllConnections();
    await new Promise(resolve => bridge.close(resolve));
    await runtime.dispose();
  }
});
