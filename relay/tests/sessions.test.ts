// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { forwardSessionQuery, type SessionIdentity } from '../src/sessions.ts';
import type { ConnectorRoute, QueryGrant } from '../src/proxy.ts';
import { MemoryStore } from './fixtures/memory-store.ts';

const origin = 'https://relay.example';
const grant: QueryGrant = { subject: 'device-owner', connectorId: 'device', space: 'review',
  generation: 1, scopes: ['wenlan:query'], expiresAt: Date.now() + 60_000 };
const identity: SessionIdentity = { grantId: 'grant-a', clientId: 'client-a' };
const route: ConnectorRoute = { id: 'device', subject: grant.subject, space: 'review', generation: 1,
  enabled: true, expiresAt: Date.now() + 120_000, tunnelOrigin: 'https://demo.trycloudflare.com', backendToken: 'b'.repeat(43) };
function rpc(method = 'initialize', session?: string, verb = 'POST') {
  const headers = new Headers({ 'content-type': 'application/json', accept: 'application/json, text/event-stream' });
  if (session !== undefined) headers.set('mcp-session-id', session);
  return new Request(`${origin}/mcp`, { method: verb, headers,
    body: verb === 'POST' ? JSON.stringify({ jsonrpc: '2.0', id: 1, method }) : undefined });
}
function setup() {
  const store = new MemoryStore();
  let backendId = 'private-backend-session';
  let status = 200;
  let currentRoute = { ...route };
  const calls: RequestInit[] = [];
  const options = { publicOrigin: origin, loadRoute: async () => currentRoute,
    fetch: (async (_url, init) => {
      calls.push(init!);
      return new Response(init?.method === 'DELETE' ? null : '{"jsonrpc":"2.0","id":1,"result":{}}', {
        status, headers: { 'content-type': 'application/json', 'mcp-session-id': backendId },
      });
    }) as typeof fetch,
  };
  const call = (request: Request, who = identity, permission = grant) => forwardSessionQuery(request, permission, who, store, options);
  const initialize = async (who = identity) => {
    const response = await call(rpc(), who);
    assert.equal(response.status, 200, await response.clone().text());
    await response.text();
    const id = response.headers.get('mcp-session-id')!;
    assert.match(id, /^[a-f0-9]{64}$/);
    assert.notEqual(id, backendId);
    return id;
  };
  return { store, options, calls, call, initialize,
    setBackend: (id: string) => { backendId = id; },
    setStatus: (code: number) => { status = code; },
    setRoute: (value: ConnectorRoute) => { currentRoute = value; },
  };
}

test('public session ID translates to a private backend ID for its OAuth grant', async () => {
  const state = setup();
  const id = await state.initialize();
  const response = await state.call(rpc('tools/list', id));
  assert.equal(response.status, 200);
  await response.text();
  assert.equal(new Headers(state.calls[1].headers).get('mcp-session-id'), 'private-backend-session');
  assert.equal(response.headers.get('mcp-session-id'), null);
});

test('another grant or client cannot use GET, POST or DELETE on an existing session', async () => {
  const state = setup();
  const id = await state.initialize();
  for (const who of [{ ...identity, grantId: 'grant-b' }, { ...identity, clientId: 'client-b' }]) {
    for (const method of ['POST', 'GET', 'DELETE']) {
      const response = await state.call(rpc('tools/list', id, method), who);
      assert.equal(response.status, 404);
    }
  }
  assert.equal(state.calls.length, 1);
});

test('session binding survives access token refresh within the same grant', async () => {
  const state = setup();
  const id = await state.initialize();
  const response = await state.call(rpc('tools/list', id), identity, { ...grant, expiresAt: grant.expiresAt + 60_000 });
  assert.equal(response.status, 200);
  await response.text();
});

test('raw backend IDs, unknown IDs and missing sessions never reach the backend', async () => {
  const state = setup();
  await state.initialize();
  for (const id of ['private-backend-session', 'a'.repeat(64), 'x'.repeat(300)]) {
    assert.equal((await state.call(rpc('tools/list', id))).status, 404);
  }
  assert.equal((await state.call(rpc('tools/list'))).status, 400);
  assert.equal(state.calls.length, 1);
});

