// SPDX-License-Identifier: Apache-2.0
// Attended local-only browser verification, with synthetic credentials/data.
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createSampleRuntime, beginSampleAuthorization, sampleOrigin } from './fixtures/sample-runtime.mjs';

const cancelAfterMs = Number(process.env.WENLAN_PREVIEW_CANCEL_PAIRING_MS ?? 0);
assert(Number.isInteger(cancelAfterMs) && cancelAfterMs >= 0 && cancelAfterMs <= 30_000);
const fixture = await createSampleRuntime();
let authorization;
let localOrigin;
let pendingCancellation;
const server = createServer(async (request, response) => {
  try {
    const url = new URL(request.url, localOrigin);
    if (url.pathname === '/start') {
      authorization = await beginSampleAuthorization(fixture, `${localOrigin}/callback`);
      if (cancelAfterMs) {
        clearTimeout(pendingCancellation);
        const cookie = authorization.cookie;
        // Mutate only this local fixture to exercise a stale page's terminal state.
        pendingCancellation = setTimeout(async () => {
          try {
            const cancelled = await fixture.post('/pairing/cancel', {}, { cookie, origin: sampleOrigin });
            assert.equal(cancelled.status, 200);
            console.log('LOCAL_PAIRING_CANCELLED');
          } catch {
            console.error('LOCAL_PAIRING_CANCELLATION_FAILED');
          }
        }, cancelAfterMs);
      }
      // Preserve the __Host cookie's required Secure attribute on loopback.
      // This bridge is still not evidence of production TLS behavior.
      response.writeHead(302, { location: cancelAfterMs ? '/pairing' : '/pairing/sample', 'cache-control': 'no-store',
        'set-cookie': `${authorization.cookie}; Path=/; Secure; HttpOnly; SameSite=Lax` }).end();
      return;
    }
    if (url.pathname === '/callback') {
      assert.ok(authorization);
      assert.equal(url.searchParams.get('iss'), sampleOrigin);
      const result = await fixture.request('/oauth/token', { method: 'POST',
        headers: { 'content-type': 'application/x-www-form-urlencoded' }, body: new URLSearchParams({
          grant_type: 'authorization_code', code: url.searchParams.get('code'), code_verifier: authorization.verifier,
          client_id: authorization.client.client_id, redirect_uri: authorization.redirectUri,
          resource: `${sampleOrigin}/mcp`,
        }).toString() });
      assert.equal(result.status, 200);
      const tokens = await result.json();
      const initialized = await fixture.post('/mcp', { jsonrpc: '2.0', id: 1, method: 'initialize' },
        { authorization: `Bearer ${tokens.access_token}` });
      assert.equal(initialized.status, 200);
      await initialized.arrayBuffer();
      response.writeHead(200, { 'content-type': 'text/plain; charset=utf-8', 'cache-control': 'no-store' })
        .end('Local synthetic verification: OAuth exchange and authorized MCP initialization succeeded.');
      return;
    }
    if (url.pathname.startsWith('/__fixture')) { response.writeHead(404).end(); return; }
    const chunks = [];
    let size = 0;
    for await (const chunk of request) {
      size += chunk.length;
      if (size > 16 * 1024) { response.writeHead(413).end(); return; }
      chunks.push(chunk);
    }
    const headers = new Headers();
    for (const [key, value] of Object.entries(request.headers)) {
      if (['host', 'connection', 'content-length', 'transfer-encoding'].includes(key) || value === undefined) continue;
      for (const part of Array.isArray(value) ? value : [value]) headers.append(key,
        key === 'origin' && part === localOrigin ? sampleOrigin : part);
    }
    const result = await fixture.request(`${url.pathname}${url.search}`, { method: request.method, headers,
      body: size ? Buffer.concat(chunks) : undefined });
    response.writeHead(result.status, Object.fromEntries(result.headers));
    response.end(Buffer.from(await result.arrayBuffer()));
  } catch {
    response.writeHead(500, { 'content-type': 'text/plain' }).end('Local fixture verification failed.');
  }
});
server.requestTimeout = 10_000;
server.headersTimeout = 10_000;
await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
localOrigin = `http://127.0.0.1:${server.address().port}`;
console.log(JSON.stringify({ url: `${localOrigin}/start`, syntheticUsername: fixture.issued.account.username,
  syntheticPassword: fixture.issued.password, lifetimeSeconds: 300 }));
let closing = false;
async function close() {
  if (closing) return;
  closing = true;
  clearTimeout(timeout);
  clearTimeout(pendingCancellation);
  server.closeAllConnections();
  await new Promise(resolve => server.close(resolve));
  await fixture.runtime.dispose();
}
const timeout = setTimeout(close, 300_000);
process.once('SIGINT', close);
process.once('SIGTERM', close);
