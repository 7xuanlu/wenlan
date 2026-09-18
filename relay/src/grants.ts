// SPDX-License-Identifier: Apache-2.0
import type { PairingStore, Transaction } from './pairing.ts';
import type { QueryGrant } from './proxy.ts';
import type { SessionIdentity } from './sessions.ts';
import { hashSecret } from './secrets.ts';
import { authenticatedDeviceTransaction } from './devices.ts';

export interface GrantReceipt {
  id: string; clientId: string; subject: string; connectorId: string; space: string;
  generation: number; owner: string; active: boolean; createdAt: number; expiresAt: number;
  cleanupPending?: boolean;
  cleanupAfter?: number;
  cleanupFailures?: number;
  cleanupLease?: string;
  authorizationId: string;
}
export interface ClientAuthorization { authorizationId: string; expiresAt: number }
export interface GrantView {
  id: string; clientId: string; space: string; createdAt: number; expiresAt: number;
  status: 'active' | 'inactive'; cleanupPending: boolean;
}
const MAX_GRANT_MS = 30 * 24 * 60 * 60 * 1000;
export const validGrantId = (value: string) => /^[A-Za-z0-9_-]{16,128}$/.test(value);
const key = (subject: string, id: string) => `oauth-grant:${subject}:${id}`;
export const clientKey = async (subject: string, clientId: string) => `oauth-client:${subject}:${await hashSecret(clientId)}`;

/** Called only after consuming a valid device-approved pairing. Reauthorization
 * replaces prior consent for this client before eventual KV revocation runs.
 */
export async function replaceClientAuthorization(store: PairingStore, subject: string, clientId: string, authorizationId: string) {
  const key = await clientKey(subject, clientId);
  await store.transaction(tx => tx.put(key, { authorizationId, expiresAt: Date.now() + MAX_GRANT_MS } satisfies ClientAuthorization));
}

async function currentAuthorization(tx: Transaction, subject: string, clientId: string, authorizationId: string) {
  const current = await tx.get<ClientAuthorization>(await clientKey(subject, clientId));
  return !!current && current.authorizationId === authorizationId
    && Number.isFinite(current.expiresAt) && current.expiresAt > Date.now();
}
async function binding(identity: SessionIdentity, grant: Pick<QueryGrant, 'subject' | 'connectorId' | 'space' | 'generation'>) {
  return { key: key(grant.subject, identity.grantId),
    owner: await hashSecret(JSON.stringify([identity.clientId, grant.connectorId, grant.space, grant.generation])) };
}

/** The provider validates client, PKCE and resource before invoking this claim.
 * KV read/modify/write alone cannot make authorization-code consumption atomic.
 */
export async function claimAuthorizationGrant(
  store: PairingStore, identity: SessionIdentity,
  grant: Pick<QueryGrant, 'subject' | 'connectorId' | 'space' | 'generation'>,
  authorizationId: string,
): Promise<boolean> {
  const bound = await binding(identity, grant);
  return store.transaction(async tx => {
    if (!await currentAuthorization(tx, grant.subject, identity.clientId, authorizationId)) return false;
    const existing = await tx.get<GrantReceipt>(bound.key);
    if (existing) {
      await tx.put(bound.key, { ...existing, active: false });
      return false;
    }
    const now = Date.now();
    await tx.put(bound.key, { subject: grant.subject, connectorId: grant.connectorId, space: grant.space,
      generation: grant.generation, id: identity.grantId, clientId: identity.clientId,
      authorizationId,
      owner: bound.owner, active: true, createdAt: now, expiresAt: now + MAX_GRANT_MS } satisfies GrantReceipt);
    return true;
  });
}

export async function listDeviceGrants(store: PairingStore, deviceId: string, credential: string, cursor?: string) {
  if (cursor !== undefined && !validGrantId(cursor)) return null;
  return authenticatedDeviceTransaction(store, deviceId, credential, async (tx, device) => {
    const prefix = `oauth-grant:${device.subject}:`;
    const records = await tx.list<GrantReceipt>({ prefix,
      ...(cursor ? { startAfter: key(device.subject, cursor) } : {}), limit: 26 });
    const page = [...records.values()].slice(0, 25);
    const now = Date.now();
    const items: GrantView[] = [];
    for (const record of page) {
      if (record.subject !== device.subject || record.connectorId !== device.id) continue;
      const active = record.active && record.generation === device.generation && record.expiresAt > now
        && await currentAuthorization(tx, record.subject, record.clientId, record.authorizationId);
      items.push({ id: record.id, clientId: record.clientId, space: record.space,
        createdAt: record.createdAt, expiresAt: record.expiresAt, cleanupPending: record.cleanupPending === true,
        status: active ? 'active' : 'inactive' });
    }
    return { items, ...(records.size > 25 ? { cursor: page.at(-1)!.id } : {}) };
  });
}

/** Authoritative denial is committed before any eventually consistent token
 * deletion. A failed cleanup is recorded and can be retried idempotently.
 */
export async function revokeDeviceGrant(
  store: PairingStore, deviceId: string, credential: string, grantId: string,
  revokeTokens: (grantId: string, subject: string) => Promise<void>,
): Promise<{ revoked: true; cleanupPending: boolean } | null> {
  if (!validGrantId(grantId)) return null;
  const subject = await authenticatedDeviceTransaction(store, deviceId, credential, async (tx, device) => {
    const receipt = await tx.get<GrantReceipt>(key(device.subject, grantId));
    if (!receipt || receipt.subject !== device.subject || receipt.connectorId !== device.id) return null;
    await tx.put(key(device.subject, grantId), { ...receipt, active: false, cleanupPending: true });
    return device.subject;
  });
  if (!subject) return null;
  try {
    await revokeTokens(grantId, subject);
    await store.transaction(async tx => {
      const receipt = await tx.get<GrantReceipt>(key(subject, grantId));
      if (receipt && !receipt.active) await tx.put(key(subject, grantId), { ...receipt, cleanupPending: false });
    });
    return { revoked: true, cleanupPending: false };
  } catch { return { revoked: true, cleanupPending: true }; }
}

export async function authorizationGrantActive(
  store: PairingStore, identity: SessionIdentity,
  grant: Pick<QueryGrant, 'subject' | 'connectorId' | 'space' | 'generation'>,
): Promise<boolean> {
  const bound = await binding(identity, grant);
  return store.transaction(async tx => {
    const receipt = await tx.get<GrantReceipt>(bound.key);
    return !!receipt && receipt.active && receipt.owner === bound.owner
      && Number.isFinite(receipt.expiresAt) && receipt.expiresAt > Date.now()
      && await currentAuthorization(tx, receipt.subject, receipt.clientId, receipt.authorizationId);
  });
}
