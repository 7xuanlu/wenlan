// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';

// Attended synthetic-data proof only. The caller owns revocation and cleanup.
export async function approvePortalPairing({ transport, origin, space, deviceHeaders,
  request, post, diagnostic }, env = process.env, wait = delay) {
  const pairingId = env.WENLAN_PORTAL_PAIRING_ID;
  const clientId = env.WENLAN_PORTAL_CLIENT_ID;
  if (pairingId === undefined && clientId === undefined) return;
  const windowSeconds = env.WENLAN_PORTAL_WINDOW_SECONDS === undefined
    ? '90' : env.WENLAN_PORTAL_WINDOW_SECONDS;
  assert(windowSeconds === '90' || windowSeconds === '180',
    'WENLAN_PORTAL_WINDOW_SECONDS must be exactly 90 or 180');
  const windowMs = Number(windowSeconds) * 1000;
  assert.equal(env.WENLAN_TEST_PUBLIC_RELAY, '1');
  assert.equal(transport, 'reverse');
  assert.equal(origin, 'https://relay.wenlan.app');
  assert.equal(space, 'atlas-review');
  assert.match(pairingId ?? '', /^[a-f0-9]{64}$/);
  assert(typeof clientId === 'string' && clientId.length > 0 && clientId.length <= 2048);
  const response = await request(`${origin}/pairings/${pairingId}`, { headers: deviceHeaders() });
  assert.equal(response.status, 200, 'portal pairing inspection must succeed');
  const intent = JSON.parse(response.text);
  assert.equal(intent.clientId, clientId, 'portal client must match the observed consent page');
  assert.equal(intent.resource, `${origin}/mcp`);
  assert.deepEqual(intent.scopes, ['wenlan:query']);
  // Pairing is consumed by the browser callback; OAuth tokens govern the later
  // tool window. Do not require the one-time code to outlive that whole window.
  assert(Number.isFinite(intent.expiresAt) && intent.expiresAt > Date.now() + 35_000,
    'portal pairing needs enough remaining lifetime for the browser callback');
  const approval = await post(`/pairings/${pairingId}/approve`, {
    approved: true, clientId, resource: `${origin}/mcp`, space,
  }, deviceHeaders());
  assert.equal(approval.status, 200, 'synthetic portal approval must succeed');
  diagnostic(`Synthetic portal pairing approved; attended scan window is ${windowSeconds} seconds`);
  await wait(windowMs);
}
