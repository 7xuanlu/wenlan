// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { forwardQuery, routeDenial, TOOL_UNAVAILABLE_TEXT, tunnelOrigin, type QueryGrant, type ConnectorRoute } from '../src/proxy.ts';

const publicOrigin = 'https://relay.example';
const now = 1_000;
const grant: QueryGrant = {
  subject: 'alice', connectorId: 'alice-device', space: 'shared',
  generation: 1, scopes: ['wenlan:query'], expiresAt: 2_000,
};
const route: ConnectorRoute = {
  id: 'alice-device', subject: 'alice', space: 'shared', generation: 1,
  enabled: true, expiresAt: 2_000, tunnelOrigin: 'https://alice.trycloudflare.com',
  backendToken: 'a'.repeat(43),
};
function request(method = 'tools/call', args: object = { query: 'decision' }, name = 'recall') {
  return new Request(`${publicOrigin}/mcp`, {
    method: 'POST', headers: { 'content-type': 'application/json', authorization: 'Bearer PRIVATE_OAUTH',
      cookie: 'PRIVATE_COOKIE', 'x-wenlan-space': 'personal', 'x-origin-space': 'personal',
      'mcp-session-id': 'session-1', 'mcp-protocol-version': '2025-03-26' },
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method, params: { name, arguments: args } }),
  });
}
function setup(selected: ConnectorRoute | null = route) {
  const calls: Array<{ url: string; init: RequestInit }> = [];
  let loads = 0;
  return {
    calls, get loads() { return loads; },
    options: {
      publicOrigin, now: () => now,
      loadRoute: async (id: string) => { loads++; assert.equal(id, 'alice-device'); return selected; },
      fetch: (async (url, init) => {
        calls.push({ url: String(url), init: init! });
        return new Response('{"jsonrpc":"2.0","id":1,"result":{}}', { headers: {
          'content-type': 'application/json', 'mcp-session-id': 'session-1', 'set-cookie': 'PRIVATE_BACKEND',
          'x-private-debug': 'PRIVATE_DEBUG',
        } });
      }) as typeof fetch,
    },
  };
}

test('strict tunnel origins reject substring spoofing and URL components', () => {
  assert.equal(tunnelOrigin('https://alice.trycloudflare.com/'), 'https://alice.trycloudflare.com');
  for (const value of ['https://evil.example/?x=.trycloudflare.com', 'https://alice.trycloudflare.com.evil.example',
    'https://trycloudflare.com', 'https://a.b.trycloudflare.com', 'http://alice.trycloudflare.com',
    'https://user:pass@alice.trycloudflare.com', 'https://alice.trycloudflare.com:444',
    'https://alice.trycloudflare.com/private', 'https://alice.trycloudflare.com/#secret', 'not a url']) {
    assert.equal(tunnelOrigin(value), null, value);
  }
});

test('missing, expired or insufficient grants cannot even look up a route', async () => {
  for (const bad of [null, { ...grant, expiresAt: now }, { ...grant, scopes: [] }, { ...grant, space: '' },
    { ...grant, space: undefined }, { ...grant, scopes: undefined }] as Array<QueryGrant | null>) {
    const state = setup();
    assert.equal((await forwardQuery(request(), bad, state.options)).status, 401);
    assert.equal(state.loads, 0);
    assert.equal(state.calls.length, 0);
  }
});

test('cancelling a request interrupts a stalled body before any forwarding', async () => {
  const controller = new AbortController();
  let cancelled = false;
  const body = new ReadableStream({ cancel() { cancelled = true; } });
  const req = new Request(`${publicOrigin}/mcp`, { method: 'POST',
    headers: { 'content-type': 'application/json' }, body, signal: controller.signal,
    duplex: 'half',
  } as RequestInit);
  const state = setup();
  const result = forwardQuery(req, grant, state.options);
  setTimeout(() => controller.abort(), 5);
  assert.equal((await result).status, 413);
  assert.equal(cancelled, true);
  assert.equal(state.calls.length, 0);
});

const CHALLENGE = `Bearer resource_metadata="${publicOrigin}/.well-known/oauth-protected-resource/mcp", error="invalid_token", scope="wenlan:query"`;

