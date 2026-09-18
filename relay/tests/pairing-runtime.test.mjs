// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Reuse an explicitly selected, already installed Wrangler toolchain. Missing
// tooling is an error, not a silently skipped integration gate.
const toolchain = process.env.WENLAN_RELAY_TOOLCHAIN;
if (!toolchain) throw new Error('Set WENLAN_RELAY_TOOLCHAIN to an installed Wrangler project directory');
const requireTool = createRequire(join(resolve(toolchain), 'package.json'));
const { Miniflare } = requireTool('miniflare');
const { build } = requireTool('esbuild');
const resource = 'https://wenlan-relay.example/mcp';
const device = { id: 'd'.repeat(43), subject: 'synthetic-review-owner',
  generation: 1, credentialExpiresAt: Date.now() + 3_600_000 };
const route = { ...device, space: 'review', generation: 1, enabled: true,
  expiresAt: Date.now() + 3_600_000,
  tunnelOrigin: 'https://synthetic.trycloudflare.com', backendToken: 'b'.repeat(43) };
const intent = id => ({ authorizationId: id.repeat(43), clientId: 'synthetic-client',
  resource, scopes: ['wenlan:query'] });
const consent = { clientId: 'synthetic-client', resource, space: route.space };

test('pairing uses real SQLite Durable Object transactions and restart persistence', { timeout: 60_000 }, async t => {
  const directory = await mkdtemp(join(tmpdir(), 'wenlan-pairing-runtime-'));
  const bundle = await build({
    entryPoints: [fileURLToPath(new URL('./fixtures/pairing-worker.ts', import.meta.url))],
    bundle: true, write: false, format: 'esm', platform: 'browser', target: 'es2022',
  });
  let runtime;
  let stub;
  async function start() {
    runtime = new Miniflare({
      modules: true, script: bundle.outputFiles[0].text,
      compatibilityDate: '2026-09-08', host: '127.0.0.1', port: 0,
      durableObjects: { PAIRINGS: { className: 'FixturePairingObject', useSQLite: true } },
      durableObjectsPersist: directory,
      outboundService: () => new Response('External network disabled', { status: 503 }),
    });
    const namespace = await runtime.getDurableObjectNamespace('PAIRINGS');
    stub = namespace.get(namespace.idFromName('synthetic-authority'));
  }
  async function call(operation, ...args) {
    const response = await stub.fetch('https://fixture.invalid/', {
      method: 'POST', body: JSON.stringify({ operation, args }),
      headers: { 'content-type': 'application/json' },
    });
    assert.equal(response.status, 200, `${operation}: ${await response.clone().text()}`);
    return response.json();
  }
  try {
    await start();
    await call('seed', route);
    await t.test('pending intent survives a full runtime restart', async () => {
      const pair = await call('begin', intent('a'), resource);
      await runtime.dispose();
      runtime = undefined;
      await start();
      const view = await call('inspect', pair.pairingId);
      assert.equal(view.clientId, consent.clientId);
      assert.equal(view.expiresAt, pair.expiresAt);
      assert.equal(await call('approve', pair.pairingId, device, consent), true);
      assert.ok(await call('consume', pair.pairingId, pair.browserSecret));
      await runtime.dispose();
      runtime = undefined;
      await start();
      assert.equal(await call('consume', pair.pairingId, pair.browserSecret), null);
    });
    await t.test('parallel runtime requests approve and consume at most once', async () => {
      const pair = await call('begin', intent('b'), resource);
      const approvals = await Promise.all(Array.from({ length: 12 }, () =>
        call('approve', pair.pairingId, device, consent)));
      assert.equal(approvals.filter(Boolean).length, 1);
      const grants = await Promise.all(Array.from({ length: 12 }, () =>
        call('consume', pair.pairingId, pair.browserSecret)));
      assert.equal(grants.filter(Boolean).length, 1);
    });
    await t.test('failed transaction rolls back consumption', async () => {
      const pair = await call('begin', intent('c'), resource);
      assert.equal(await call('approve', pair.pairingId, device, consent), true);
      assert.deepEqual(await call('fail-consume', pair.pairingId, pair.browserSecret), { injected: true });
      assert.ok(await call('consume', pair.pairingId, pair.browserSecret));
      assert.equal(await call('consume', pair.pairingId, pair.browserSecret), null);
    });
    await t.test('route revocation is read from the same durable authority', async () => {
      const pair = await call('begin', intent('e'), resource);
      assert.equal(await call('approve', pair.pairingId, device, consent), true);
      await call('seed', { ...route, generation: route.generation + 1 });
      assert.equal(await call('consume', pair.pairingId, pair.browserSecret), null);
      await call('seed', route);
    });
    await t.test('cancellation survives restart and cannot be consumed', async () => {
      const pair = await call('begin', intent('f'), resource);
      assert.equal(await call('approve', pair.pairingId, device, consent), true);
      assert.equal(await call('cancel', pair.pairingId, pair.browserSecret), true);
      await runtime.dispose();
      runtime = undefined;
      await start();
      assert.equal(await call('consume', pair.pairingId, pair.browserSecret), null);
    });
    await t.test('enrolled device, rotation and revocation share durable pairing authority', async () => {
      const credential = await call('enroll', { tunnelOrigin: route.tunnelOrigin,
        backendToken: route.backendToken, space: route.space });
      assert.ok(credential);
      await runtime.dispose();
      runtime = undefined;
      await start();
      const oldIdentity = await call('authenticate', credential.id, credential.managementToken);
      assert.equal(oldIdentity.id, credential.id);
      const pair = await call('begin', intent('g'), resource);
      const rotated = await call('rotate', credential.id, credential.managementToken);
      assert.ok(rotated);
      assert.equal(await call('authenticate', credential.id, credential.managementToken), null);
      assert.equal(await call('approve', pair.pairingId, oldIdentity, consent), false);
      const identity = await call('authenticate', rotated.id, rotated.managementToken);
      assert.equal(await call('approve', pair.pairingId, identity, consent), true);
      assert.equal(await call('revoke', rotated.id, rotated.managementToken), true);
      await runtime.dispose();
      runtime = undefined;
      await start();
      assert.equal(await call('authenticate', rotated.id, rotated.managementToken), null);
      assert.equal(await call('consume', pair.pairingId, pair.browserSecret), null);
    });
  } finally {
    if (runtime) await runtime.dispose();
    await rm(directory, { recursive: true, force: true });
  }
});
