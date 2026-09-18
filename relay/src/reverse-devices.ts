// SPDX-License-Identifier: Apache-2.0
import { validConnectorContract, verifyConnectorContract, type ConnectorContract, type ConnectorProbe } from './connector-check.ts';
import { CREDENTIAL_TTL_MS, ROUTE_TTL_MS, deviceKey, type DeviceCredential, type DeviceRecord } from './devices.ts';
import { connectorKey, type PairingStore, type Transaction } from './pairing.ts';
import { connectorBinding, tunnelOrigin, type ConnectorRoute } from './proxy.ts';
import { hashSecret, randomSecret, sameHash, validSecret } from './secrets.ts';

export const REVERSE_ENROLLMENT_TTL_MS = 5 * 60 * 1000;

/** Pending records retain a management capability, but never an OAuth route. */
export function pendingReverseDevice(device: DeviceRecord | undefined, now: number): boolean {
  const pending = device?.pendingReverse;
  return !!device && device.enabled === false && !!pending
    && Number.isFinite(device.expiresAt) && device.expiresAt > now
    && Number.isFinite(pending.expiresAt) && pending.expiresAt > now
    && validConnectorContract(pending);
}

export async function prepareReverseDevice(
  store: PairingStore, candidate: ConnectorContract, clock = Date.now,
): Promise<(DeviceCredential & { pendingUntil: number }) | null> {
  if (!validConnectorContract(candidate)) return null;
  const contract = { backendToken: candidate.backendToken, space: candidate.space };
  const id = randomSecret();
  const managementToken = randomSecret();
  const credentialHash = await hashSecret(managementToken);
  return store.transaction(async tx => {
    if (await tx.get(deviceKey(id)) || await tx.get(connectorKey(id))) return null;
    const now = clock();
    const expiresAt = now + CREDENTIAL_TTL_MS;
    const pendingUntil = now + REVERSE_ENROLLMENT_TTL_MS;
    await tx.put(deviceKey(id), { id, subject: id, credentialHash, enabled: false,
      expiresAt, revision: 0, pendingReverse: { ...contract, expiresAt: pendingUntil } } satisfies DeviceRecord);
    return { id, managementToken, expiresAt, pendingUntil };
  });
}

export interface OwnedState {
  device: DeviceRecord;
  route?: ConnectorRoute;
  contract: ConnectorContract;
}

async function ownedState(tx: Transaction, id: string, digest: string, now: number): Promise<OwnedState | null> {
  const device = await tx.get<DeviceRecord>(deviceKey(id));
  if (!device || device.id !== id || !device.subject
    || !Number.isFinite(device.expiresAt) || device.expiresAt <= now
    || !Number.isSafeInteger(device.revision) || device.revision < 0
    || typeof device.credentialHash !== 'string' || !sameHash(device.credentialHash, digest)) return null;
  const route = await tx.get<ConnectorRoute>(connectorKey(id));
  if (device.pendingReverse) {
    if (!pendingReverseDevice(device, now) || route) return null;
    return { device, contract: { backendToken: device.pendingReverse.backendToken, space: device.pendingReverse.space } };
  }
  // Expired routes can reconnect while the management credential is still live.
  if (device.enabled !== true || !route || route.enabled !== true || route.id !== id
    || route.subject !== device.subject || !Number.isSafeInteger(route.generation) || route.generation < 0
    || !validConnectorContract(route)) return null;
  const validTunnel = typeof route.tunnelOrigin === 'string' && !!tunnelOrigin(route.tunnelOrigin)
    && route.reverseConnectionId === undefined;
  const validReverse = validSecret(route.reverseConnectionId ?? '') && route.tunnelOrigin === undefined;
  if (!validTunnel && !validReverse) return null;
  return { device, route, contract: { backendToken: route.backendToken, space: route.space } };
}

/** Native upgrade admission only. This must never authorize an OAuth tool call. */
export async function authenticateReverseDevice(
  store: PairingStore, id: string, managementToken: string, clock = Date.now,
): Promise<boolean> {
  if (!validSecret(id) || !validSecret(managementToken)) return false;
  const digest = await hashSecret(managementToken);
  return store.transaction(async tx => !!await ownedState(tx, id, digest, clock()));
}

