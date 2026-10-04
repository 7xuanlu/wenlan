// SPDX-License-Identifier: Apache-2.0
// Characterization tests: offline diagnostics at the session boundary.
// Uses only existing public APIs (forwardSessionQuery, forwardQuery) and the
// existing MemoryStore double. All fixtures synthetic. No source changes.
//
// Intended contract (Chrome client cannot be patched): when the local device
// is unreachable but auth is still valid, POST initialize/tools/list/ping
// return HTTP 200 with an explicit JSON-RPC error (-32000); tools/call
// returns the recoverable isError tool result. Everything else offline
// (GET/DELETE, notifications, invalid ids) remains a 502 transport failure,
// and auth changes still fail closed.
import assert from 'node:assert/strict';
import test from 'node:test';
import { forwardQuery } from '../src/proxy.ts';
import { forwardSessionQuery, type SessionIdentity } from '../src/sessions.ts';
import type { ConnectorRoute, QueryGrant } from '../src/proxy.ts';
import { MemoryStore } from './fixtures/memory-store.ts';

const SAFE_TEXT = 'Wenlan could not reach your local device. Make sure Wenlan is running and the device is online, then try again. This does not mean your authorization was revoked.';
const publicOrigin = 'https://relay.example';
const identity: SessionIdentity = { grantId: 'grant-a', clientId: 'client-a' };
const liveGrant: QueryGrant = { subject: 'alice', connectorId: 'alice-device', space: 'shared',
  generation: 1, scopes: ['wenlan:query'], expiresAt: Date.now() + 60_000 };
const liveRoute: ConnectorRoute = { id: 'alice-device', subject: 'alice', space: 'shared', generation: 1,
  enabled: true, expiresAt: Date.now() + 120_000,
  tunnelOrigin: 'https://alice.trycloudflare.com', backendToken: 'a'.repeat(43) };

const offlineFetch = (async () => { throw new Error('connect ECONNREFUSED 10.0.0.7 PrivateHost'); }) as typeof fetch;

function setup() {
  const store = new MemoryStore();
  const opts = { publicOrigin, loadRoute: async () => ({ ...liveRoute }), fetch: offlineFetch };
  return { store, opts };
}

const post = (body: unknown, session?: string) => new Request(`${publicOrigin}/mcp`, { method: 'POST',
  headers: { 'content-type': 'application/json', ...(session ? { 'mcp-session-id': session } : {}) },
  body: JSON.stringify(body) });
const initializeRpc = { jsonrpc: '2.0', id: 1, method: 'initialize' };
const toolsCallRpc = (id: unknown = 9) =>
  ({ jsonrpc: '2.0', id, method: 'tools/call', params: { name: 'brief', arguments: {} } });

async function initializeLive(): Promise<{ store: MemoryStore; opts: { publicOrigin: string; loadRoute: () => Promise<ConnectorRoute>; fetch: typeof fetch }; session: string }> {
  const store = new MemoryStore();
  const opts = { publicOrigin,
    loadRoute: async () => ({ ...liveRoute }),
    fetch: (async () => new Response('{"jsonrpc":"2.0","id":1,"result":{}}',
      { headers: { 'content-type': 'application/json', 'mcp-session-id': 'backend-s' } })) as typeof fetch,
  };
  const response = await forwardSessionQuery(post(initializeRpc), liveGrant, identity, store, opts);
  assert.equal(response.status, 200);
  await response.text();
  const session = response.headers.get('mcp-session-id')!;
  assert.match(session, /^[a-f0-9]{64}$/);
  return { store, opts, session };
}

test('offline initialize returns explicit JSON-RPC error and mints no session', async () => {
  const { store, opts } = setup();
  const response = await forwardSessionQuery(post(initializeRpc), liveGrant, identity, store, opts);
  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(),
    { jsonrpc: '2.0', id: 1, error: { code: -32000, message: SAFE_TEXT } });
  assert.match(response.headers.get('content-type') ?? '', /application\/json/);
  assert.equal(response.headers.get('mcp-session-id'), null);
  assert.equal(store.data.size, 0);
  // Same at the proxy layer: no session machinery, still the JSON-RPC error.
  const direct = await forwardQuery(post(initializeRpc), liveGrant, opts);
  assert.equal(direct.status, 200);
  assert.deepEqual(await direct.json(),
    { jsonrpc: '2.0', id: 1, error: { code: -32000, message: SAFE_TEXT } });
});

