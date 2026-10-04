// SPDX-License-Identifier: Apache-2.0
import { authenticatedDeviceTransaction, type DeviceRecord } from './devices.ts';
import { approvePairing, browserPairingView, connectorKey, type DeviceIdentity, type PairingStore } from './pairing.ts';
import type { ConnectorRoute } from './proxy.ts';
import { hashSecret, randomSecret, sameHash, validSecret } from './secrets.ts';

const MAX_ACCOUNT_TTL_MS = 30 * 24 * 60 * 60 * 1000;
const usernamePattern = /^[a-z0-9][a-z0-9_-]{2,63}$/;
const digestPattern = /^[a-f0-9]{64}$/;

/** Operator-owned binding for a preprovisioned synthetic library. Never accept
 * this object from a login request or publish it in a response/config asset.
 */
export interface SampleAccount {
  username: string;
  passwordHash: string;
  deviceId: string;
  managementHash: string;
  generation: number;
  space: string;
  resource: string;
  expiresAt: number;
}

function validResource(value: unknown): value is string {
  if (typeof value !== 'string' || value.length > 2048) return false;
  try {
    const url = new URL(value);
    return url.protocol === 'https:' && url.href === `${url.origin}/mcp` && value === url.href;
  } catch { return false; }
}

export function parseSampleAccount(value: unknown): SampleAccount | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const account = value as Record<string, unknown>;
  if (Object.keys(account).sort().join(',') !== 'deviceId,expiresAt,generation,managementHash,passwordHash,resource,space,username'
    || typeof account.username !== 'string' || !usernamePattern.test(account.username)
    || typeof account.passwordHash !== 'string' || !digestPattern.test(account.passwordHash)
    || typeof account.managementHash !== 'string' || !digestPattern.test(account.managementHash)
    || typeof account.deviceId !== 'string' || !validSecret(account.deviceId)
    || typeof account.generation !== 'number' || !Number.isSafeInteger(account.generation) || account.generation < 0
    || typeof account.space !== 'string' || !account.space.trim() || account.space.length > 256
    || !validResource(account.resource)
    || typeof account.expiresAt !== 'number' || !Number.isSafeInteger(account.expiresAt) || account.expiresAt <= 0) return null;
  return { username: account.username, passwordHash: account.passwordHash,
    deviceId: account.deviceId, managementHash: account.managementHash,
    generation: account.generation, space: account.space,
    resource: account.resource, expiresAt: account.expiresAt };
}

function passwordDigest(username: string, password: string) {
  return hashSecret(`wenlan-sample-account-v1\0${username}\0${password}`);
}

/** Offline operator preparation from a fresh POST /devices response. This
 * creates configuration, not authority: generation zero and the credential
 * hash must still match the live device at every sample login. Never use a
 * rotated or scope-changed device, or infer that its data is synthetic.
 */
export async function prepareNewDeviceSampleAccount(
  enrollment: unknown,
  options: { username: string; resource: string; space: string }, clock = Date.now,
): Promise<{ account: SampleAccount; password: string } | null> {
  if (!enrollment || typeof enrollment !== 'object' || Array.isArray(enrollment)) return null;
  const record = enrollment as Record<string, unknown>;
  const { username, resource, space } = options;
  const now = clock();
  if (Object.keys(record).sort().join(',') !== 'expiresAt,id,managementToken'
    || typeof record.id !== 'string' || !validSecret(record.id)
    || typeof record.managementToken !== 'string' || !validSecret(record.managementToken)
    || typeof record.expiresAt !== 'number' || !Number.isSafeInteger(record.expiresAt) || record.expiresAt <= now
    || typeof username !== 'string' || !usernamePattern.test(username) || !validResource(resource)
    || typeof space !== 'string' || !space.trim() || space.trim() !== space || space.length > 256
    || !Number.isSafeInteger(now) || now <= 0) return null;
  const password = randomSecret();
  const account = parseSampleAccount({ username, resource, space,
    deviceId: record.id, managementHash: await hashSecret(record.managementToken), generation: 0,
    passwordHash: await passwordDigest(username, password),
    expiresAt: Math.min(record.expiresAt, now + MAX_ACCOUNT_TTL_MS) });
  return account ? { account, password } : null;
}

