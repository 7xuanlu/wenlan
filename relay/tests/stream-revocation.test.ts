// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { forwardQuery, type ConnectorRoute, type QueryGrant } from '../src/proxy.ts';

const publicOrigin = 'https://relay.example';
function fixture() {
  const grant: QueryGrant = { subject: 'owner', connectorId: 'device', space: 'review',
    generation: 1, scopes: ['wenlan:query'], expiresAt: Date.now() + 60_000 };
  let route: ConnectorRoute = { id: grant.connectorId, subject: grant.subject, space: grant.space,
    generation: 1, enabled: true, expiresAt: grant.expiresAt,
    tunnelOrigin: 'https://synthetic.trycloudflare.com', backendToken: 'b'.repeat(43) };
  let controller: ReadableStreamDefaultController<Uint8Array>;
  let cancelled = false;
  const upstream = new ReadableStream<Uint8Array>({ start(value) { controller = value; },
    cancel() { cancelled = true; } });
  return { grant, get route() { return route; }, set route(value) { route = value; },
    get controller() { return controller!; }, get cancelled() { return cancelled; },
    options: { publicOrigin, loadRoute: async () => ({ ...route }),
      fetch: (async () => new Response(upstream, { headers: { 'content-type': 'text/event-stream' } })) as typeof fetch } };
}
const request = () => new Request(`${publicOrigin}/mcp`);
async function within<T>(promise: Promise<T>, milliseconds: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout>;
  try { return await Promise.race([promise, new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error('TEST_TIMEOUT')), milliseconds);
  })]); } finally { clearTimeout(timer!); }
}

test('revocation prevents the next upstream chunk from reaching the client', async () => {
  const state = fixture();
  const response = await forwardQuery(request(), state.grant, state.options);
  const reader = response.body!.getReader();
  try {
    state.controller.enqueue(new TextEncoder().encode('data: first\n\n'));
    assert.equal(new TextDecoder().decode((await reader.read()).value), 'data: first\n\n');
    state.route = { ...state.route, enabled: false };
    state.controller.enqueue(new TextEncoder().encode('data: PRIVATE_AFTER_REVOKE\n\n'));
    await assert.rejects(within(reader.read(), 2500), /Connector stream unavailable/);
    assert.equal(state.cancelled, true);
  } finally { await reader.cancel().catch(() => {}); }
});

test('an idle stream is terminated after device revocation', async () => {
  const state = fixture();
  const response = await forwardQuery(request(), state.grant, state.options);
  const reader = response.body!.getReader();
  try {
    state.route = { ...state.route, generation: 2 };
    await assert.rejects(within(reader.read(), 2500), /Connector stream unavailable/);
    assert.equal(state.cancelled, true);
  } finally { await reader.cancel().catch(() => {}); }
});

test('oversized streamed responses are cancelled rather than returned successfully', async () => {
  const state = fixture();
  const response = await forwardQuery(request(), state.grant, state.options);
  try {
    state.controller.enqueue(new Uint8Array(2 * 1024 * 1024 + 1));
    state.controller.close();
    await assert.rejects(within(response.arrayBuffer(), 2500), /Connector stream unavailable/);
  } finally { await response.body?.cancel().catch(() => {}); }
});

test('revocation also interrupts waiting for upstream response headers', async () => {
  const state = fixture();
  const abort = new AbortController();
  const safety = setTimeout(() => abort.abort(), 2600);
  const changed = setTimeout(() => { state.route = { ...state.route, enabled: false }; }, 20);
  const started = Date.now();
  try {
    const result = await forwardQuery(new Request(`${publicOrigin}/mcp`, { signal: abort.signal }), state.grant, {
      ...state.options, fetch: ((_input, init) => new Promise<Response>((_resolve, reject) => {
        init!.signal!.addEventListener('abort', () => reject(new Error('upstream aborted')), { once: true });
      })) as typeof fetch,
    });
    assert.equal(result.status, 502);
    assert(Date.now() - started < 2300, 'revocation must abort before the client safety deadline');
  } finally { clearTimeout(safety); clearTimeout(changed); }
});

test('client cancellation cancels upstream and stops periodic storage reads', async () => {
  const state = fixture();
  let loads = 0;
  const response = await forwardQuery(request(), state.grant, { ...state.options,
    loadRoute: async () => { loads++; return { ...state.route }; } });
  await response.body!.cancel();
  assert.equal(state.cancelled, true);
  const completedLoads = loads;
  await new Promise(resolve => setTimeout(resolve, 1150));
  assert.equal(loads, completedLoads);
});

test('source bytes are unchanged across chunk boundaries', async () => {
  const state = fixture();
  const response = await forwardQuery(request(), state.grant, state.options);
  const bytes = new TextEncoder().encode('data: {"text":"synthetic result"}\n\n');
  state.controller.enqueue(bytes.slice(0, 5));
  state.controller.enqueue(bytes.slice(5));
  state.controller.close();
  assert.deepEqual(new Uint8Array(await response.arrayBuffer()), bytes);
});

test('a failed authorization read terminates a response without leaking storage details', async () => {
  const state = fixture();
  let fail = false;
  const response = await forwardQuery(request(), state.grant, { ...state.options,
    loadRoute: async () => { if (fail) throw new Error('PRIVATE_STORAGE_DETAIL'); return { ...state.route }; } });
  const reader = response.body!.getReader();
  fail = true;
  try {
    await assert.rejects(within(reader.read(), 2500), error =>
      error instanceof Error && error.message === 'Connector stream unavailable');
    assert.equal(state.cancelled, true);
  } finally { await reader.cancel().catch(() => {}); }
});

test('an unresponsive authorization store cannot keep an idle stream alive', async () => {
  const state = fixture();
  let stalled = false;
  const response = await forwardQuery(request(), state.grant, { ...state.options,
    loadRoute: async () => stalled ? new Promise<ConnectorRoute | null>(() => {}) : { ...state.route } });
  const reader = response.body!.getReader();
  stalled = true;
  try {
    await assert.rejects(within(reader.read(), 3800), /Connector stream unavailable/);
    assert.equal(state.cancelled, true);
  } finally { await reader.cancel().catch(() => {}); }
});