test('cross-user, scope, device, generation and revoked routes fail closed with an OAuth challenge', async () => {
  for (const bad of [null, { ...route, subject: 'bob' }, { ...route, space: 'personal' },
    { ...route, id: 'bob-device' }, { ...route, generation: 2 }, { ...route, enabled: false }]) {
    const state = setup(bad);
    const response = await forwardQuery(request(), grant, state.options);
    assert.equal(response.status, 401);
    // A dead grant sends the client back through OAuth, which lands in pairing.
    assert.equal(response.headers.get('www-authenticate'), CHALLENGE);
    assert.equal(state.calls.length, 0);
  }
});

test('an expired route lease with a current grant is the friendly offline 503, not a reauthorization', async () => {
  const state = setup({ ...route, expiresAt: now });
  const offline = await forwardQuery(request(), grant, { ...state.options, grantActive: async () => true });
  assert.equal(offline.status, 503);
  assert.equal(offline.headers.get('www-authenticate'), null);
  assert.deepEqual(await offline.json(), { error: TOOL_UNAVAILABLE_TEXT });
  // Without a grant check (no OAuth context) the lease alone decides: offline.
  assert.equal((await forwardQuery(request(), grant, setup({ ...route, expiresAt: now }).options)).status, 503);
  // Lapsed lease AND a dead grant (consent gone, device expired): reauthorize.
  const dead = await forwardQuery(request(), grant, { ...state.options, grantActive: async () => false });
  assert.equal(dead.status, 401);
  assert.equal(dead.headers.get('www-authenticate'), CHALLENGE);
  // A storage failure while checking stays a sanitized 503, never a 401.
  const failing = await forwardQuery(request(), grant, { ...state.options,
    grantActive: async () => { throw new Error('PRIVATE_DB'); } });
  assert.equal(failing.status, 503);
  assert.equal(await failing.text(), '{"error":"Connector unavailable"}');
  assert.equal(state.calls.length, 0);
});

test('a refused authorize is 401 only when the grant itself is dead; otherwise 403', async () => {
  const state = setup();
  const dead = await forwardQuery(request(), grant, { ...state.options,
    authorize: async () => false, grantActive: async () => false });
  assert.equal(dead.status, 401);
  assert.equal(dead.headers.get('www-authenticate'), CHALLENGE);
  const scoped = await forwardQuery(request(), grant, { ...state.options,
    authorize: async () => false, grantActive: async () => true });
  assert.equal(scoped.status, 403);
  assert.equal(await scoped.text(), '{"error":"Connection is not authorized"}');
  assert.equal((await forwardQuery(request(), grant, { ...state.options, authorize: async () => false })).status, 403);
  assert.equal(state.calls.length, 0);
});

test('routeDenial separates an offline lease from a changed binding', () => {
  assert.equal(routeDenial(route, grant, now), null);
  assert.equal(routeDenial({ ...route, expiresAt: now }, grant, now), 'offline');
  assert.equal(routeDenial({ ...route, expiresAt: Number.NaN }, grant, now), 'offline');
  for (const bad of [null, undefined, { ...route, enabled: false }, { ...route, generation: 2 },
    { ...route, space: 'personal' }, { ...route, subject: 'bob' }, { ...route, id: 'bob-device' },
    { ...route, generation: 2, expiresAt: now }]) {
    assert.equal(routeDenial(bad, grant, now), 'revoked');
  }
});

test('allowed query uses only bound tunnel, replaces OAuth bearer and strips private headers', async () => {
  const state = setup();
  const response = await forwardQuery(request(), grant, state.options);
  assert.equal(response.status, 200);
  assert.equal(state.calls[0].url, 'https://alice.trycloudflare.com/mcp');
  const forwarded = new Headers(state.calls[0].init.headers);
  assert.equal(forwarded.get('authorization'), `Bearer ${route.backendToken}`);
  assert.equal(forwarded.get('mcp-session-id'), 'session-1');
  for (const name of ['cookie', 'x-wenlan-space', 'x-origin-space']) assert.equal(forwarded.get(name), null);
  assert.equal(state.calls[0].init.redirect, 'manual');
  assert.equal(response.headers.get('set-cookie'), null);
  assert.equal(response.headers.get('x-private-debug'), null);
  assert.equal(response.headers.get('cache-control'), 'no-store');
});

test('only query tools and authorized Space are accepted', async () => {
  for (const req of [request('tools/call', {}, 'capture'), request('tools/call', {}, 'forget'),
    request('resources/read'), request('tools/call', { space: 'personal' }),
    request('tools/call', { domain: 'personal' }), request('tools/call', { space: null })]) {
    const state = setup();
    assert.equal((await forwardQuery(req, grant, state.options)).status, 403);
    assert.equal(state.calls.length, 0);
  }
  for (const name of ['brief', 'recall', 'get_page_sources']) {
    assert.equal((await forwardQuery(request('tools/call', { space: 'shared' }, name), grant, setup().options)).status, 200);
  }
});

