// SPDX-License-Identifier: Apache-2.0
import { verifyConnector, type ConnectorCandidate } from './connector-check.ts';
import { connectorKey, type DeviceIdentity, type PairingStore, type Transaction } from './pairing.ts';
import type { ConnectorRoute } from './proxy.ts';
import { hashSecret, randomSecret, sameHash, validSecret } from './secrets.ts';

export const ROUTE_TTL_MS = 24 * 60 * 60 * 1000;
export const CREDENTIAL_TTL_MS = 30 * ROUTE_TTL_MS;
export const deviceKey = (id: string) => `device:${id}`;
export interface DeviceRecord {
  id: string;
  subject: string;
  credentialHash: string;
  enabled: boolean;
  expiresAt: number;
  revision: number;
  pendingReverse?: { backendToken: string; space: string; expiresAt: number };
}
interface DeviceOptions {
  fetch?: typeof globalThis.fetch;
  clock?: () => number;
}
export interface DeviceCredential {
  id: string;
  managementToken: string;
  expiresAt: number;
}

async function owned(
  tx: Transaction, id: string, credentialHash: string, clock: () => number,
): Promise<{ device: DeviceRecord; route: ConnectorRoute } | null> {
  const device = await tx.get<DeviceRecord>(deviceKey(id));
  const route = await tx.get<ConnectorRoute>(connectorKey(id));
  const now = clock();
  if (!device || !route || device.enabled !== true || device.pendingReverse || route.enabled !== true
    || device.id !== id || route.id !== id || device.subject !== route.subject
    || !device.subject || !Number.isFinite(device.expiresAt) || device.expiresAt <= now
    || !Number.isSafeInteger(device.revision) || device.revision < 0
    || !Number.isSafeInteger(route.generation) || route.generation < 0
    || typeof device.credentialHash !== 'string'
    || !sameHash(device.credentialHash, credentialHash)) return null;
  // An expired tunnel route may still be renewed by a live management credential.
  return { device, route };
}

/** Internal enrollment core, not an HTTP handler. The public adapter must add
 * bounded parsing, rate limits and abuse controls before calling this function.
 * Subject denotes this enrolled device owner, not a verified email/cloud user.
 */
export async function enrollDevice(
  store: PairingStore, candidate: ConnectorCandidate, options: DeviceOptions = {},
): Promise<DeviceCredential | null> {
  const snapshot = { tunnelOrigin: candidate.tunnelOrigin,
    backendToken: candidate.backendToken, space: candidate.space };
  if (!await verifyConnector(snapshot, options.fetch)) return null;
  const id = randomSecret();
  const managementToken = randomSecret();
  const credentialHash = await hashSecret(managementToken);
  const clock = options.clock ?? Date.now;
  return store.transaction(async tx => {
    if (await tx.get(deviceKey(id)) || await tx.get(connectorKey(id))) return null;
    const now = clock();
    const expiresAt = now + CREDENTIAL_TTL_MS;
    await tx.put(deviceKey(id), { id, subject: id, credentialHash,
      enabled: true, revision: 0, expiresAt } satisfies DeviceRecord);
    await tx.put(connectorKey(id), { ...snapshot, id, subject: id, generation: 0,
      enabled: true, expiresAt: now + ROUTE_TTL_MS } satisfies ConnectorRoute);
    return { id, managementToken, expiresAt };
  });
}

/** Returned identity may feed inspect/approve only inside the trusted adapter.
 * Refresh/revoke reauthenticate within their own write transaction.
 */
export async function authenticateDevice(
  store: PairingStore, id: string, managementToken: string, clock = Date.now,
): Promise<DeviceIdentity | null> {
  return authenticatedDeviceTransaction(store, id, managementToken, async (_tx, identity) => identity, clock);
}

/** Recheck the management credential in the same transaction as a scoped read
 * or mutation, so a credential rotation cannot race a previously verified ID.
 */
export async function authenticatedDeviceTransaction<T>(
  store: PairingStore, id: string, managementToken: string,
  operation: (tx: Transaction, identity: DeviceIdentity) => Promise<T>, clock = Date.now,
): Promise<T | null> {
  if (!validSecret(id) || !validSecret(managementToken)) return null;
  const digest = await hashSecret(managementToken);
  return store.transaction(async tx => {
    const state = await owned(tx, id, digest, clock);
    return state ? operation(tx, { id, subject: state.device.subject,
      generation: state.route.generation, credentialExpiresAt: state.device.expiresAt }) : null;
  });
}

