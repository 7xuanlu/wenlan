// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { renewSampleRoute } from '../scripts/renew-sample-route.mjs';
import { prepareNewDeviceSampleAccount } from '../src/sample-account.ts';

async function input() {
  const enrollment = { id: 'd'.repeat(64), managementToken: 'm'.repeat(64), expiresAt: 1_000_000 };
  const relayOrigin = 'https://wenlan-relay.example';
  const connector = { tunnelOrigin: 'https://original.trycloudflare.com', backendToken: 'b'.repeat(64), space: 'review' };
  const prepared = await prepareNewDeviceSampleAccount(enrollment,
    { username: 'reviewer', resource: `${relayOrigin}/mcp`, space: connector.space }, () => 1000);
  return { enrollment, connector, account: prepared.account, relayOrigin,
    nextTunnelOrigin: 'https://replacement.trycloudflare.com' };
}

test('one-shot renewal sends only the existing boundary to the exact relay without cookies or redirects', async () => {
  const value = await input();
  const before = structuredClone(value);
  let calls = 0;
  assert.equal(await renewSampleRoute(value, { clock: () => 1000, fetcher: async (url, init) => {
    calls++;
    assert.equal(url, `${value.relayOrigin}/devices/renew`);
    assert.equal(init.method, 'POST');
    assert.equal(init.redirect, 'manual');
    assert.equal(init.credentials, 'omit');
    assert.deepEqual(init.headers, { 'content-type': 'application/json', accept: 'application/json',
      authorization: `Bearer ${value.enrollment.managementToken}`, 'x-wenlan-device-id': value.enrollment.id });
    assert.deepEqual(JSON.parse(init.body), { tunnelOrigin: value.nextTunnelOrigin,
      backendToken: value.connector.backendToken, space: 'review', expectedGeneration: 0 });
    return Response.json({ success: true });
  } }), 'renewed');
  assert.equal(calls, 1);
  assert.deepEqual(value, before);
});

test('invalid credentials, scope, resource, generation and tunnel never send secrets', async () => {
  const original = await input();
  const changes = [
    { enrollment: { ...original.enrollment, id: 'x'.repeat(64) } },
    { enrollment: { ...original.enrollment, managementToken: 'x'.repeat(64) } },
    { enrollment: { ...original.enrollment, expiresAt: 1000 } },
    { account: { ...original.account, expiresAt: 1000 } },
    { account: { ...original.account, generation: 1 } },
    { connector: { ...original.connector, space: 'private' } },
    { connector: { ...original.connector, backendToken: 'bad' } },
    { relayOrigin: 'https://attacker.example' },
    { relayOrigin: `${original.relayOrigin}/` },
    ...['http://replacement.trycloudflare.com', 'https://a.b.trycloudflare.com',
      'https://replacement.trycloudflare.com/mcp', 'https://replacement.trycloudflare.com?token=secret',
      'https://user:secret@replacement.trycloudflare.com', 'https://example.com',
      'https://replacement.trycloudflare.com:443', 'https://Replacement.trycloudflare.com']
      .map(nextTunnelOrigin => ({ nextTunnelOrigin })),
  ];
  let calls = 0;
  for (const change of changes) assert.equal(await renewSampleRoute({ ...original, ...change },
    { clock: () => 1000, fetcher: async () => { calls++; throw new Error('must not send'); } }), 'invalid');
  assert.equal(calls, 0);
});

test('redirects, denial, rate limiting and uncertain responses are not success or automatic retries', async () => {
  const value = await input();
  for (const status of [301, 307, 400, 401, 403, 404, 429, 500, 503]) {
    let calls = 0;
    assert.equal(await renewSampleRoute(value, { clock: () => 1000, fetcher: async () => {
      calls++; return new Response('secret', { status, headers: { location: 'https://attacker.example', 'retry-after': '1' } });
    } }), 'rejected');
    assert.equal(calls, 1);
  }
  for (const response of [Response.json({ success: false }), Response.json({ success: true, secret: 'sensitive' }),
    Response.json({ secret: 'x'.repeat(4096) }), new Response('{broken', { headers: { 'content-type': 'application/json' } }),
    new Response('success'), new Response(null, { headers: { 'content-type': 'application/json' } })]) {
    assert.equal(await renewSampleRoute(value, { clock: () => 1000, fetcher: async () => response }), 'uncertain');
  }
  assert.equal(await renewSampleRoute(value, { clock: () => 1000,
    fetcher: async () => { throw new Error('secret network detail'); } }), 'uncertain');
});

test('response deadline cancels a stalled stream and does not claim renewal', async () => {
  const value = await input();
  const controller = new AbortController();
  let cancelled = false;
  const timer = setTimeout(() => controller.abort(), 20);
  try {
    assert.equal(await renewSampleRoute(value, { clock: () => 1000, signal: controller.signal,
      fetcher: async () => new Response(new ReadableStream({ cancel() { cancelled = true; } }),
        { headers: { 'content-type': 'application/json' } }) }), 'uncertain');
    assert.equal(cancelled, true);
  } finally { clearTimeout(timer); }
});

test('CLI rejects private malformed input without leaking content or parser errors', async () => {
  const root = await mkdtemp(join(tmpdir(), 'wenlan-renewal-cli-'));
  try {
    const secret = 'synthetic-private-sentinel-not-for-output';
    const file = join(root, 'enrollment.json');
    await writeFile(file, `{"secret":"${secret}`, { mode: 0o600 });
    await assert.rejects(promisify(execFile)(process.execPath,
      [fileURLToPath(new URL('../scripts/renew-sample-route.mjs', import.meta.url)), '--enrollment', file],
      { timeout: 5000, env: { ...process.env, WENLAN_NO_AUTOSTART: '1' } }), error => {
      assert.equal(error.code, 1);
      assert.equal(error.stdout, '');
      assert.match(error.stderr, /^Renewal not confirmed\./);
      for (const forbidden of [secret, file, 'SyntaxError', 'JSON.parse']) assert(!error.stderr.includes(forbidden));
      return true;
    });
  } finally { await rm(root, { recursive: true, force: true }); }
});
