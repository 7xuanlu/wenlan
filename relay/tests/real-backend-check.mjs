// SPDX-License-Identifier: Apache-2.0
// Invoked by the Rust reviewer fixture while its real isolated MCP is alive.
import assert from 'node:assert/strict';
import test from 'node:test';
import { createSampleRuntime, beginSampleAuthorization, sampleOrigin as origin } from './fixtures/sample-runtime.mjs';

const local = new URL(process.env.WENLAN_TEST_MCP_URL ?? 'invalid:');
assert.equal(local.protocol, 'http:');
assert.equal(local.hostname, '127.0.0.1');
assert(local.port && !local.username && !local.password && local.pathname === '/' && !local.search && !local.hash);

async function result(response, id) {
  assert.equal(response.status, 200, await response.clone().text());
  const text = await response.text();
  const messages = response.headers.get('content-type')?.startsWith('text/event-stream')
    // The pinned rmcp transport can emit empty priming data before its JSON.
    ? text.split('\n').filter(line => line.startsWith('data:')).map(line => line.slice(5).trim())
      .filter(Boolean).map(data => JSON.parse(data))
    : [JSON.parse(text)];
  const message = messages.find(message => message.id === id);
  assert(message, 'matching JSON-RPC response');
  assert.equal(message.error, undefined);
  return message.result;
}

function projected(value) {
  assert.equal(value.isError, false);
  assert.deepEqual(JSON.parse(value.content[0].text), value.structuredContent);
  for (const secret of ['UNAUTHORIZED_SENTINEL', 'mem_private-sentinel', 'mem_atlas-unavailable', 'synthetic: true']) {
    assert(!JSON.stringify(value).includes(secret), `unexpected output: ${secret}`);
  }
  return value.structuredContent;
}

