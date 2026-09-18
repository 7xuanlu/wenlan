// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { forwardQuery, type ConnectorRoute, type QueryGrant } from '../src/proxy.ts';
import { forwardSessionQuery, type SessionIdentity } from '../src/sessions.ts';
import { MemoryStore } from './fixtures/memory-store.ts';

const SAFE_TEXT = 'Wenlan could not reach your local device. Make sure Wenlan is running and the device is online, then try again. This does not mean your authorization was revoked.';
const publicOrigin = 'https://relay.example';
const now = 1_000;
const grant: QueryGrant = { subject: 'alice', connectorId: 'alice-device', space: 'shared',
  generation: 1, scopes: ['wenlan:query'], expiresAt: 60_000 };
const route: ConnectorRoute = { id: 'alice-device', subject: 'alice', space: 'shared', generation: 1,
  enabled: true, expiresAt: 60_000, tunnelOrigin: 'https://alice.trycloudflare.com', backendToken: 'a'.repeat(43) };
const offlineFetch = (async () => { throw new Error('connect ECONNREFUSED 10.0.0.7 PrivateHost'); }) as typeof fetch;
function setup(overrides: Record<string, unknown> = {}) {
  return { publicOrigin, now: () => now, loadRoute: async () => ({ ...route }), fetch: offlineFetch, ...overrides };
}
const toolsCall = (id: unknown) => new Request(`${publicOrigin}/mcp`, { method: 'POST',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify({ jsonrpc: '2.0', id, method: 'tools/call', params: { name: 'brief', arguments: {} } }) });

test('offline tunnel rejection returns the safe recoverable tool result', async () => {
  for (const id of [7, 'call-abc', '']) {
    const response = await forwardQuery(toolsCall(id), grant, setup());
    assert.equal(response.status, 200);
    assert.deepEqual(await response.json(), { jsonrpc: '2.0', id,
      result: { content: [{ type: 'text', text: SAFE_TEXT }], isError: true } });
    assert.equal(response.headers.get('cache-control'), 'no-store');
    assert.equal(response.headers.get('www-authenticate'), null);
  }
  const raw = await (await forwardQuery(toolsCall(1), grant, setup())).text();
  for (const leaked of ['ECONNREFUSED', '10.0.0.7', 'PrivateHost', 'alice-device']) assert(!raw.includes(leaked), leaked);
});

test('missing reverse channel returns the tool result without tunnel fallback', async () => {
  let tunnelCalls = 0;
  const missing = { ...route, tunnelOrigin: undefined, reverseConnectionId: 'x'.repeat(43) };
  const response = await forwardQuery(toolsCall(3), grant, setup({ loadRoute: async () => missing,
    fetch: (async () => { tunnelCalls++; throw new Error('Tunnel fallback must never be used'); }) as typeof fetch,
    fetchReverse: (async () => { throw new Error('missing reverse channel'); }) as never }));
  assert.equal(response.status, 200);
  assert.deepEqual(await response.json(), { jsonrpc: '2.0', id: 3,
    result: { content: [{ type: 'text', text: SAFE_TEXT }], isError: true } });
  assert.equal(tunnelCalls, 0);
});

test('excluded shapes keep existing transport failure behavior', async () => {
  const rpc = (body: unknown) => new Request(`${publicOrigin}/mcp`, { method: 'POST',
    headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) });
  const cases: Request[] = [
    rpc({ jsonrpc: '2.0', id: 1, method: 'initialize' }),
    rpc({ jsonrpc: '2.0', id: 2, method: 'tools/list' }),
    rpc({ jsonrpc: '2.0', method: 'notifications/initialized' }),
    rpc({ jsonrpc: '2.0', id: null, method: 'tools/call', params: { name: 'brief', arguments: {} } }),
    rpc({ jsonrpc: '2.0', method: 'tools/call', params: { name: 'brief', arguments: {} } }),
    rpc({ jsonrpc: '2.0', id: 1.5, method: 'tools/call', params: { name: 'brief', arguments: {} } }),
    new Request(`${publicOrigin}/mcp`, { method: 'GET' }),
    new Request(`${publicOrigin}/mcp`, { method: 'DELETE' }),
  ];
  for (const request of cases) {
    const response = await forwardQuery(request, grant, setup());
    assert.equal(response.status, 502, request.method);
    assert.equal(await response.text(), '{"error":"Connector unavailable"}');
  }
});

