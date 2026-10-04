// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { forwardQuery, type ConnectorRoute, type QueryGrant } from '../src/proxy.ts';

const publicOrigin = 'https://relay.example';
const KEEPALIVE = ': wenlan keepalive\n\n';

function fixture(contentType = 'text/event-stream') {
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
      fetch: (async () => new Response(upstream, { headers: { 'content-type': contentType } })) as typeof fetch } };
}
const request = () => new Request(`${publicOrigin}/mcp`);
const encode = (text: string) => new TextEncoder().encode(text);
const decode = (value: Uint8Array | undefined) => new TextDecoder().decode(value);

async function within<T>(promise: Promise<T>, milliseconds: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout>;
  try { return await Promise.race([promise, new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error('TEST_TIMEOUT')), milliseconds);
  })]); } finally { clearTimeout(timer!); }
}

const sleep = (milliseconds: number) => new Promise(resolve => setTimeout(resolve, milliseconds));

test('idle stream emits keepalive on a safe event boundary', { timeout: 10_000 }, async () => {
  const state = fixture();
  const response = await forwardQuery(request(), state.grant, state.options);
  const reader = response.body!.getReader();
  try {
    state.controller.enqueue(encode('data: first\n\n'));
    assert.equal(decode((await within(reader.read(), 2500)).value), 'data: first\n\n');
    // Downstream demand: keep a read pending across the 1s recheck.
    assert.equal(decode((await within(reader.read(), 2500)).value), KEEPALIVE);
  } finally { await reader.cancel().catch(() => {}); }
});

test('split LF boundary across chunks still allows heartbeat', { timeout: 10_000 }, async () => {
  const state = fixture();
  const response = await forwardQuery(request(), state.grant, state.options);
  const reader = response.body!.getReader();
  try {
    state.controller.enqueue(encode('data: split\n'));
    state.controller.enqueue(encode('\n'));
    assert.equal(decode((await within(reader.read(), 2500)).value), 'data: split\n');
    assert.equal(decode((await within(reader.read(), 2500)).value), '\n');
    assert.equal(decode((await within(reader.read(), 2500)).value), KEEPALIVE);
  } finally { await reader.cancel().catch(() => {}); }
});

test('split CRLF boundary across chunks still allows heartbeat', { timeout: 10_000 }, async () => {
  const state = fixture();
  const response = await forwardQuery(request(), state.grant, state.options);
  const reader = response.body!.getReader();
  try {
    state.controller.enqueue(encode('data: split\r\n'));
    state.controller.enqueue(encode('\r\n'));
    assert.equal(decode((await within(reader.read(), 2500)).value), 'data: split\r\n');
    assert.equal(decode((await within(reader.read(), 2500)).value), '\r\n');
    assert.equal(decode((await within(reader.read(), 2500)).value), KEEPALIVE);
  } finally { await reader.cancel().catch(() => {}); }
});

test('no heartbeat inside a partial event across the recheck window', { timeout: 10_000 }, async () => {
  const state = fixture();
  const response = await forwardQuery(request(), state.grant, state.options);
  const reader = response.body!.getReader();
  try {
    state.controller.enqueue(encode('data: partial'));
    assert.equal(decode((await within(reader.read(), 2500)).value), 'data: partial');
    // Keep the pending demand promise; a 1.2s idle window must stay silent.
    const pending = reader.read();
    const outcome = await Promise.race([
      pending.then(value => ({ kind: 'data' as const, value })),
      sleep(1200).then(() => ({ kind: 'silent' as const })),
    ]);
    assert.equal(outcome.kind, 'silent');
    // Complete the event afterwards; the kept promise must deliver it intact.
    state.controller.enqueue(encode('\n\n'));
    const completed = await within(pending, 2500);
    assert.equal(decode(completed.value), '\n\n');
  } finally { await reader.cancel().catch(() => {}); }
});

test('no heartbeat inside a partial field line across the recheck window', { timeout: 10_000 }, async () => {
  const state = fixture();
  const response = await forwardQuery(request(), state.grant, state.options);
  const reader = response.body!.getReader();
  try {
    state.controller.enqueue(encode('data: first\n\n'));
    assert.equal(decode((await within(reader.read(), 2500)).value), 'data: first\n\n');
    state.controller.enqueue(encode('data: second'));
    assert.equal(decode((await within(reader.read(), 2500)).value), 'data: second');
    const pending = reader.read();
    const outcome = await Promise.race([
      pending.then(value => ({ kind: 'data' as const, value })),
      sleep(1200).then(() => ({ kind: 'silent' as const })),
    ]);
    assert.equal(outcome.kind, 'silent');
    state.controller.enqueue(encode('\n\n'));
    assert.equal(decode((await within(pending, 2500)).value), '\n\n');
  } finally { await reader.cancel().catch(() => {}); }
});

test('non-SSE JSON responses get no heartbeat', { timeout: 10_000 }, async () => {
  const state = fixture('application/json');
  const response = await forwardQuery(request(), state.grant, state.options);
  const reader = response.body!.getReader();
  try {
    const pending = reader.read();
    const outcome = await Promise.race([
      pending.then(value => ({ kind: 'data' as const, value })),
      sleep(1200).then(() => ({ kind: 'silent' as const })),
    ]);
    assert.equal(outcome.kind, 'silent');
  } finally { await reader.cancel().catch(() => {}); }
});

test('consumer cancel ends storage reads and upstream', { timeout: 10_000 }, async () => {
  const state = fixture();
  let loads = 0;
  const response = await forwardQuery(request(), state.grant, { ...state.options,
    loadRoute: async () => { loads++; return { ...state.route }; } });
  await response.body!.cancel();
  assert.equal(state.cancelled, true);
  const completedLoads = loads;
  await sleep(1200);
  assert.equal(loads, completedLoads);
});

test('keepalive is bounded by downstream backpressure', { timeout: 10_000 }, async () => {
  const state = fixture('Text/Event-Stream; charset=utf-8');
  const response = await forwardQuery(request(), state.grant, state.options);
  try {
    await sleep(2200);
    state.controller.close();
    assert.equal(await response.text(), KEEPALIVE, 'no consumer demand must not accumulate timer output');
  } finally { await response.body!.cancel().catch(() => {}); }
});

test('keepalive cannot exceed the response byte budget', { timeout: 10_000 }, async () => {
  const state = fixture();
  const response = await forwardQuery(request(), state.grant, state.options);
  const reader = response.body!.getReader();
  try {
    const fullBudget = new Uint8Array(2 * 1024 * 1024).fill(32);
    fullBudget.set(encode('\n\n'), fullBudget.length - 2);
    state.controller.enqueue(fullBudget);
    assert.equal((await within(reader.read(), 2500)).value!.byteLength, fullBudget.length);
    await assert.rejects(within(reader.read(), 2500), /Connector stream unavailable/);
    assert.equal(state.cancelled, true);
  } finally { await reader.cancel().catch(() => {}); }
});

test('revocation after first data prevents further data and heartbeat', { timeout: 10_000 }, async () => {
  const state = fixture();
  const response = await forwardQuery(request(), state.grant, state.options);
  const reader = response.body!.getReader();
  try {
    state.controller.enqueue(encode('data: first\n\n'));
    assert.equal(decode((await within(reader.read(), 2500)).value), 'data: first\n\n');
    state.route = { ...state.route, enabled: false };
    state.controller.enqueue(encode('data: PRIVATE_AFTER_REVOKE\n\n'));
    await assert.rejects(within(reader.read(), 2500), /Connector stream unavailable/);
    assert.equal(state.cancelled, true);
  } finally { await reader.cancel().catch(() => {}); }
});
