// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

if (!process.env.WENLAN_RELAY_TOOLCHAIN) throw new Error('WENLAN_RELAY_TOOLCHAIN required');
const requireTool = createRequire(join(resolve(process.env.WENLAN_RELAY_TOOLCHAIN), 'package.json'));
const { Miniflare } = requireTool('miniflare');
const { build } = requireTool('esbuild');
const origin = 'https://reverse-fixture.example';

function messages(socket) {
  const queued = [];
  const waiting = [];
  socket.addEventListener('message', event => {
    const frame = JSON.parse(event.data);
    const resolve = waiting.shift();
    if (resolve) resolve(frame); else queued.push(frame);
  });
  return () => queued.length ? Promise.resolve(queued.shift()) : new Promise(resolve => waiting.push(resolve));
}

test('real workerd WebSocket transports incremental SSE and propagates disconnect', { timeout: 20_000 }, async () => {
  const bundle = await build({ entryPoints: [fileURLToPath(new URL('./fixtures/reverse-worker.ts', import.meta.url))],
    bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022' });
  const runtime = new Miniflare({ modules: true, script: bundle.outputFiles[0].text,
    compatibilityDate: '2026-09-08', host: '127.0.0.1', port: 0,
    durableObjects: { AUTHORITY: { className: 'ReverseFixtureAuthority', useSQLite: true } },
  });
  let socket;
  // Retain rejection handlers while a paired WS assertion may fail before
  // its pending HTTP request is awaited; disposal still rejects that request.
  const dispatch = path => {
    const request = runtime.dispatchFetch(`${origin}${path}`);
    void request.catch(() => {});
    return request;
  };
  try {
    const upgraded = await runtime.dispatchFetch(`${origin}/socket`, { headers: { upgrade: 'websocket' } });
    assert.equal(upgraded.status, 101);
    socket = upgraded.webSocket;
    const next = messages(socket);
    socket.accept();
    const query = dispatch('/query');
    const request = await next();
    assert.equal(request.type, 'request');
    assert.equal(request.path, '/mcp');
    assert.equal(request.method, 'POST');
    assert.equal(JSON.parse(Buffer.from(request.body, 'base64').toString()).method, 'tools/list');
    const send = frame => socket.send(JSON.stringify({ v: 1, id: request.id, ...frame }));
    send({ type: 'response', status: 200, headers: { 'content-type': 'text/event-stream' } });
    const response = await query;
    assert.equal(response.status, 200);
    assert.equal(response.headers.get('cache-control'), 'no-store');
    const reader = response.body.getReader();
    const first = reader.read();
    const firstCredit = await next();
    assert.equal(firstCredit.type, 'credit'); assert.equal(firstCredit.seq, 0);
    send({ type: 'chunk', seq: 0, body: Buffer.from('data: first\n\n').toString('base64') });
    assert.equal(new TextDecoder().decode((await first).value), 'data: first\n\n');
    const second = reader.read();
    const secondCredit = await next();
    assert.equal(secondCredit.type, 'credit'); assert.equal(secondCredit.seq, 1);
    send({ type: 'chunk', seq: 1, body: Buffer.from('data: second\n\n').toString('base64') });
    assert.equal(new TextDecoder().decode((await second).value), 'data: second\n\n');
    send({ type: 'end' });
    assert.equal((await reader.read()).done, true);

    const interrupted = dispatch('/query');
    let nextRequest = await next();
    // workerd's HTTP bridge may pull once more before it receives end.
    if (nextRequest.type === 'credit') {
      assert.equal(nextRequest.id, request.id);
      assert.equal(nextRequest.seq, 2);
      nextRequest = await next();
    }
    assert.equal(nextRequest.type, 'request');
    assert.notEqual(nextRequest.id, request.id);
    socket.close(1000, 'Fixture finished');
    assert.equal((await interrupted).status, 503);
    assert.equal((await runtime.dispatchFetch(`${origin}/query`)).status, 503);
  } finally {
    try { socket?.close(); } catch { /* Already closed. */ }
    await runtime.dispose();
  }
});