test('revoked, expired, aborted and failing-store checks never convert', async () => {
  let loads = 0;
  const revoked = await forwardQuery(toolsCall(1), grant, setup({ loadRoute: async () =>
    (++loads === 1 ? { ...route } : { ...route, enabled: false }) }));
  assert.equal(revoked.status, 502);
  assert(!((await revoked.json()) as { result?: unknown }).result);

  let ticks = 0;
  const expired = await forwardQuery(toolsCall(1), grant, setup({ now: () =>
    (++ticks <= 3 ? now : grant.expiresAt) }));
  assert.equal(expired.status, 502);

  const controller = new AbortController();
  const aborted = await forwardQuery(new Request(`${publicOrigin}/mcp`, { method: 'POST',
    headers: { 'content-type': 'application/json' }, signal: controller.signal,
    body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'brief', arguments: {} } }) }),
    grant, setup({ fetch: (async () => { controller.abort(); throw new Error('upstream aborted'); }) as typeof fetch }));
  assert.equal(aborted.status, 502);

  let reads = 0;
  const storeFailed = await forwardQuery(toolsCall(1), grant, setup({ loadRoute: async () => {
    if (++reads > 1) throw new Error('PRIVATE_DB'); return { ...route };
  } }));
  assert.equal(storeFailed.status, 502);
  assert.equal(await storeFailed.text(), '{"error":"Connector unavailable"}');
});

test('successful tool output is untouched', async () => {
  const payload = '{"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":"hello"}]}}';
  const response = await forwardQuery(toolsCall(1), grant, setup({ fetch: (async () =>
    new Response(payload, { headers: { 'content-type': 'application/json' } })) as typeof fetch }));
  assert.equal(response.status, 200);
  assert.equal(await response.text(), payload);
});

test('unsafe integers never emit a rounded RPC id', async () => {
  for (const id of [Number.MAX_SAFE_INTEGER + 1, -(Number.MAX_SAFE_INTEGER + 1)]) {
    const response = await forwardQuery(toolsCall(id), grant, setup());
    assert.equal(response.status, 502);
    assert.equal(await response.text(), '{"error":"Connector unavailable"}');
  }
  const raw = new Request(`${publicOrigin}/mcp`, { method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: '{"jsonrpc":"2.0","id":9007199254740993,"method":"tools/call","params":{"name":"brief","arguments":{}}}' });
  const response = await forwardQuery(raw, grant, setup());
  assert.equal(response.status, 502);
  assert.equal(await response.text(), '{"error":"Connector unavailable"}');
});

test('session wrapper passes the tool error without a new or changed session', async () => {
  const store = new MemoryStore();
  const identity: SessionIdentity = { grantId: 'grant-a', clientId: 'client-a' };
  const liveGrant: QueryGrant = { ...grant, expiresAt: Date.now() + 60_000 };
  const liveRoute: ConnectorRoute = { ...route, expiresAt: Date.now() + 120_000 };
  const options = { publicOrigin, loadRoute: async () => ({ ...liveRoute }),
    fetch: (async (_url: unknown, init?: RequestInit) => {
      const method = JSON.parse(new TextDecoder().decode(init?.body as Uint8Array)).method;
      if (method === 'tools/call') throw new Error('offline');
      return new Response('{"jsonrpc":"2.0","id":1,"result":{}}',
        { headers: { 'content-type': 'application/json', 'mcp-session-id': 'backend-s' } });
    }) as typeof fetch };
  const initialized = await forwardSessionQuery(new Request(`${publicOrigin}/mcp`, { method: 'POST',
    headers: { 'content-type': 'application/json' }, body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize' }) }),
    liveGrant, identity, store, options);
  assert.equal(initialized.status, 200);
  await initialized.text();
  const session = initialized.headers.get('mcp-session-id')!;

  const failed = await forwardSessionQuery(new Request(`${publicOrigin}/mcp`, { method: 'POST',
    headers: { 'content-type': 'application/json', 'mcp-session-id': session },
    body: JSON.stringify({ jsonrpc: '2.0', id: 9, method: 'tools/call', params: { name: 'brief', arguments: {} } }) }),
    liveGrant, identity, store, options);
  assert.equal(failed.status, 200);
  assert.deepEqual(await failed.json(), { jsonrpc: '2.0', id: 9,
    result: { content: [{ type: 'text', text: SAFE_TEXT }], isError: true } });
  assert.equal(failed.headers.get('mcp-session-id'), null);

  const listed = await forwardSessionQuery(new Request(`${publicOrigin}/mcp`, { method: 'POST',
    headers: { 'content-type': 'application/json', 'mcp-session-id': session },
    body: JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'tools/list' }) }), liveGrant, identity, store, options);
  assert.equal(listed.status, 200);
  await listed.text();
});