test('offline tools/call on a valid session returns the safe recoverable tool result', async () => {
  const { store, opts, session } = await initializeLive();
  const offlineOpts = { ...opts, fetch: offlineFetch };
  const response = await forwardSessionQuery(post(toolsCallRpc(9), session), liveGrant, identity, store, offlineOpts);
  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { jsonrpc: '2.0', id: 9,
    result: { content: [{ type: 'text', text: SAFE_TEXT }], isError: true } });
  assert.equal(response.headers.get('cache-control'), 'no-store');
  assert.equal(response.headers.get('www-authenticate'), null);
  assert.equal(response.headers.get('mcp-session-id'), null);
  // Session mapping survives the offline blip: a later tools/list still works.
  const listed = await forwardSessionQuery(
    post({ jsonrpc: '2.0', id: 2, method: 'tools/list' }, session), liveGrant, identity, store, opts);
  assert.equal(listed.status, 200);
  await listed.text();
});

test('offline tools/list and ping on a valid session return the JSON-RPC error without touching the session', async () => {
  const { store, opts, session } = await initializeLive();
  const offlineOpts = { ...opts, fetch: offlineFetch };
  const storedBefore = store.data.size;
  for (const body of [{ jsonrpc: '2.0', id: 2, method: 'tools/list' },
    { jsonrpc: '2.0', id: 3, method: 'ping' }]) {
    const response = await forwardSessionQuery(post(body, session), liveGrant, identity, store, offlineOpts);
    assert.equal(response.status, 200);
    assert.deepEqual(await response.json(),
      { jsonrpc: '2.0', id: (body as { id: number }).id, error: { code: -32000, message: SAFE_TEXT } });
    assert.equal(response.headers.get('mcp-session-id'), null);
  }
  // No session was minted, revoked, or rebound by the errors above.
  assert.equal(store.data.size, storedBefore);
  const listed = await forwardSessionQuery(
    post({ jsonrpc: '2.0', id: 4, method: 'tools/list' }, session), liveGrant, identity, store, opts);
  assert.equal(listed.status, 200);
  await listed.text();
});

test('offline GET, notifications and invalid ids remain 502 transport failures', async () => {
  const { store, opts, session } = await initializeLive();
  const offlineOpts = { ...opts, fetch: offlineFetch };
  const cases: Request[] = [
    new Request(`${publicOrigin}/mcp`, { method: 'GET', headers: { 'mcp-session-id': session } }),
    post({ jsonrpc: '2.0', method: 'notifications/initialized' }, session),
    post({ jsonrpc: '2.0', id: null, method: 'tools/call', params: { name: 'brief', arguments: {} } }, session),
    post({ jsonrpc: '2.0', method: 'tools/call', params: { name: 'brief', arguments: {} } }, session),
    post({ jsonrpc: '2.0', id: 1.5, method: 'tools/call', params: { name: 'brief', arguments: {} } }, session),
    post({ jsonrpc: '2.0', id: 1.5, method: 'initialize' }),
  ];
  for (const request of cases) {
    const label = `${request.method} ${request.url}`;
    const response = await forwardSessionQuery(request, liveGrant, identity, store, offlineOpts);
    assert.equal(response.status, 502, label);
    assert.equal(await response.text(), '{"error":"Connector unavailable"}');
  }
});

test('offline auth changes still fail closed instead of converting', async () => {
  const { store, opts, session } = await initializeLive();
  // Route revoked between dispatch and the fresh lease check: generic 502,
  // never the recoverable tool result nor the JSON-RPC error.
  let loads = 0;
  const revoked = await forwardSessionQuery(post(toolsCallRpc(1), session), liveGrant, identity, store,
    { ...opts, fetch: offlineFetch, loadRoute: async () => (++loads <= 2 ? { ...liveRoute } : { ...liveRoute, enabled: false }) });
  assert.equal(revoked.status, 502);
  assert.equal(await revoked.text(), '{"error":"Connector unavailable"}');

  // Pre-dispatch states never reach the backend at all.
  let calls = 0;
  const countingFetch = (async (...args: Parameters<typeof fetch>) => {
    calls++;
    return offlineFetch(...args);
  }) as typeof fetch;
  const base = { ...opts, fetch: countingFetch };
  const expiredGrant: QueryGrant = { ...liveGrant, expiresAt: Date.now() - 1 };
  const expired = await forwardSessionQuery(post(toolsCallRpc(1), session), expiredGrant, identity, store, base);
  assert.equal(expired.status, 401);
  assert.equal(await expired.text(), '{"error":"Authorization required"}');
  const revokedPre = await forwardSessionQuery(post(toolsCallRpc(1), session), liveGrant, identity, store,
    { ...base, loadRoute: async () => ({ ...liveRoute, enabled: false }) });
  assert.equal(revokedPre.status, 403);
  assert.equal(await revokedPre.text(), '{"error":"Connection is not authorized"}');
  const unknown = await forwardSessionQuery(post(toolsCallRpc(1), 'b'.repeat(64)), liveGrant, identity, store, base);
  assert.equal(unknown.status, 404);
  assert.equal(await unknown.text(), '{"error":"MCP session unavailable"}');
  assert.equal(calls, 0);
});