/** The adapter mints a unique connection ID and owns the socket/probe. No client
 * URL or caller-selected contract is accepted here. Verification happens before
 * activation; the final transaction rejects revocation, rotation and competing
 * connections that changed state while awaiting the probe.
 */
export async function activateReverseDevice(
  store: PairingStore, id: string, managementToken: string, connectionId: string,
  probe: ConnectorProbe, options: { clock?: () => number; connected: () => boolean },
): Promise<ConnectorRoute | null> {
  if (!validSecret(id) || !validSecret(managementToken) || !validSecret(connectionId)) return null;
  const clock = options.clock ?? Date.now;
  const before = await beginReverseActivation(store, id, managementToken, clock);
  if (!before || before.route?.reverseConnectionId === connectionId || !options.connected()) return null;
  if (!await verifyConnectorContract(before.contract, probe)) return null;
  return commitReverseActivation(store, id, managementToken, connectionId, before, options);
}

/** Internal authority operations: socket ownership may be sharded, but both
 * snapshots and the compare-and-swap remain in the authorization authority.
 */
export async function beginReverseActivation(
  store: PairingStore, id: string, managementToken: string, clock = Date.now,
): Promise<OwnedState | null> {
  if (!validSecret(id) || !validSecret(managementToken)) return null;
  const digest = await hashSecret(managementToken);
  return store.transaction(tx => ownedState(tx, id, digest, clock()));
}

export async function commitReverseActivation(
  store: PairingStore, id: string, managementToken: string, connectionId: string,
  before: OwnedState, options: { clock?: () => number; connected: () => boolean },
): Promise<ConnectorRoute | null> {
  if (!validSecret(id) || !validSecret(managementToken) || !validSecret(connectionId)
    || before.route?.reverseConnectionId === connectionId) return null;
  const clock = options.clock ?? Date.now;
  const digest = await hashSecret(managementToken);
  return store.transaction(async tx => {
    const current = await ownedState(tx, id, digest, clock());
    if (!current || !options.connected() || current.device.revision !== before.device.revision
      || current.device.subject !== before.device.subject
      || current.contract.space !== before.contract.space || current.contract.backendToken !== before.contract.backendToken
      || !!current.route !== !!before.route
      || (current.route && before.route && connectorBinding(current.route) !== connectorBinding(before.route))) return null;
    const revision = current.device.revision + 1;
    if (!Number.isSafeInteger(revision)) return null;
    const { pendingReverse: _pending, ...device } = current.device;
    const route: ConnectorRoute = { id, subject: device.subject, generation: current.route?.generation ?? 0,
      enabled: true, expiresAt: clock() + ROUTE_TTL_MS, reverseConnectionId: connectionId, ...current.contract };
    await tx.put(deviceKey(id), { ...device, enabled: true, revision });
    await tx.put(connectorKey(id), route);
    return route;
  });
}

/** Recheck channel ownership during its lifetime, not just at upgrade time. */
export async function reverseConnectionCurrent(
  store: PairingStore, id: string, connectionId: string, generation: number, clock = Date.now,
): Promise<boolean> {
  if (!validSecret(id) || !validSecret(connectionId) || !Number.isSafeInteger(generation) || generation < 0) return false;
  return store.transaction(async tx => {
    const device = await tx.get<DeviceRecord>(deviceKey(id));
    const route = await tx.get<ConnectorRoute>(connectorKey(id));
    const now = clock();
    return !!device && device.enabled === true && !device.pendingReverse && device.id === id
      && Number.isFinite(device.expiresAt) && device.expiresAt > now
      && !!route && route.enabled === true && route.id === id && route.subject === device.subject
      && route.generation === generation
      && route.tunnelOrigin === undefined && route.reverseConnectionId === connectionId
      && Number.isFinite(route.expiresAt) && route.expiresAt > now;
  });
}
