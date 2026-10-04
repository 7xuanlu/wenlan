// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { renewSampleRoute } from '../scripts/renew-sample-route.mjs';
import { createSampleRuntime, beginSampleAuthorization, sampleOrigin } from './fixtures/sample-runtime.mjs';

test('real Worker conditional renewal preserves reviewer login and refuses stale boundary updates', { timeout: 30_000 }, async () => {
  const f = await createSampleRuntime();
  try {
    const original = { ...f.candidate };
    const input = { enrollment: f.device, connector: original, account: f.issued.account,
      relayOrigin: sampleOrigin, nextTunnelOrigin: 'https://replacement.trycloudflare.com' };
    const fetcher = (url, init) => {
      assert.equal(url, `${sampleOrigin}/devices/renew`);
      return f.request('/devices/renew', init);
    };
    f.candidate.tunnelOrigin = input.nextTunnelOrigin;
    assert.equal(await renewSampleRoute(input, { fetcher }), 'renewed');
    const a = await beginSampleAuthorization(f);
    const login = { username: f.issued.account.username, password: f.issued.password,
      clientId: a.client.client_id, approved: true, resource: `${sampleOrigin}/mcp`, space: original.space };
    const accepted = await f.post('/pairing/sample', login, { cookie: a.cookie, origin: sampleOrigin });
    assert.equal(accepted.status, 200);
    const callback = new URL((await accepted.json()).redirectTo);
    const response = await f.request('/oauth/token', { method: 'POST',
      headers: { 'content-type': 'application/x-www-form-urlencoded' },
      body: new URLSearchParams({ grant_type: 'authorization_code', code: callback.searchParams.get('code'),
        code_verifier: a.verifier, client_id: a.client.client_id, redirect_uri: a.redirectUri,
        resource: `${sampleOrigin}/mcp` }).toString() });
    assert.equal(response.status, 200);
    const tokens = await response.json();
    const mcp = () => f.post('/mcp', { jsonrpc: '2.0', id: 1, method: 'initialize' },
      { authorization: `Bearer ${tokens.access_token}` });
    assert.equal((await mcp()).status, 200);
    input.nextTunnelOrigin = f.candidate.tunnelOrigin = 'https://second.trycloudflare.com';
    assert.equal(await renewSampleRoute(input, { fetcher }), 'renewed');
    assert.equal((await mcp()).status, 200, 'existing OAuth grant survives same-boundary renewal');
    const headers = { authorization: `Bearer ${f.device.managementToken}`, 'x-wenlan-device-id': f.device.id };
    assert.equal((await f.post('/devices/renew', { ...f.candidate, expectedGeneration: 0 })).status, 401);
    for (const browser of [{ origin: sampleOrigin }, { cookie: 'session=synthetic' }]) {
      assert.equal((await f.post('/devices/renew', { ...f.candidate, expectedGeneration: 0 },
        { ...headers, ...browser })).status, 403);
    }
    for (const expectedGeneration of [undefined, null, -1, '0', 1.5]) assert.equal((await f.post('/devices/renew',
      { ...f.candidate, expectedGeneration }, headers)).status, 400);
    f.candidate.space = 'other';
    assert.equal((await f.post('/devices/refresh', f.candidate, headers)).status, 200, 'normal desktop API remains compatible');
    assert.equal(await renewSampleRoute(input, { fetcher }), 'rejected');
    assert.equal((await mcp()).status, 403, 'stale renewal cannot restore a revoked data boundary');
  } finally { await f.runtime.dispose(); }
});