test('OAuth sample login reaches the real scoped Wenlan database through relay and HTTP MCP', { timeout: 45_000 }, async () => {
  let offline = false;
  let privateSession;
  const fixture = await createSampleRuntime(undefined, async (request, candidate) => {
    const upstream = new URL(request.url);
    assert.equal(upstream.origin, candidate.tunnelOrigin);
    assert(['/mcp', '/connector-info'].includes(upstream.pathname));
    if (upstream.pathname === '/mcp') {
      assert.equal(request.headers.get('authorization'), `Bearer ${candidate.backendToken}`);
      assert.equal(request.headers.get('cookie'), null);
      assert.equal(request.headers.get('x-wenlan-space'), null);
    }
    if (offline) throw new Error('Synthetic transport outage');
    // Substitute transport only: preserve real auth, request body, status and SSE.
    const headers = new Headers(request.headers);
    headers.delete('host');
    const response = await fetch(new URL(upstream.pathname, local), {
      method: request.method, headers, body: request.body, duplex: 'half',
      redirect: 'manual', signal: AbortSignal.any([request.signal, AbortSignal.timeout(10_000)]),
    });
    if (response.headers.has('mcp-session-id')) privateSession = response.headers.get('mcp-session-id');
    return response;
  });
  const f = fixture;
  try {
    const authorization = await beginSampleAuthorization(f);
    const login = await f.post('/pairing/sample', {
      username: f.issued.account.username, password: f.issued.password, approved: true,
      clientId: authorization.client.client_id, resource: `${origin}/mcp`, space: 'atlas-review',
    }, { cookie: authorization.cookie, origin, 'sec-fetch-site': 'same-origin' });
    assert.equal(login.status, 200, await login.clone().text());
    const callback = new URL((await login.json()).redirectTo);
    assert.equal(callback.origin, 'https://client.example');
    assert.equal(callback.searchParams.get('iss'), origin);
    const tokenRequest = fields => f.request('/oauth/token', {
      method: 'POST', headers: { 'content-type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({ ...fields, client_id: authorization.client.client_id, resource: `${origin}/mcp` }).toString(),
    });
    const exchanged = await tokenRequest({ grant_type: 'authorization_code', code: callback.searchParams.get('code'),
      code_verifier: authorization.verifier, redirect_uri: authorization.redirectUri });
    assert.equal(exchanged.status, 200);
    let tokens = await exchanged.json();
    let session;
    let id = 0;
    const headers = () => ({ authorization: `Bearer ${tokens.access_token}`, accept: 'application/json, text/event-stream',
      'mcp-protocol-version': '2025-06-18', cookie: 'synthetic-client-cookie=must-not-forward',
      'x-wenlan-space': 'private-sentinel', ...(session ? { 'mcp-session-id': session } : {}) });
    const call = async (name, args) => {
      const callId = ++id;
      return f.post('/mcp', { jsonrpc: '2.0', id: callId, method: 'tools/call',
        params: { name, arguments: args } }, headers());
    };
    const initialize = async () => {
      const response = await f.post('/mcp', { jsonrpc: '2.0', id: ++id, method: 'initialize', params: {
        protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'reviewer-relay-check', version: '1' },
      } }, headers());
      session = response.headers.get('mcp-session-id');
      const info = await result(response, id);
      assert(info.serverInfo.name);
      assert(session && privateSession && session !== privateSession, 'public session is remapped');
      const notified = await f.post('/mcp', { jsonrpc: '2.0', method: 'notifications/initialized' }, headers());
      assert.equal(notified.status, 202);
      await notified.arrayBuffer();
    };
    await initialize();
    const listed = await result(await f.post('/mcp', { jsonrpc: '2.0', id: ++id, method: 'tools/list' }, headers()), id);
    assert.deepEqual(listed.tools.map(tool => tool.name).sort(), ['brief', 'get_page_sources', 'recall']);
    const brief = projected(await result(await call('brief', {}), id));
    assert.equal(brief.space, 'atlas-review');
    assert.equal(brief.brief.active[0].text, 'Use signed requests');
    assert.equal(brief.brief.backlog[0].text, 'Document offline fallback');
    const topic = projected(await result(await call('brief', { topic: 'Atlas authentication decision' }), id));
    assert.deepEqual(topic.brief, brief.brief);
    assert(topic.related_context.results.some(hit => hit.source_id === 'mem_atlas-auth'));
    const recalled = projected(await result(await call('recall', { query: 'Atlas authentication decision', limit: 3, rerank: false }), id));
    assert(recalled.results.length <= 3 && recalled.results.some(hit => hit.source_id === 'mem_atlas-auth'));
    const sources = projected(await result(await call('get_page_sources', { page_id: 'page_atlas-auth' }), id));
    assert.equal(sources.sources.length, 1);
    assert(JSON.stringify(sources).includes('mem_atlas-auth'));
    assert.deepEqual(projected(await result(await call('get_page_sources', { page_id: 'page_atlas-unavailable' }), id)),
      { page_id: 'page_atlas-unavailable', sources: [] });
    const deniedPage = await result(await call('get_page_sources', { page_id: 'page_private-sentinel' }), id);
    assert.equal(deniedPage.isError, true);
    assert(!JSON.stringify(deniedPage).includes('UNAUTHORIZED_SENTINEL'));
    const beforeDenials = f.backendCalls();
    assert.equal((await call('brief', { space: 'private-sentinel' })).status, 403);
    assert.equal((await call('capture', { content: 'must not be stored' })).status, 403);
    assert.equal(f.backendCalls(), beforeDenials, 'denials never reach the daemon');

    offline = true;
    const failed = await call('brief', {});
    assert.equal(failed.status, 502);
    assert.deepEqual(await failed.json(), { error: 'Connector unavailable' });
    offline = false;
    assert.deepEqual(projected(await result(await call('brief', {}), id)), brief);

    const refreshed = await tokenRequest({ grant_type: 'refresh_token', refresh_token: tokens.refresh_token });
    assert.equal(refreshed.status, 200);
    const previousRefresh = tokens.refresh_token;
    tokens = await refreshed.json();
    assert(tokens.refresh_token && tokens.refresh_token !== previousRefresh);
    assert.deepEqual(projected(await result(await call('brief', {}), id)), brief);

    const deleted = await f.request('/mcp', { method: 'DELETE', headers: headers() });
    assert.equal(deleted.status, 202);
    await deleted.arrayBuffer();
    assert.equal((await call('brief', {})).status, 404);
    session = undefined;
    await initialize();
    const beforeRevoke = f.backendCalls();
    assert.equal((await f.post('/devices/revoke', {}, { authorization: `Bearer ${f.device.managementToken}`,
      'x-wenlan-device-id': f.device.id })).status, 200);
    assert.equal((await call('brief', {})).status, 403);
    assert.equal(f.backendCalls(), beforeRevoke, 'revoked grant never reaches the daemon');
  } finally { await f.runtime.dispose(); }
});