/** Offline/operator provisioning only; no HTTP route exposes this operation.
 * The operator must verify that this is a dedicated synthetic-data device.
 * A 256-bit random password is distinct from the management/backend secrets;
 * its hash is for this generated-secret scheme, not human-chosen passwords.
 */
export async function issueSampleAccount(
  store: PairingStore, deviceId: string, managementToken: string,
  options: { username: string; resource: string; expiresAt: number }, clock = Date.now,
): Promise<{ account: SampleAccount; password: string } | null> {
  const { username, resource, expiresAt } = options;
  if (typeof username !== 'string' || !usernamePattern.test(username) || !validResource(resource)
    || !validSecret(deviceId) || !validSecret(managementToken)
    || !Number.isSafeInteger(expiresAt) || expiresAt <= clock()
    || expiresAt > clock() + MAX_ACCOUNT_TTL_MS) return null;
  const password = randomSecret();
  const passwordHash = await passwordDigest(username, password);
  const managementHash = await hashSecret(managementToken);
  return authenticatedDeviceTransaction(store, deviceId, managementToken, async (tx, identity) => {
    const route = await tx.get<ConnectorRoute>(connectorKey(deviceId));
    if (!route || !Number.isFinite(route.expiresAt) || route.expiresAt <= clock() || expiresAt <= clock()
      || expiresAt > identity.credentialExpiresAt) return null;
    const account = parseSampleAccount({ username, passwordHash, deviceId, managementHash,
      generation: identity.generation, space: route.space, resource, expiresAt });
    return account ? { account, password } : null;
  }, clock);
}

/** Authenticates only a sample-library pairing, never device management or MCP.
 * HTTP must additionally enforce same-origin/CSRF, bounded input and rate limits.
 * Every invocation still needs a live browser pairing and explicit consent.
 */
export async function approveSamplePairing(
  store: PairingStore, configuredAccount: unknown,
  input: { username: string; password: string; pairingId: string; browserSecret: string;
    approved: boolean; clientId: string; resource: string; space: string },
  clock = Date.now,
): Promise<boolean> {
  const account = parseSampleAccount(configuredAccount);
  const attempt = { ...input };
  if (!account || account.expiresAt <= clock() || attempt.approved !== true
    || typeof attempt.username !== 'string' || !usernamePattern.test(attempt.username)
    || attempt.username !== account.username
    || typeof attempt.password !== 'string' || !digestPattern.test(attempt.password)
    || attempt.resource !== account.resource || attempt.space !== account.space) return false;
  const digest = await passwordDigest(attempt.username, attempt.password);
  if (!sameHash(digest, account.passwordHash)) return false;
  const view = await browserPairingView(store, attempt.pairingId, attempt.browserSecret, clock);
  if (!view || view.status !== 'pending' || view.clientId !== attempt.clientId) return false;
  const identity = await store.transaction(async tx => {
    const device = await tx.get<DeviceRecord>(`device:${account.deviceId}`);
    const route = await tx.get<ConnectorRoute>(connectorKey(account.deviceId));
    const now = clock();
    if (account.expiresAt <= now || !device || !route || device.enabled !== true || route.enabled !== true
      || device.id !== account.deviceId || route.id !== account.deviceId || device.subject !== route.subject
      || !device.subject || typeof device.credentialHash !== 'string'
      || !Number.isSafeInteger(device.revision) || device.revision < 0
      || !sameHash(device.credentialHash, account.managementHash)
      || !Number.isFinite(device.expiresAt) || device.expiresAt <= now
      || !Number.isFinite(route.expiresAt) || route.expiresAt <= now
      || route.generation !== account.generation || route.space !== account.space) return null;
    return { id: device.id, subject: device.subject, generation: route.generation,
      credentialExpiresAt: Math.min(device.expiresAt, account.expiresAt) } satisfies DeviceIdentity;
  });
  // Existing approval rechecks route generation, scope, revocation and expiry
  // in its write transaction, including changes after the credential read.
  return identity ? approvePairing(store, attempt.pairingId, identity, {
    clientId: attempt.clientId, resource: attempt.resource, space: attempt.space,
  }, clock) : false;
}