test('DELETE terminates the public session and cannot be replayed', async () => {
  const state = setup();
  const id = await state.initialize();
  assert.equal((await state.call(rpc('', id, 'DELETE'))).status, 200);
  for (const method of ['GET', 'POST', 'DELETE']) assert.equal((await state.call(rpc('tools/list', id, method))).status, 404);
  assert.equal(state.calls.length, 2);
});

test('backend session reuse cannot let another OAuth grant join the same local session', async () => {
  const state = setup();
  await state.initialize();
  assert.equal((await state.call(rpc(), { ...identity, grantId: 'grant-b' })).status, 502);
  state.setBackend('new-private-session');
  await state.initialize({ ...identity, grantId: 'grant-b' });
});

test('concurrent initializations cannot claim the same backend session twice', async () => {
  const state = setup();
  const results = await Promise.all([state.call(rpc()), state.call(rpc(), { ...identity, grantId: 'grant-b' })]);
  assert.deepEqual(results.map(response => response.status).sort(), [200, 502]);
  for (const response of results) await response.text();
});

test('tunnel renewal and backend credential changes require a fresh MCP session', async () => {
  const state = setup();
  const id = await state.initialize();
  for (const next of [{ ...route, tunnelOrigin: 'https://new.trycloudflare.com' },
    { ...route, backendToken: 'c'.repeat(43) }]) {
    state.setRoute(next);
    assert.equal((await state.call(rpc('tools/list', id))).status, 404);
  }
  assert.equal(state.calls.length, 1);
});

test('backend protocol errors preserve sanitized 400, 404 and 405 statuses', async () => {
  for (const status of [400, 404, 405]) {
    const state = setup();
    const id = await state.initialize();
    state.setStatus(status);
    const response = await state.call(rpc('tools/list', id));
    assert.equal(response.status, status);
    assert.equal(response.headers.get('mcp-session-id'), null);
    assert(!await response.text().then(text => text.includes('private-backend-session')));
  }
});

test('a backend 404 invalidates the mapping and later calls do not reuse it', async () => {
  const state = setup();
  const id = await state.initialize();
  state.setStatus(404);
  assert.equal((await state.call(rpc('tools/list', id))).status, 404);
  state.setStatus(200);
  assert.equal((await state.call(rpc('tools/list', id))).status, 404);
  assert.equal(state.calls.length, 2);
  await state.initialize();
});

test('session expiry rejects requests even with a valid refreshed OAuth token', async () => {
  const state = setup();
  const id = await state.initialize();
  const now = Date.now() + 25 * 60 * 60 * 1000;
  const response = await forwardSessionQuery(rpc('tools/list', id), { ...grant, expiresAt: now + 60_000 }, identity,
    state.store, { ...state.options, now: () => now, loadRoute: async () => ({ ...route, expiresAt: now + 120_000 }) });
  assert.equal(response.status, 404);
  assert.equal(state.calls.length, 1);
});

test('an idle SSE stream stops when its session is deleted', async () => {
  const state = setup();
  const id = await state.initialize();
  let cancelled = false;
  const response = await forwardSessionQuery(rpc('', id, 'GET'), grant, identity, state.store, {
    ...state.options, fetch: (async () => new Response(new ReadableStream({ cancel() { cancelled = true; } }),
      { headers: { 'content-type': 'text/event-stream' } })) as typeof fetch,
  });
  assert.equal(response.status, 200);
  const reader = response.body!.getReader();
  assert.equal((await state.call(rpc('', id, 'DELETE'))).status, 200);
  let timeout: ReturnType<typeof setTimeout> | undefined;
  try {
    await assert.rejects(Promise.race([reader.read(), new Promise((_, reject) => {
      timeout = setTimeout(() => reject(new Error('TEST_TIMEOUT')), 4000);
    })]), /Connector stream unavailable/);
    assert(cancelled);
  } finally { clearTimeout(timeout); await reader.cancel().catch(() => {}); }
});