export async function refreshDevice(
  store: PairingStore, id: string, managementToken: string,
  candidate: ConnectorCandidate, options: DeviceOptions & { expectedGeneration?: number } = {},
): Promise<boolean> {
  if (!validSecret(id) || !validSecret(managementToken)) return false;
  const expectedGeneration = options.expectedGeneration;
  if (expectedGeneration !== undefined && (!Number.isSafeInteger(expectedGeneration) || expectedGeneration < 0)) return false;
  const digest = await hashSecret(managementToken);
  const clock = options.clock ?? Date.now;
  const before = await store.transaction(tx => owned(tx, id, digest, clock));
  if (!before) return false;
  // Reverse routes renew through a verified socket activation, never a tunnel update.
  if (before.route.reverseConnectionId !== undefined) return false;
  const snapshot = { tunnelOrigin: candidate.tunnelOrigin,
    backendToken: candidate.backendToken, space: candidate.space };
  // Optional compare-and-renew mode never changes the consent boundary.
  const matches = (route: ConnectorRoute) => expectedGeneration === undefined
    || (route.generation === expectedGeneration && route.space === snapshot.space
      && route.backendToken === snapshot.backendToken);
  if (!matches(before.route)) return false;
  // Never probe an unauthenticated update, or hold a transaction over fetch.
  if (!await verifyConnector(snapshot, options.fetch)) return false;
  return store.transaction(async tx => {
    const current = await owned(tx, id, digest, clock);
    if (!current || current.device.revision !== before.device.revision || !matches(current.route)) return false;
    const boundaryChanged = current.route.space !== snapshot.space
      || current.route.backendToken !== snapshot.backendToken;
    const generation = current.route.generation + Number(boundaryChanged);
    const revision = current.device.revision + 1;
    if (!Number.isSafeInteger(generation) || !Number.isSafeInteger(revision)) return false;
    await tx.put(deviceKey(id), { ...current.device, revision });
    await tx.put(connectorKey(id), { ...current.route, ...snapshot, generation,
      expiresAt: clock() + ROUTE_TTL_MS } satisfies ConnectorRoute);
    return true;
  });
}

export async function revokeDevice(
  store: PairingStore, id: string, managementToken: string, _clock = Date.now,
): Promise<boolean> {
  if (!validSecret(id) || !validSecret(managementToken)) return false;
  const digest = await hashSecret(managementToken);
  return store.transaction(async tx => {
    const device = await tx.get<DeviceRecord>(deviceKey(id));
    const route = await tx.get<ConnectorRoute>(connectorKey(id));
    // A retry can arrive after maintenance has removed both records. IDs are
    // server-generated and refresh cannot recreate them: absence is terminal
    // denial, not proof of identity or permission to perform another operation.
    if (!device && !route) return true;
    if (!device || device.id !== id || !device.subject
      || typeof device.credentialHash !== 'string' || !sameHash(device.credentialHash, digest)
      || (route && (route.id !== id || route.subject !== device.subject))) return false;
    // Expired or already-disabled credentials may only attenuate access. Keep
    // checking the current hash so a rotated credential cannot revoke its heir.
    if (device.enabled || device.pendingReverse) {
      const { pendingReverse: _pending, ...revoked } = device;
      await tx.put(deviceKey(id), { ...revoked, enabled: false });
    }
    if (route?.enabled) await tx.put(connectorKey(id), { ...route, enabled: false });
    return true;
  });
}

/** Rotation invalidates the old management credential and all previous OAuth
 * approvals. Lost rotation responses require re-enrollment; never store tokens
 * in plaintext to make a blind retry succeed.
 */
export async function rotateDeviceCredential(
  store: PairingStore, id: string, managementToken: string, clock = Date.now,
): Promise<DeviceCredential | null> {
  if (!validSecret(id) || !validSecret(managementToken)) return null;
  const digest = await hashSecret(managementToken);
  const nextToken = randomSecret();
  const credentialHash = await hashSecret(nextToken);
  return store.transaction(async tx => {
    const current = await owned(tx, id, digest, clock);
    if (!current) return null;
    const generation = current.route.generation + 1;
    const revision = current.device.revision + 1;
    if (!Number.isSafeInteger(generation) || !Number.isSafeInteger(revision)) return null;
    const expiresAt = clock() + CREDENTIAL_TTL_MS;
    await tx.put(deviceKey(id), { ...current.device, credentialHash, revision, expiresAt });
    await tx.put(connectorKey(id), { ...current.route, generation });
    return { id, managementToken: nextToken, expiresAt };
  });
}