test('arbitrary proxy paths and query-string route selection are rejected', async () => {
  for (const suffix of ['/alice-device/mcp', '/health', '/mcp?user_id=bob', '/mcp/anything']) {
    const state = setup();
    assert.equal((await forwardQuery(new Request(publicOrigin + suffix), grant, state.options)).status, 404);
    assert.equal(state.loads, 0);
  }
});

test('oversized bodies and JSON-RPC batches are rejected before forwarding', async () => {
  const tooLarge = new Request(`${publicOrigin}/mcp`, { method: 'POST',
    headers: { 'content-type': 'application/json' }, body: 'x'.repeat(65 * 1024) });
  const state = setup();
  assert.equal((await forwardQuery(tooLarge, grant, state.options)).status, 413);
  const batch = new Request(`${publicOrigin}/mcp`, { method: 'POST',
    headers: { 'content-type': 'application/json' }, body: '[]' });
  assert.equal((await forwardQuery(batch, grant, state.options)).status, 403);
  assert.equal(state.calls.length, 0);
});

test('redirects, storage errors and offline backends return sanitized failures', async () => {
  const state = setup();
  for (const fetcher of [async () => Response.redirect('https://evil.example'),
    async () => new Response('PRIVATE_BACKEND', { status: 401 })]) {
    const response = await forwardQuery(request(), grant, { ...state.options, fetch: fetcher as typeof fetch });
    assert.equal(response.status, 502);
    assert.equal(await response.text(), '{"error":"Connector unavailable"}');
  }
  const offline = await forwardQuery(request(), grant, { ...state.options,
    fetch: (async () => { throw new Error('PRIVATE_TOKEN secret upstream'); }) as typeof fetch });
  assert.equal(offline.status, 200);
  assert.deepEqual(await offline.json(), { jsonrpc: '2.0', id: 1, result: { content: [{ type: 'text',
    text: 'Wenlan could not reach your local device. Make sure Wenlan is running and the device is online, then try again. This does not mean your authorization was revoked.' }],
    isError: true } });
  const response = await forwardQuery(request(), grant, { ...state.options, loadRoute: async () => { throw new Error('PRIVATE_DB'); } });
  assert.equal(response.status, 503);
  assert.equal(await response.text(), '{"error":"Connector unavailable"}');
});

test('every request reloads revocation state, including session GET and DELETE', async () => {
  let enabled = true;
  const state = setup();
  const options = { ...state.options, loadRoute: async () => ({ ...route, enabled }) };
  assert.equal((await forwardQuery(request(), grant, options)).status, 200);
  enabled = false;
  for (const method of ['GET', 'DELETE']) {
    const req = new Request(`${publicOrigin}/mcp`, { method, headers: { 'mcp-session-id': 'session-1' } });
    assert.equal((await forwardQuery(req, grant, options)).status, 401);
  }
  assert.equal(state.calls.length, 1);
});

test('expiry and revocation during body upload are rechecked before dispatch', async () => {
  for (const change of ['expiry', 'revocation']) {
    let clock = now;
    let enabled = true;
    const state = setup();
    const body = new ReadableStream({ start(controller) {
      setTimeout(() => {
        if (change === 'expiry') clock = grant.expiresAt;
        else enabled = false;
        controller.enqueue(new TextEncoder().encode('{"jsonrpc":"2.0","id":1,"method":"ping"}'));
        controller.close();
      }, 5);
    } });
    const req = new Request(`${publicOrigin}/mcp`, { method: 'POST', body, duplex: 'half',
      headers: { 'content-type': 'application/json' } } as RequestInit);
    const response = await forwardQuery(req, grant, { ...state.options,
      now: () => clock, loadRoute: async () => ({ ...route, enabled }) });
    assert.equal(response.status, 401);
    if (change === 'revocation') assert.equal(response.headers.get('www-authenticate'), CHALLENGE);
    assert.equal(state.calls.length, 0);
  }
});

test('authorization-store latency cannot dispatch a credential that expired while waiting', async () => {
  const state = setup();
  let clock = now;
  const response = await forwardQuery(request(), grant, { ...state.options, now: () => clock,
    authorize: async () => { clock = grant.expiresAt; return true; } });
  assert.equal(response.status, 401);
  assert.equal(state.calls.length, 0);
});
