// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash, randomBytes } from 'node:crypto';

const toolchain = process.env.WENLAN_RELAY_TOOLCHAIN;
if (!toolchain) throw new Error('Set WENLAN_RELAY_TOOLCHAIN to an installed Wrangler project directory');
const requireTool = createRequire(join(resolve(toolchain), 'package.json'));
const { Miniflare } = requireTool('miniflare');
const { build } = requireTool('esbuild');
const origin = 'https://wenlan-relay.example';
const resource = `${origin}/mcp`;
const redirectUri = 'https://client.example/callback';
const clientRegistrationTTL = 90 * 24 * 60 * 60;

test('real OAuth library requires pairing and enforces PKCE, resource and token lifecycle', { timeout: 60_000 }, async t => {
  const bundle = await build({ entryPoints: [fileURLToPath(new URL('./fixtures/oauth-worker.ts', import.meta.url))],
    bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022', external: ['cloudflare:workers'] });
  const runtime = new Miniflare({ modules: true, script: bundle.outputFiles[0].text,
    compatibilityDate: '2026-09-08', host: '127.0.0.1', port: 0,
    kvNamespaces: ['OAUTH_KV'],
    durableObjects: { AUTHORITY: { className: 'FixtureOAuthAuthority', useSQLite: true } },
    outboundService: () => new Response('External network disabled', { status: 503 }),
  });
  // These API calls use bearer tokens, never browser cookies. Explicit omit also
  // avoids Undici's automatic 401 replay of Miniflare's streamed request body.
  const request = (path, init) => runtime.dispatchFetch(`${origin}${path}`, { ...init, credentials: 'omit' });
  const post = (path, value) => request(path, { method: 'POST',
    headers: { 'content-type': 'application/json' }, body: JSON.stringify(value) });
  const token = params => request('/oauth/token', { method: 'POST',
    headers: { 'content-type': 'application/x-www-form-urlencoded' }, body: new URLSearchParams(params).toString() });
  const clientExpiration = async clientId => {
    const kv = await runtime.getKVNamespace('OAUTH_KV');
    const key = `client:${clientId}`;
    const entry = (await kv.list({ prefix: key })).keys.find(item => item.name === key);
    return entry?.expiration;
  };
  let client;
  let device;
  async function authorize(overrides = {}) {
    const verifier = randomBytes(32).toString('base64url');
    const params = { response_type: 'code', client_id: client.client_id, redirect_uri: redirectUri,
      scope: 'wenlan:query', resource, state: 'synthetic-state',
      code_challenge: createHash('sha256').update(verifier).digest('base64url'), code_challenge_method: 'S256', ...overrides };
    const response = await request(`/authorize?${new URLSearchParams(params)}`);
    return { response, verifier };
  }
  async function code(selectedClient = client, selectedDevice = device) {
    const { response, verifier } = await authorize({ client_id: selectedClient.client_id });
    assert.equal(response.status, 200);
    const pair = await response.json();
    assert.equal(await (await post('/fixture/finish', pair)).json(), null, 'no desktop consent');
    assert.equal(await (await post('/fixture/approve', { ...pair, deviceId: selectedDevice.id,
      managementToken: selectedDevice.managementToken, clientId: selectedClient.client_id })).json(), true);
    const redirect = await (await post('/fixture/finish', pair)).json();
    assert.ok(redirect);
    const url = new URL(redirect);
    assert.equal(url.origin + url.pathname, redirectUri);
    assert.equal(url.searchParams.get('state'), 'synthetic-state');
    assert.equal(url.searchParams.get('iss'), origin);
    assert.equal(await (await post('/fixture/finish', pair)).json(), null, 'consumed pairing replay');
    return { code: url.searchParams.get('code'), verifier };
  }
  const exchange = (grant, overrides = {}) => token({ grant_type: 'authorization_code',
    code: grant.code, client_id: client.client_id, redirect_uri: redirectUri,
    code_verifier: grant.verifier, resource, ...overrides });
  const query = async accessToken => {
    const headers = { 'content-type': 'application/json', authorization: `Bearer ${accessToken}` };
    const initialized = await request('/mcp', { method: 'POST', headers,
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize' }) });
    if (!initialized.ok) return initialized;
    const session = initialized.headers.get('mcp-session-id');
    await initialized.text();
    return request('/mcp', { method: 'POST', headers: { ...headers, 'mcp-session-id': session },
      body: JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'tools/call', params: { name: 'brief', arguments: {} } }) });
  };
  try {
    await t.test('discovery and unauthorized challenge identify the fixed resource', async () => {
      const denied = await request('/mcp');
      assert.equal(denied.status, 401);
      assert.match(denied.headers.get('www-authenticate'), /oauth-protected-resource/);
      const metadata = await (await request('/.well-known/oauth-protected-resource/mcp')).json();
      assert.equal(metadata.resource, resource);
      assert.deepEqual(metadata.scopes_supported, ['wenlan:query']);
      const server = await (await request('/.well-known/oauth-authorization-server')).json();
      assert.equal(server.issuer, origin);
      assert.deepEqual(server.code_challenge_methods_supported, ['S256']);
      assert.equal(server.authorization_response_iss_parameter_supported, true);
    });
    const registration = await post('/oauth/register', { client_name: 'Synthetic test client',
      redirect_uris: [redirectUri], grant_types: ['authorization_code', 'refresh_token'],
      response_types: ['code'], token_endpoint_auth_method: 'none' });
    assert.equal(registration.status, 201);
    client = await registration.json();
    await t.test('DCR registration is retained for exactly 90 days', async () => {
      const expiration = await clientExpiration(client.client_id);
      const expected = Math.floor(Date.now() / 1000) + clientRegistrationTTL;
      assert.ok(Number.isInteger(expiration), 'DCR client KV record must have an expiration');
      assert.ok(Math.abs(expiration - expected) <= 5,
        `DCR client expiration ${expiration} should be near ${expected}`);
    });
    device = await (await post('/fixture/enroll', {})).json();
    await t.test('invalid redirects, plain PKCE, scope and resource are rejected', async () => {
      for (const invalid of [{ redirect_uri: 'https://attacker.example/callback' },
        { code_challenge_method: 'plain' }, { scope: 'write' }, { resource: 'https://other.example/mcp' }]) {
        assert.equal((await authorize(invalid)).response.status, 400);
      }
    });
    await t.test('wrong verifier and cross-resource exchange cannot mint usable tokens', async () => {
      for (const invalid of [{ code_verifier: randomBytes(32).toString('base64url') },
        { resource: 'https://other.example/mcp' }]) {
        const response = await exchange(await code(), invalid);
        assert.equal(response.status, 400);
        assert.equal((await response.json()).access_token, undefined);
      }
    });
    await t.test('authorization code replay revokes its previously issued grant', async () => {
      const grant = await code();
      const response = await exchange(grant);
      assert.equal(response.status, 200);
      const replayed = await response.json();
      assert.equal((await query(replayed.access_token)).status, 200);
      assert.equal((await exchange(grant)).status, 400);
      const rejectedRefresh = await token({ grant_type: 'refresh_token', client_id: client.client_id,
        refresh_token: replayed.refresh_token, resource });
      assert.equal(rejectedRefresh.status, 400);
    });
    await t.test('concurrent exchanges cannot consume one authorization code twice', async () => {
      const grant = await code();
      const results = await Promise.all(Array.from({ length: 6 }, () => exchange(grant)));
      assert.equal(results.filter(response => response.status === 200).length, 1);
      for (const response of results) {
        const body = await response.json();
        if (response.status === 200) {
          assert([401, 403].includes((await query(body.access_token)).status), 'replay must invalidate the winning credential too');
          assert.equal((await token({ grant_type: 'refresh_token', client_id: client.client_id,
            refresh_token: body.refresh_token, resource })).status, 400);
        }
      }
    });
    let tokens;
    await t.test('a fresh paired authorization reaches the scoped backend', async () => {
      const response = await exchange(await code());
      assert.equal(response.status, 200);
      tokens = await response.json();
      assert.ok(tokens.access_token);
      assert.ok(tokens.refresh_token);
      assert.equal(tokens.scope, 'wenlan:query');
      assert.equal((await query(tokens.access_token)).status, 200);
    });
    await t.test('refresh rotates credentials and rejects resource/scope escalation', async () => {
      for (const invalid of [{ resource: 'https://other.example/mcp' }, { scope: 'write' }]) {
        const response = await token({ grant_type: 'refresh_token', client_id: client.client_id,
          refresh_token: tokens.refresh_token, resource, ...invalid });
        assert.equal(response.status, 400);
      }
      const response = await token({ grant_type: 'refresh_token', client_id: client.client_id,
        refresh_token: tokens.refresh_token, resource });
      assert.equal(response.status, 200);
      const refreshed = await response.json();
      assert.notEqual(refreshed.refresh_token, tokens.refresh_token);
      assert.equal((await query(refreshed.access_token)).status, 200);
      tokens = refreshed;
    });
    await t.test('real OAuth grants cannot swap MCP sessions and refresh preserves the original binding', async () => {
      const first = tokens;
      const registered = await post('/oauth/register', { client_name: 'Second synthetic client',
        redirect_uris: [redirectUri], grant_types: ['authorization_code', 'refresh_token'],
        response_types: ['code'], token_endpoint_auth_method: 'none' });
      assert.equal(registered.status, 201);
      const secondClient = await registered.json();
      const secondResponse = await exchange(await code(secondClient), { client_id: secondClient.client_id });
      assert.equal(secondResponse.status, 200);
      const second = await secondResponse.json();
      const headers = access => ({ 'content-type': 'application/json', authorization: `Bearer ${access}` });
      const initialized = await request('/mcp', { method: 'POST', headers: headers(first.access_token),
        body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize' }) });
      assert.equal(initialized.status, 200);
      const session = initialized.headers.get('mcp-session-id');
      await initialized.text();
      const useSession = (access, method = 'POST') => request('/mcp', { method,
        headers: { ...headers(access), 'mcp-session-id': session },
        body: method === 'POST' ? JSON.stringify({ jsonrpc: '2.0', id: 2, method: 'tools/list' }) : undefined });
      for (const method of ['GET', 'POST', 'DELETE']) assert.equal((await useSession(second.access_token, method)).status, 404);
      const refreshed = await (await token({ grant_type: 'refresh_token', client_id: client.client_id,
        refresh_token: first.refresh_token, resource })).json();
      const continued = await useSession(refreshed.access_token);
      assert.equal(continued.status, 200);
      await continued.text();
      const deleted = await useSession(refreshed.access_token, 'DELETE');
      assert.equal(deleted.status, 200);
      await deleted.body?.cancel();
      assert.equal((await useSession(refreshed.access_token)).status, 404);
      tokens = refreshed;
    });
    await t.test('expired access credentials fail while a valid refresh can renew them', async () => {
      const kv = await runtime.getKVNamespace('OAUTH_KV');
      const [userId, grantId] = tokens.access_token.split(':');
      const entries = await kv.list({ prefix: `token:${userId}:${grantId}:` });
      assert.ok(entries.keys.length > 0);
      for (const entry of entries.keys) {
        const value = await kv.get(entry.name, 'json');
        await kv.put(entry.name, JSON.stringify({ ...value, expiresAt: Math.floor(Date.now() / 1000) - 1 }));
      }
      assert.equal((await query(tokens.access_token)).status, 401);
      const response = await token({ grant_type: 'refresh_token', client_id: client.client_id,
        refresh_token: tokens.refresh_token, resource });
      assert.equal(response.status, 200);
      tokens = await response.json();
      assert.equal((await query(tokens.access_token)).status, 200);
    });
    await t.test('device revocation prevents existing OAuth access reaching the connector', async () => {
      assert.equal(await (await post('/fixture/revoke', { deviceId: device.id,
        managementToken: device.managementToken })).json(), true);
      assert.equal((await query(tokens.access_token)).status, 403);
      assert.equal((await query('invalid')).status, 401);
      const rejected = await token({ grant_type: 'refresh_token', client_id: client.client_id,
        refresh_token: tokens.refresh_token, resource });
      assert.equal(rejected.status, 400);
      assert.equal((await rejected.json()).error, 'invalid_grant');
    });
    await t.test('expired DCR clients require fresh registration and consent', async () => {
      const kv = await runtime.getKVNamespace('OAUTH_KV');
      const registered = await post('/oauth/register', { client_name: 'Expiring synthetic client',
        redirect_uris: [redirectUri], grant_types: ['authorization_code', 'refresh_token'],
        response_types: ['code'], token_endpoint_auth_method: 'none' });
      assert.equal(registered.status, 201);
      const expiringClient = await registered.json();
      const clientKey = `client:${expiringClient.client_id}`;
      const registeredExpiration = await clientExpiration(expiringClient.client_id);
      const expected = Math.floor(Date.now() / 1000) + clientRegistrationTTL;
      assert.ok(Number.isInteger(registeredExpiration));
      assert.ok(Math.abs(registeredExpiration - expected) <= 5);
      // A distinct test expiry detects sliding writes even within one second.
      const fixedExpiration = Math.floor(Date.now() / 1000) + 3600;
      await kv.put(clientKey, await kv.get(clientKey), { expiration: fixedExpiration });
      const lifecycleDevice = await (await post('/fixture/enroll', {})).json();
      const grant = await code(expiringClient, lifecycleDevice);
      assert.equal(await clientExpiration(expiringClient.client_id), fixedExpiration,
        'authorization must not slide DCR expiration');
      const exchanged = await exchange(grant, { client_id: expiringClient.client_id });
      assert.equal(exchanged.status, 200);
      let lifecycleTokens = await exchanged.json();
      assert.equal(await clientExpiration(expiringClient.client_id), fixedExpiration,
        'code exchange must not slide DCR expiration');
      const refreshed = await token({ grant_type: 'refresh_token', client_id: expiringClient.client_id,
        refresh_token: lifecycleTokens.refresh_token, resource });
      assert.equal(refreshed.status, 200);
      lifecycleTokens = await refreshed.json();
      assert.equal(await clientExpiration(expiringClient.client_id), fixedExpiration,
        'refresh must not slide DCR expiration');

      const [userId, grantId] = lifecycleTokens.refresh_token.split(':');
      const grantKey = `grant:${userId}:${grantId}`;
      assert.ok(await kv.get(grantKey), 'grant must exist before modeled client expiry');
      await kv.delete(clientKey);
      assert.equal(await kv.get(clientKey), null, 'exact synthetic client key must be absent');
      assert.ok(await kv.get(grantKey), 'removing the client record must retain the grant');
      assert.equal((await authorize({ client_id: expiringClient.client_id })).response.status, 400);
      const expiredRefresh = await token({ grant_type: 'refresh_token', client_id: expiringClient.client_id,
        refresh_token: lifecycleTokens.refresh_token, resource });
      assert.equal(expiredRefresh.status, 401);

      const freshRegistration = await post('/oauth/register', { client_name: 'Fresh synthetic client',
        redirect_uris: [redirectUri], grant_types: ['authorization_code', 'refresh_token'],
        response_types: ['code'], token_endpoint_auth_method: 'none' });
      assert.equal(freshRegistration.status, 201);
      const freshClient = await freshRegistration.json();
      const freshGrant = await code(freshClient, lifecycleDevice);
      const freshExchange = await exchange(freshGrant, { client_id: freshClient.client_id });
      assert.equal(freshExchange.status, 200);
      assert.ok((await freshExchange.json()).access_token);
    });
  } finally { await runtime.dispose(); }
});
