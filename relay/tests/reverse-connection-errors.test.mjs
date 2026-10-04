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

test('reverse connection error classification preserves authority failures', { timeout: 20_000 }, async t => {
  // Execute the actual class in its Worker environment; only its socket and
  // authority are synthetic. Production code has no test-only branches.
  const bundled = await build({ stdin: {
    resolveDir: fileURLToPath(new URL('../src', import.meta.url)),
    contents: `import { ReverseConnections, ReverseChannelUnavailable } from './reverse-connections.ts';
export default { async fetch(request) {
  const mode = new URL(request.url).pathname.slice(1);
  let reads = 0, sent = 0, closed = false;
  const id = 'c'.repeat(43);
  const socket = { readyState: WebSocket.OPEN,
    deserializeAttachment: () => ({version:1,deviceId:'d'.repeat(43),connectionId:id,generation:1}),
    send() { sent++; }, close() { closed = true; }
  };
  const connections = new ReverseConnections({getWebSockets:()=>mode==='missing'?[]:[socket]}, {
    authenticate: async()=>true, activate: async()=>null,
    current: async()=>{ reads++; if(mode==='reject') throw new Error('AUTHORITY_FAILED'); return mode!=='denied'; }
  }, ()=>true);
  try {
    await connections.fetch(id, {method:'POST',headers:new Headers(),body:new Uint8Array(),signal:AbortSignal.abort()});
    return Response.json({unexpected:true});
  } catch(error) {
    return Response.json({typed:error instanceof ReverseChannelUnavailable,message:error.message,reads,sent,closed});
  }
} };`,
  }, bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022', external: ['cloudflare:workers'] });
  const runtime = new Miniflare({ modules: true, script: bundled.outputFiles[0].text,
    compatibilityDate: '2026-09-08', host: '127.0.0.1', port: 0 });
  t.after(() => runtime.dispose());
  const run = async mode => {
    const response = await runtime.dispatchFetch(`https://fixture.example/${mode}`);
    assert.equal(response.status, 200);
    return response.json();
  };
  await t.test('missing channel is typed unavailable without authority reads', async () => {
    assert.deepEqual(await run('missing'), { typed: true, message: 'Connector unavailable', reads: 0, sent: 0, closed: false });
  });
  await t.test('authority rejection is untyped and never dispatches', async () => {
    assert.deepEqual(await run('reject'), { typed: false, message: 'AUTHORITY_FAILED', reads: 1, sent: 0, closed: false });
  });
  await t.test('authority denial is untyped, closes the channel and never dispatches', async () => {
    assert.deepEqual(await run('denied'), { typed: false, message: 'Connector unavailable', reads: 1, sent: 0, closed: true });
  });
  await t.test('transport rejection after authority success is typed', async () => {
    assert.deepEqual(await run('dispatch'), { typed: true, message: 'Connector unavailable', reads: 1, sent: 0, closed: false });
  });
});
