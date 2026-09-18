// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { createSampleRuntime, beginSampleAuthorization, sampleOrigin as origin } from './fixtures/sample-runtime.mjs';

test('sample login uses real OAuth issuance and normal grant enforcement without device approval', { timeout: 30_000 }, async () => {
  const f = await createSampleRuntime();
  try {
    const a = await beginSampleAuthorization(f);
    const headers = { cookie: a.cookie, origin, 'sec-fetch-site': 'same-origin' };
    const login = { username: f.issued.account.username, password: f.issued.password, approved: true,
      clientId: a.client.client_id, resource: `${origin}/mcp`, space: f.candidate.space };
    assert.equal((await f.request('/pairing/sample')).status, 401);
    const pairing = await f.request('/pairing', { headers: { cookie: a.cookie } });
    assert.match(await pairing.text(), /Connect a sample library/);
    const page = await f.request('/pairing/sample', { headers: { cookie: a.cookie } });
    assert.equal(page.status, 200);
    const html = await page.text();
    for (const secret of [f.issued.password, f.issued.account.passwordHash, f.issued.account.managementHash,
      f.device.id, f.device.managementToken, f.candidate.backendToken, a.cookie.split('.').at(-1)]) assert(!html.includes(secret));
    assert.match(html, /type="password"/);
    assert.match(html, /type="checkbox" required/);
    assert.match(html, /atlas-review/);
    assert.equal(page.headers.get('cache-control'), 'no-store');
    assert.match(page.headers.get('content-security-policy'), /form-action 'self'/);
    assert.equal((await f.post('/pairing/sample', login, { cookie: a.cookie })).status, 403);
    assert.equal((await f.post('/pairing/sample', login, { ...headers, origin: 'https://attacker.example' })).status, 403);
    assert.equal((await f.post('/pairing/sample', login, { ...headers, 'sec-fetch-site': 'cross-site' })).status, 403);
    let peer = 1;
    for (const invalid of [{ password: 'c'.repeat(64) }, { approved: false }, { clientId: 'wrong-client' },
      { resource: 'https://other.example/mcp' }, { space: 'private' }, { deviceId: f.device.id },
      { password: f.device.managementToken }]) {
      const response = await f.post('/pairing/sample', { ...login, ...invalid },
        { ...headers, 'cf-connecting-ip': `192.0.2.${peer++}` });
      assert.equal(response.status, 401);
      assert.deepEqual(await response.json(), { error: 'Sample account sign-in rejected' });
    }
    assert.equal((await f.post('/pairing/complete', {}, headers)).status, 409, 'failed login cannot approve');
    const completed = await f.post('/pairing/sample', login, headers);
    assert.equal(completed.status, 200, await completed.clone().text());
    assert.match(completed.headers.get('set-cookie'), /Max-Age=0/);
    for (const flag of ['HttpOnly', 'Secure', 'SameSite=Lax', 'Path=/']) assert(completed.headers.get('set-cookie').includes(flag));
    const callback = new URL((await completed.json()).redirectTo);
    assert.equal(callback.origin, 'https://client.example');
    assert.equal(callback.searchParams.get('iss'), origin);
    const tokenBody = new URLSearchParams({ grant_type: 'authorization_code', code: callback.searchParams.get('code'),
      code_verifier: a.verifier, client_id: a.client.client_id, redirect_uri: a.redirectUri, resource: `${origin}/mcp` }).toString();
    const exchange = () => f.request('/oauth/token', { method: 'POST',
      headers: { 'content-type': 'application/x-www-form-urlencoded' }, body: tokenBody });
    const tokensResponse = await exchange();
    assert.equal(tokensResponse.status, 200);
    const tokens = await tokensResponse.json();
    assert.equal((await f.post('/pairing/sample', login, headers)).status, 401, 'pairing cannot be replayed');
    assert.equal((await f.request('/mcp', { headers: { authorization: `Bearer ${f.issued.password}` } })).status, 401);
    const initialized = await f.post('/mcp', { jsonrpc: '2.0', id: 1, method: 'initialize' },
      { authorization: `Bearer ${tokens.access_token}` });
    assert.equal(initialized.status, 200);
    await initialized.arrayBuffer();
    const mcpHeaders = { authorization: `Bearer ${tokens.access_token}`, 'mcp-session-id': initialized.headers.get('mcp-session-id') };
    const query = space => ({ jsonrpc: '2.0', id: 1, method: 'tools/call', params: { name: 'brief', arguments: { space } } });
    assert.equal((await f.post('/mcp', query('atlas-review'), mcpHeaders)).status, 200);
    const calls = f.backendCalls();
    assert.equal((await f.post('/mcp', query('private'), mcpHeaders)).status, 403);
    assert.equal(f.backendCalls(), calls);
    assert.equal((await f.post('/devices/revoke', {}, { authorization: `Bearer ${f.issued.password}`, 'x-wenlan-device-id': f.device.id })).status, 401);
    assert.equal((await f.post('/devices/revoke', {}, { authorization: `Bearer ${f.device.managementToken}`, 'x-wenlan-device-id': f.device.id })).status, 200);
    assert.equal((await f.post('/mcp', query('atlas-review'), mcpHeaders)).status, 403);
    // The provider revokes an issued grant on code replay. Exercise normal
    // querying/revocation first instead of accidentally revoking this token.
    assert.equal((await exchange()).status, 400, 'code cannot be replayed');
    const next = await beginSampleAuthorization(f);
    assert.equal((await f.post('/pairing/sample', { ...login, clientId: next.client.client_id }, { ...headers, cookie: next.cookie })).status, 401);
  } finally { await f.runtime.dispose(); }
});

