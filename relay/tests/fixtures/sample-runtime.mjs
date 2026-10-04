// SPDX-License-Identifier: Apache-2.0
// Local synthetic fixture using the production Worker and public device API.
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash, randomBytes } from 'node:crypto';
import { prepareNewDeviceSampleAccount } from '../../src/sample-account.ts';

export const sampleOrigin = 'https://wenlan-relay.example';

export async function createSampleRuntime(configure = account => JSON.stringify(account), connector) {
  if (!process.env.WENLAN_RELAY_TOOLCHAIN) throw new Error('WENLAN_RELAY_TOOLCHAIN required');
  const requireTool = createRequire(join(resolve(process.env.WENLAN_RELAY_TOOLCHAIN), 'package.json'));
  const { Miniflare } = requireTool('miniflare');
  const { build } = requireTool('esbuild');
  const candidate = { tunnelOrigin: 'https://synthetic.trycloudflare.com', backendToken: 'b'.repeat(64), space: 'atlas-review' };
  let backendCalls = 0;
  const backend = async request => {
    const url = new URL(request.url);
    assert.equal(url.origin, candidate.tunnelOrigin);
    if (url.pathname === '/mcp') backendCalls++;
    if (connector) return connector(request, candidate);
    if (!request.headers.has('authorization')) return new Response(null, { status: 401 });
    assert.equal(request.headers.get('authorization'), `Bearer ${candidate.backendToken}`);
    if (url.pathname === '/connector-info') return Response.json({ contract_version: 1, server: 'wenlan-mcp',
      tool_profile: 'query-only', authentication: 'bearer', space: candidate.space });
    assert.equal(url.pathname, '/mcp');
    return Response.json({ jsonrpc: '2.0', id: 1, result: { synthetic: true } },
      { headers: { 'mcp-session-id': 'synthetic-sample-session' } });
  };
  const bundle = await build({ entryPoints: [fileURLToPath(new URL('../../src/worker.ts', import.meta.url))],
    bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022',
    external: ['cloudflare:workers'], loader: { '.png': 'binary' } });
  const options = { modules: true, script: bundle.outputFiles[0].text,
    compatibilityDate: '2026-09-08', host: '127.0.0.1', port: 0,
    bindings: { PUBLIC_ORIGIN: sampleOrigin },
    kvNamespaces: ['OAUTH_KV'], durableObjects: { AUTHORITY: { className: 'RelayAuthority', useSQLite: true } },
    outboundService: backend };
  const runtime = new Miniflare(options);
  const request = (path, init) => runtime.dispatchFetch(`${sampleOrigin}${path}`, { credentials: 'omit', redirect: 'manual', ...init });
  const post = (path, body, headers = {}) => request(path, { method: 'POST',
    headers: { 'content-type': 'application/json', ...headers }, body: JSON.stringify(body) });
  let device, issued;
  try {
    const enrolled = await post('/devices', candidate);
    assert.equal(enrolled.status, 201);
    device = await enrolled.json();
    issued = await prepareNewDeviceSampleAccount(device,
      { username: 'reviewer', resource: `${sampleOrigin}/mcp`, space: candidate.space });
    assert.ok(issued);
    const configured = configure(issued.account);
    await runtime.setOptions({ ...options, bindings: { ...options.bindings,
      ...(configured === undefined ? {} : { SAMPLE_ACCOUNT: configured }) } });
    assert.equal((await request('/__fixture')).status, 404, 'no privileged fixture endpoint');
  } catch (error) { await runtime.dispose(); throw error; }
  return { runtime, request, post, issued, device, candidate, backendCalls: () => backendCalls };
}

export async function beginSampleAuthorization(fixture, redirectUri = 'https://client.example/callback') {
  const response = await fixture.post('/oauth/register', { client_name: 'Synthetic sample client',
    redirect_uris: [redirectUri], grant_types: ['authorization_code', 'refresh_token'],
    response_types: ['code'], token_endpoint_auth_method: 'none' });
  assert.equal(response.status, 201);
  const client = await response.json();
  const verifier = randomBytes(32).toString('base64url');
  const query = new URLSearchParams({ response_type: 'code', client_id: client.client_id,
    redirect_uri: redirectUri, resource: `${sampleOrigin}/mcp`, scope: 'wenlan:query',
    code_challenge_method: 'S256', code_challenge: createHash('sha256').update(verifier).digest('base64url') });
  const authorizationPath = `/authorize?${query}`;
  const authorization = await fixture.request(authorizationPath);
  assert.equal(authorization.status, 303);
  return { client, verifier, redirectUri, authorizationPath,
    cookie: authorization.headers.get('set-cookie').split(';')[0] };
}
