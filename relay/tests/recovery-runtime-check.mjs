// SPDX-License-Identifier: Apache-2.0
// Explicit deployment-artifact lane: requires a frozen Wrangler bundle.
import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { createHash, randomBytes } from 'node:crypto';

test('frozen deployment bundle retains grants and revocation across recovery restart', { timeout: 60_000 }, async () => {
  assert(process.env.WENLAN_RELAY_TOOLCHAIN, 'existing toolchain required');
  assert(process.env.WENLAN_RELAY_BUNDLE_DIR, 'frozen Wrangler bundle required');
  const requireTool = createRequire(join(resolve(process.env.WENLAN_RELAY_TOOLCHAIN), 'package.json'));
  const { Miniflare } = requireTool('miniflare');
  const scriptPath = join(resolve(process.env.WENLAN_RELAY_BUNDLE_DIR), 'worker.js');
  const directory = await mkdtemp(join(tmpdir(), 'wenlan-recovery-'));
  const origin = 'https://relay.wenlan.app';
  const candidate = { tunnelOrigin: 'https://synthetic.trycloudflare.com', backendToken: 'b'.repeat(64), space: 'review' };
  let backendCalls = 0;
  const options = {
    modules: true, scriptPath, modulesRoot: dirname(scriptPath),
    modulesRules: [{ type: 'Data', include: ['**/*.png'] }],
    compatibilityDate: '2026-09-08', host: '127.0.0.1', port: 0,
    bindings: { PUBLIC_ORIGIN: origin }, kvNamespaces: ['OAUTH_KV'],
    kvPersist: join(directory, 'kv'), durableObjectsPersist: join(directory, 'do'),
    durableObjects: { AUTHORITY: { className: 'RelayAuthority', useSQLite: true } },
    outboundService: request => {
      const url = new URL(request.url);
      assert.equal(url.origin, candidate.tunnelOrigin, 'no legacy/external network dependency');
      if (!request.headers.has('authorization')) return new Response(null, { status: 401 });
      assert.equal(request.headers.get('authorization'), `Bearer ${candidate.backendToken}`);
      if (url.pathname === '/connector-info') return Response.json({ contract_version: 1,
        server: 'wenlan-mcp', tool_profile: 'query-only', authentication: 'bearer', space: candidate.space });
      assert.equal(url.pathname, '/mcp');
      backendCalls++;
      return Response.json({ jsonrpc: '2.0', id: 1, result: { synthetic: true } },
        { headers: { 'mcp-session-id': 'synthetic-recovery-session' } });
    },
  };
  let runtime;
  const request = (path, init) => runtime.dispatchFetch(`${origin}${path}`, {
    ...init, credentials: 'omit', redirect: 'manual',
  });
  const post = (path, body, headers = {}) => request(path, { method: 'POST',
    headers: { 'content-type': 'application/json', ...headers }, body: JSON.stringify(body) });
  const restart = async () => {
    if (runtime) await runtime.dispose();
    runtime = new Miniflare(options);
    await runtime.ready;
  };
  try {
    await restart();
    const enrolled = await post('/devices', candidate);
    assert.equal(enrolled.status, 201);
    const device = await enrolled.json();
    const deviceHeaders = { authorization: `Bearer ${device.managementToken}`, 'x-wenlan-device-id': device.id };
    const registered = await post('/oauth/register', { client_name: 'Synthetic recovery client',
      redirect_uris: ['https://client.example/callback'], grant_types: ['authorization_code', 'refresh_token'],
      response_types: ['code'], token_endpoint_auth_method: 'none' });
    assert.equal(registered.status, 201);
    const client = await registered.json();
    const verifier = randomBytes(32).toString('base64url');
    const query = new URLSearchParams({ response_type: 'code', client_id: client.client_id,
      redirect_uri: 'https://client.example/callback', resource: `${origin}/mcp`, scope: 'wenlan:query',
      code_challenge_method: 'S256', code_challenge: createHash('sha256').update(verifier).digest('base64url') });
    const authorize = await request(`/authorize?${query}`);
    assert.equal(authorize.status, 303);
    const cookie = authorize.headers.get('set-cookie').split(';')[0];
    const pairId = cookie.split('=')[1].split('.')[0];
    await restart();
    const intent = await request(`/pairings/${pairId}`, { headers: deviceHeaders });
    assert.equal(intent.status, 200, 'pending pairing and device survive recovery');
    assert.equal((await post(`/pairings/${pairId}/approve`, { approved: true, clientId: client.client_id,
      resource: `${origin}/mcp`, space: candidate.space }, deviceHeaders)).status, 200);
    const complete = await post('/pairing/complete', {}, { cookie, origin });
    assert.equal(complete.status, 200);
    const redirect = new URL((await complete.json()).redirectTo);
    const exchanged = await request('/oauth/token', { method: 'POST',
      headers: { 'content-type': 'application/x-www-form-urlencoded' }, body: new URLSearchParams({
        grant_type: 'authorization_code', code: redirect.searchParams.get('code'), code_verifier: verifier,
        client_id: client.client_id, redirect_uri: 'https://client.example/callback', resource: `${origin}/mcp`,
      }).toString() });
    assert.equal(exchanged.status, 200);
    const tokens = await exchanged.json();
    const auth = { authorization: `Bearer ${tokens.access_token}` };
    const initialized = await post('/mcp', { jsonrpc: '2.0', id: 1, method: 'initialize' }, auth);
    assert.equal(initialized.status, 200);
    await initialized.text();
    const session = initialized.headers.get('mcp-session-id');
    assert(session && session !== 'synthetic-recovery-session');
    const headers = { ...auth, 'mcp-session-id': session };
    const brief = { jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'brief', arguments: {} } };
    await restart();
    const restored = await post('/mcp', brief, headers);
    assert.equal(restored.status, 200, 'OAuth token and authority session survive recovery');
    await restored.text();
    assert.equal((await post('/devices/revoke', {}, deviceHeaders)).status, 200);
    await restart();
    const callsBeforeDenial = backendCalls;
    const denied = await post('/mcp', brief, headers);
    assert.equal(denied.status, 403, 'valid token cannot use a revoked device route after recovery');
    await denied.text();
    assert.equal(backendCalls, callsBeforeDenial, 'revoked request never reaches the backend');
    const refresh = await request('/oauth/token', { method: 'POST',
      headers: { 'content-type': 'application/x-www-form-urlencoded' }, body: new URLSearchParams({
        grant_type: 'refresh_token', refresh_token: tokens.refresh_token, client_id: client.client_id,
      }).toString() });
    assert.equal(refresh.status, 400);
    assert.equal((await refresh.json()).error, 'invalid_grant');
    assert.equal((await post('/devices/revoke', {}, deviceHeaders)).status, 200, 'terminal revoke remains idempotent');
  } finally {
    if (runtime) await runtime.dispose();
    await rm(directory, { recursive: true, force: true });
  }
});
