// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';

if (!process.env.WENLAN_RELAY_TOOLCHAIN) throw new Error('WENLAN_RELAY_TOOLCHAIN required');
const requireTool = createRequire(join(resolve(process.env.WENLAN_RELAY_TOOLCHAIN), 'package.json'));
const { Miniflare } = requireTool('miniflare');
const { build } = requireTool('esbuild');
const origin = 'https://wenlan-relay.example';
const candidate = { tunnelOrigin: 'https://synthetic.trycloudflare.com',
  backendToken: 'b'.repeat(43), space: 'review' };

test('actual Worker capacity is durable, rolls back partial enrollment and preserves revoke/cleanup', { timeout: 60_000 }, async () => {
  const options = { bundle: true, write: false, format: 'esm', platform: 'browser',
    target: 'es2022', external: ['cloudflare:workers'], loader: { '.png': 'binary' } };
  const bundle = await build({ ...options,
    entryPoints: [fileURLToPath(new URL('./fixtures/capacity-worker.ts', import.meta.url))] });
  const production = await build({ ...options,
    entryPoints: [fileURLToPath(new URL('../src/worker.ts', import.meta.url))] });
  assert(!production.outputFiles[0].text.includes('/__capacity_fixture'));
  assert(!production.outputFiles[0].text.includes('CapacityFixture'));
  const dir = await mkdtemp(join(tmpdir(), 'wenlan-relay-capacity-'));
  const runtimeOptions = { modules: true, script: bundle.outputFiles[0].text,
    compatibilityDate: '2026-09-08', host: '127.0.0.1', port: 0,
    bindings: { PUBLIC_ORIGIN: origin }, kvNamespaces: ['OAUTH_KV'],
    kvPersist: join(dir, 'kv'), durableObjectsPersist: join(dir, 'do'),
    durableObjects: { AUTHORITY: { className: 'CapacityFixture', useSQLite: true } },
    outboundService: request => {
      assert.equal(new URL(request.url).origin, candidate.tunnelOrigin);
      assert.equal(new URL(request.url).pathname, '/connector-info');
      if (!request.headers.has('authorization')) return new Response(null, { status: 401 });
      assert.equal(request.headers.get('authorization'), `Bearer ${candidate.backendToken}`);
      return Response.json({ contract_version: 1, server: 'wenlan-mcp',
        tool_profile: 'query-only', authentication: 'bearer', space: candidate.space });
    } };
  let runtime = new Miniflare(runtimeOptions);
  const fixture = async operation => {
    const namespace = await runtime.getDurableObjectNamespace('AUTHORITY');
    const response = await namespace.get(namespace.idFromName('wenlan-v1')).fetch(`${origin}/__capacity_fixture`, {
      method: 'POST', body: JSON.stringify({ operation }),
    });
    assert.equal(response.status, 200);
    return response.json();
  };
  const post = (path, value, headers = {}) => runtime.dispatchFetch(`${origin}${path}`, {
    method: 'POST', headers: { 'content-type': 'application/json', ...headers }, body: JSON.stringify(value),
  });
  try {
    const enrolled = await post('/devices', candidate);
    assert.equal(enrolled.status, 201);
    const device = await enrolled.json();
    const credential = { authorization: `Bearer ${device.managementToken}`, 'x-wenlan-device-id': device.id };
    const registered = await post('/oauth/register', { client_name: 'Synthetic capacity client',
      redirect_uris: ['https://client.example/callback'], grant_types: ['authorization_code'],
      response_types: ['code'], token_endpoint_auth_method: 'none' });
    assert.equal(registered.status, 201);
    const client = await registered.json();
    assert.deepEqual(await fixture('fill'), { count: 8192, devices: 1, connectors: 1 });
    await runtime.dispose();
    runtime = new Miniflare(runtimeOptions);
    assert.deepEqual(await fixture('inspect'), { count: 8192, devices: 1, connectors: 1 });
    const params = new URLSearchParams({ response_type: 'code', client_id: client.client_id,
      redirect_uri: 'https://client.example/callback', resource: `${origin}/mcp`, scope: 'wenlan:query',
      code_challenge_method: 'S256', code_challenge: 'A'.repeat(43) });
    const authorization = await runtime.dispatchFetch(`${origin}/authorize?${params}`, { redirect: 'manual' });
    assert.equal(authorization.status, 503, 'capacity is service unavailability, not invalid OAuth input');
    assert.equal(authorization.headers.get('set-cookie'), null);
    assert.equal((await post('/devices', candidate)).status, 503);
    assert.deepEqual(await fixture('inspect'), { count: 8192, devices: 1, connectors: 1 });
    await fixture('release-one');
    assert.equal((await post('/devices', candidate)).status, 503, 'one free row cannot admit a two-record device');
    assert.deepEqual(await fixture('inspect'), { count: 8191, devices: 1, connectors: 1 });
    assert.equal((await runtime.dispatchFetch(`${origin}/grants`, { headers: credential })).status, 200);
    assert.equal((await post('/devices/revoke', {}, credential)).status, 200, 'capacity must not block revocation');
    assert.deepEqual(await fixture('alarm'), { count: 8189, devices: 0, connectors: 0 });
    // The previous attempts consumed this peer's hourly enrollment budget.
    const fresh = await post('/devices', candidate, { 'cf-connecting-ip': '203.0.113.22' });
    assert.equal(fresh.status, 201);
    await fresh.body.cancel();
    assert.deepEqual(await fixture('inspect'), { count: 8191, devices: 1, connectors: 1 });
    await fixture('release-one');
    const competing = await Promise.all(Array.from({ length: 3 }, (_, n) => post('/devices', candidate,
      { 'cf-connecting-ip': `203.0.113.${30 + n}` })));
    assert.deepEqual(competing.map(response => response.status).sort(), [201, 503, 503]);
    for (const response of competing) await response.body?.cancel();
    assert.deepEqual(await fixture('inspect'), { count: 8192, devices: 2, connectors: 2 });
  } finally {
    await runtime.dispose();
    await rm(dir, { recursive: true, force: true });
  }
});
