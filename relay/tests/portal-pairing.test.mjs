// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { approvePortalPairing } from './fixtures/portal-pairing.mjs';

const origin = 'https://relay.wenlan.app';
const env = { WENLAN_TEST_PUBLIC_RELAY: '1', WENLAN_PORTAL_PAIRING_ID: 'a'.repeat(64),
  WENLAN_PORTAL_CLIENT_ID: 'observed-client' };
function fixture(overrides = {}) {
  const calls = [];
  const requests = [];
  const intent = { clientId: env.WENLAN_PORTAL_CLIENT_ID, resource: `${origin}/mcp`,
    scopes: ['wenlan:query'], expiresAt: Date.now() + 300_000, ...overrides };
  return { calls, requests, args: { transport: 'reverse', origin, space: 'atlas-review',
    deviceHeaders: () => ({ authorization: 'synthetic-device' }),
    request: async (...args) => { requests.push(args); return { status: 200, text: JSON.stringify(intent) }; },
    post: async (...args) => { calls.push(args); return { status: 200 }; },
    diagnostic: () => {},
  } };
}
test('disabled unless both explicit portal values are provided', async () => {
  const { args, calls } = fixture();
  await approvePortalPairing(args, {});
  assert.equal(calls.length, 0);
  await assert.rejects(approvePortalPairing(args, { ...env, WENLAN_PORTAL_CLIENT_ID: undefined }));
  assert.equal(calls.length, 0);
});
test('exact observed intent grants only the synthetic Space with a bounded hold', async () => {
  const { args, calls } = fixture();
  let waited;
  let diagnostic;
  await approvePortalPairing({ ...args, diagnostic: message => { diagnostic = message; } }, env,
    async ms => { waited = ms; });
  assert.equal(waited, 90_000);
  assert.match(diagnostic, /window is 90 seconds$/);
  assert.equal(calls.length, 1);
  assert.deepEqual(calls[0][1], { approved: true, clientId: env.WENLAN_PORTAL_CLIENT_ID,
    resource: `${origin}/mcp`, space: 'atlas-review' });
});
test('explicit 180-second window controls the hold and diagnostic', async () => {
  const { args, calls } = fixture();
  let waited;
  let diagnostic;
  const windowEnv = { ...env, WENLAN_PORTAL_WINDOW_SECONDS: '180' };
  await approvePortalPairing({ ...args, diagnostic: message => { diagnostic = message; } }, windowEnv,
    async ms => { waited = ms; });
  assert.equal(waited, 180_000);
  assert.match(diagnostic, /window is 180 seconds$/);
  assert.equal(calls.length, 1);
});
test('unsupported window values fail before inspecting or approving the pairing', async () => {
  for (const value of ['60', '090', '181', '90 ', 90, null]) {
    const { args, calls, requests } = fixture();
    await assert.rejects(approvePortalPairing(args, {
      ...env, WENLAN_PORTAL_WINDOW_SECONDS: value,
    }), /WENLAN_PORTAL_WINDOW_SECONDS/);
    assert.equal(requests.length, 0);
    assert.equal(calls.length, 0);
  }
});
test('wrong client, resource, scope, expiry, or personal Space fails before approval', async () => {
  for (const mismatch of [{ clientId: 'other' }, { resource: 'https://other.example/mcp' },
    { scopes: ['wenlan:write'] }, { expiresAt: Date.now() + 30_000 }]) {
    const { args, calls } = fixture(mismatch);
    await assert.rejects(approvePortalPairing(args, env, async () => {}));
    assert.equal(calls.length, 0);
  }
  const { args, calls } = fixture();
  await assert.rejects(approvePortalPairing({ ...args, space: 'personal' }, env, async () => {}));
  assert.equal(calls.length, 0);
});
test('expiry within the browser callback allowance fails before approval', async () => {
  const { args, calls, requests } = fixture({ expiresAt: Date.now() + 35_000 });
  const windowEnv = { ...env, WENLAN_PORTAL_WINDOW_SECONDS: '180' };
  await assert.rejects(approvePortalPairing(args, windowEnv, async () => {
    throw new Error('wait must not run');
  }), /enough remaining lifetime/);
  assert.equal(requests.length, 1);
  assert.equal(calls.length, 0);
});
test('pairing need not outlive the OAuth tool window after browser consumption', async () => {
  const { args, calls } = fixture({ expiresAt: Date.now() + 60_000 });
  let waited;
  await approvePortalPairing(args, { ...env, WENLAN_PORTAL_WINDOW_SECONDS: '180' },
    async ms => { waited = ms; });
  assert.equal(calls.length, 1);
  assert.equal(waited, 180_000);
});