test('missing, malformed, expired and wrong-resource sample bindings do not enable login', { timeout: 30_000 }, async () => {
  for (const configure of [() => undefined, () => '{invalid', account => JSON.stringify({ ...account, expiresAt: 1 }),
    account => JSON.stringify({ ...account, resource: 'https://other.example/mcp' })]) {
    const f = await createSampleRuntime(configure);
    try {
      const a = await beginSampleAuthorization(f);
      const page = await f.request('/pairing', { headers: { cookie: a.cookie } });
      assert.equal(page.status, 200);
      assert(!(await page.text()).includes('Connect a sample library'));
      assert.equal((await f.request('/pairing/sample', { headers: { cookie: a.cookie } })).status, 404);
      assert.equal((await f.post('/pairing/sample', {}, { origin, cookie: a.cookie })).status, 404);
    } finally { await f.runtime.dispose(); }
  }
});

test('sample login applies body limits and finite peer admission before credential processing', { timeout: 30_000 }, async () => {
  const f = await createSampleRuntime();
  try {
    assert.equal((await f.post('/pairing/sample', { password: 'x'.repeat(17_000) }, { origin })).status, 413);
    const startMinute = Math.floor(Date.now() / 60_000);
    let limited = false;
    // At most two calendar windows if this short test happens at a boundary.
    for (let index = 0; index < 21; index++) {
      const response = await f.post('/pairing/sample', {}, { origin, 'cf-connecting-ip': '198.51.100.7' });
      if (response.status === 429) {
        assert(Number(response.headers.get('retry-after')) > 0);
        assert(Number(response.headers.get('retry-after')) <= 60);
        limited = true;
        break;
      }
      assert.equal(response.status, 401);
      if (Math.floor(Date.now() / 60_000) === startMinute) assert(index < 10);
    }
    assert.equal(limited, true);
    let globallyLimited = false;
    for (let index = 0; index < 121; index++) {
      const response = await f.post('/pairing/sample', {}, { origin, 'cf-connecting-ip': `203.0.113.${index + 1}` });
      if (response.status === 429) {
        globallyLimited = true;
        assert(Number(response.headers.get('retry-after')) > 0);
        break;
      }
      assert.equal(response.status, 401);
      if (Math.floor(Date.now() / 60_000) === startMinute) assert(index < 60);
    }
    assert.equal(globallyLimited, true, 'changing peers cannot evade the global login cap');
  } finally { await f.runtime.dispose(); }
});
