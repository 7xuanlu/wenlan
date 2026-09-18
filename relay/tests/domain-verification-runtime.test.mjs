// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

test('actual Worker serves only its configured challenge without OAuth or storage', { timeout: 30_000 }, async () => {
  if (!process.env.WENLAN_RELAY_TOOLCHAIN) throw new Error('WENLAN_RELAY_TOOLCHAIN required');
  const requireTool = createRequire(join(resolve(process.env.WENLAN_RELAY_TOOLCHAIN), 'package.json'));
  const { Miniflare } = requireTool('miniflare');
  const { build } = requireTool('esbuild');
  const bundle = await build({ entryPoints: [fileURLToPath(new URL('../src/worker.ts', import.meta.url))],
    bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022',
    external: ['cloudflare:workers'], loader: { '.png': 'binary' } });
  const origin = 'https://wenlan-relay.example';
  const path = '/.well-known/openai-apps-challenge';
  const token = 'synthetic-domain-ownership-token';
  const options = { modules: true, script: bundle.outputFiles[0].text,
    compatibilityDate: '2026-09-08', host: '127.0.0.1', port: 0,
    // Deliberately no DO/KV bindings: ownership proof needs no user state.
    bindings: { PUBLIC_ORIGIN: origin, DOMAIN_VERIFICATION_TOKEN: token },
    outboundService: () => { throw new Error('domain proof must not call another service'); } };
  const runtime = new Miniflare(options);
  const request = (url, init) => runtime.dispatchFetch(url, { credentials: 'omit', redirect: 'manual', ...init });
  try {
    const response = await request(`${origin}${path}`);
    assert.equal(response.status, 200);
    assert.equal(await response.text(), token);
    assert.match(response.headers.get('content-type'), /^text\/plain/);
    assert.equal(response.headers.get('cache-control'), 'no-store');
    assert.equal(response.headers.get('x-content-type-options'), 'nosniff');
    assert.equal(response.headers.has('set-cookie'), false);
    assert.equal(response.headers.has('www-authenticate'), false);
    assert.equal((await request(`https://other.example${path}`)).status, 400);
    const rejected = await request(`${origin}${path}`, { method: 'POST', body: 'attacker-token' });
    assert.equal(rejected.status, 405);
    assert(!((await rejected.text()).includes('attacker-token')));
    assert.equal((await request(`${origin}/mcp`)).status, 503, 'ownership endpoint does not make MCP or storage available');
    for (const configured of [undefined, '', 'two tokens', 'x\ny', 'x'.repeat(4097)]) {
      await runtime.setOptions({ ...options, bindings: { PUBLIC_ORIGIN: origin,
        ...(configured === undefined ? {} : { DOMAIN_VERIFICATION_TOKEN: configured }) } });
      const missing = await request(`${origin}${path}?token=attacker-token`);
      assert.equal(missing.status, 404);
      assert(!((await missing.text()).includes('attacker-token')));
    }
    await runtime.setOptions({ ...options, bindings: { ...options.bindings, DOMAIN_VERIFICATION_TOKEN: `${token}-replacement` } });
    assert.equal(await (await request(`${origin}${path}`)).text(), `${token}-replacement`, 'exact replacement, never a token list');
  } finally { await runtime.dispose(); }
});
