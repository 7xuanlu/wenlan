// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { claimAuthorizationGrant, clientKey, listDeviceGrants, replaceClientAuthorization, revokeDeviceGrant,
  touchGrant, LAST_USED_WRITE_MS, type ClientAuthorization, type GrantReceipt } from '../src/grants.ts';
import { enrollDevice, rotateDeviceCredential } from '../src/devices.ts';
import { MemoryStore } from './fixtures/memory-store.ts';

const authorizationId = 'c'.repeat(64);
const claude = { clientName: 'Claude', redirectHost: 'claude.ai', knownClient: true };

async function managed() {
  const store = new MemoryStore();
  const candidate = { space: 'review', tunnelOrigin: 'https://demo.trycloudflare.com', backendToken: 'b'.repeat(43) };
  const fetcher = (async (_url, init) => new Headers(init?.headers).has('authorization')
    ? Response.json({ contract_version: 1, server: 'wenlan-mcp', tool_profile: 'query-only', authentication: 'bearer', space: 'review' })
    : new Response(null, { status: 401 })) as typeof fetch;
  const device = (await enrollDevice(store, candidate, { fetch: fetcher }))!;
  const permission = { subject: device.id, connectorId: device.id, space: 'review', generation: 0 };
  const first = { grantId: 'a'.repeat(16), clientId: 'client-a' };
  await replaceClientAuthorization(store, permission.subject, first.clientId, authorizationId, claude);
  assert.equal(await claimAuthorizationGrant(store, first, permission, authorizationId), true);
  const list = async (token = device.managementToken) => (await listDeviceGrants(store, device.id, token))!.items;
  const item = async (token?: string) => (await list(token)).find(entry => entry.id === first.grantId)!;
  return { store, device, permission, first, list, item };
}

test('claimed grants carry the consent display identity and no end reason while active', async () => {
  const { item } = await managed();
  const view = await item();
  assert.equal(view.status, 'active');
  assert.equal(view.clientName, 'Claude');
  assert.equal(view.redirectHost, 'claude.ai');
  assert.equal(view.knownClient, true);
  assert.equal(view.lastUsedAt, null);
  assert.equal('endReason' in view, false);
});

test('receipts written before the display fields list as null and false', async () => {
  const { store, item } = await managed();
  const [key, receipt] = [...store.data.entries()].find(([name]) => name.startsWith('oauth-grant:')) as [string, GrantReceipt];
  const old = { ...receipt } as Partial<GrantReceipt>;
  for (const field of ['clientName', 'redirectHost', 'knownClient', 'lastUsedAt', 'revokedAt'] as const) delete old[field];
  store.data.set(key, old);
  const view = await item();
  assert.equal(view.status, 'active');
  assert.deepEqual([view.clientName, view.redirectHost, view.knownClient, view.lastUsedAt], [null, null, false, null]);
});

test('consent written without display fields claims a grant with null identity', async () => {
  const store = new MemoryStore();
  await replaceClientAuthorization(store, 'subject', 'client', authorizationId);
  const consent = store.data.get(await clientKey('subject', 'client')) as ClientAuthorization;
  assert.equal('clientName' in consent, false);
  assert.equal(await claimAuthorizationGrant(store, { grantId: 'g'.repeat(16), clientId: 'client' },
    { subject: 'subject', connectorId: 'subject', space: 'review', generation: 0 }, authorizationId), true);
  const receipt = [...store.data.entries()].find(([name]) => name.startsWith('oauth-grant:'))![1] as GrantReceipt;
  assert.deepEqual([receipt.clientName, receipt.redirectHost, receipt.knownClient], [null, null, false]);
});

test('lastUsedAt is written at most once per fifteen minutes for an active grant', async () => {
  const { store, first, permission, item } = await managed();
  const start = Date.now();
  assert.equal(LAST_USED_WRITE_MS, 15 * 60 * 1000);
  assert.equal(await touchGrant(store, first, permission, start), true);
  assert.equal((await item()).lastUsedAt, start);
  assert.equal(await touchGrant(store, first, permission, start + LAST_USED_WRITE_MS - 1), false);
  assert.equal((await item()).lastUsedAt, start);
  assert.equal(await touchGrant(store, first, permission, start + LAST_USED_WRITE_MS), true);
  assert.equal((await item()).lastUsedAt, start + LAST_USED_WRITE_MS);
  assert.equal(await touchGrant(store, first, { ...permission, space: 'other' }, start + 2 * LAST_USED_WRITE_MS), false,
    'a differently bound grant is not touched');
  assert.equal(await touchGrant(store, { ...first, grantId: 'z'.repeat(16) }, permission, start + 2 * LAST_USED_WRITE_MS), false);
});

test('end reason: an explicit revoke reads revoked and is never touched again', async () => {
  const { store, device, first, permission, item } = await managed();
  await revokeDeviceGrant(store, device.id, device.managementToken, first.grantId, async () => {});
  const view = await item();
  assert.equal(view.status, 'inactive');
  assert.equal(view.endReason, 'revoked');
  assert.equal(await touchGrant(store, first, permission, Date.now() + LAST_USED_WRITE_MS), false);
});

test('end reason: a device generation change reads reset', async () => {
  const { store, device, item } = await managed();
  const rotated = (await rotateDeviceCredential(store, device.id, device.managementToken))!;
  const view = await item(rotated.managementToken);
  assert.equal(view.status, 'inactive');
  assert.equal(view.endReason, 'reset');
});

test('end reason: a newer authorization of the same client reads replaced', async () => {
  const { store, permission, first, item } = await managed();
  await replaceClientAuthorization(store, permission.subject, first.clientId, 'd'.repeat(64), claude);
  const view = await item();
  assert.equal(view.status, 'inactive');
  assert.equal(view.endReason, 'replaced');
});

test('end reason: lapsed consent or receipt reads expired', async () => {
  const { store, permission, first, item } = await managed();
  const key = await clientKey(permission.subject, first.clientId);
  await store.transaction(tx => tx.put(key, { authorizationId, expiresAt: Date.now() - 1 } satisfies ClientAuthorization));
  assert.equal((await item()).endReason, 'expired');
  await store.transaction(tx => tx.delete(key));
  assert.equal((await item()).endReason, 'expired', 'missing consent also reads expired');
  const second = await managed();
  const [grantKey, receipt] = [...second.store.data.entries()].find(([name]) => name.startsWith('oauth-grant:')) as [string, GrantReceipt];
  second.store.data.set(grantKey, { ...receipt, expiresAt: Date.now() - 1 });
  assert.equal((await second.item()).endReason, 'expired');
});

test('end reason precedence: revoke wins over a later reset', async () => {
  const { store, device, first, item } = await managed();
  await revokeDeviceGrant(store, device.id, device.managementToken, first.grantId, async () => {});
  const rotated = (await rotateDeviceCredential(store, device.id, device.managementToken))!;
  assert.equal((await item(rotated.managementToken)).endReason, 'revoked');
});
