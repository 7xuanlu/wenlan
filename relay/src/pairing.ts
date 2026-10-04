// SPDX-License-Identifier: Apache-2.0
import type { ConnectorRoute, QueryGrant } from './proxy.ts';
import { validSecret as validId, randomSecret, hashSecret as hash, sameHash } from './secrets.ts';

/** Production adapters must provide serializable, durable transactions.
 * Cloudflare KV is not an implementation of this contract.
 */
export interface Transaction {
  get<T>(key: string): Promise<T | undefined>;
  put<T>(key: string, value: T): Promise<void>;
  delete(key: string): Promise<boolean>;
  list<T>(options: { prefix: string; startAfter?: string; limit: number }): Promise<Map<string, T>>;
}
export interface PairingStore {
  transaction<T>(operation: (tx: Transaction) => Promise<T>): Promise<T>;
}

/** Trusted OAuth adapter output after client/redirect/resource/PKCE validation.
 * authorizationId identifies the immutable server-stored OAuth request.
 * Never construct this by casting an incoming JSON body.
 */
export interface AuthorizationIntent {
  authorizationId: string;
  clientId: string;
  resource: string;
  scopes: readonly string[];
}

/** Trusted device-auth adapter output, not a client-supplied identity. */
export interface DeviceIdentity {
  id: string;
  subject: string;
  generation: number;
  credentialExpiresAt: number;
}

interface Approval {
  subject: string;
  connectorId: string;
  space: string;
  generation: number;
}
interface PairingRecord extends AuthorizationIntent {
  browserHash: string;
  expiresAt: number;
  status: 'pending' | 'approved' | 'consumed' | 'cancelled';
  approval?: Approval;
}

export const PAIRING_TTL_MS = 5 * 60 * 1000;
const pairingKey = (id: string) => `pairing:${id}`;
export const connectorKey = (id: string) => `connector:${id}`;

function live(record: PairingRecord | undefined, now: number): record is PairingRecord {
  return !!record && Number.isFinite(record.expiresAt) && record.expiresAt > now
    && (record.status === 'pending' || record.status === 'approved');
}
function matchingRoute(route: ConnectorRoute | undefined, approval: Approval, now: number): boolean {
  return !!route && route.enabled === true && Number.isFinite(route.expiresAt) && route.expiresAt > now
    && route.id === approval.connectorId && route.subject === approval.subject
    && route.space === approval.space && route.generation === approval.generation;
}

export async function beginPairing(
  store: PairingStore, intent: AuthorizationIntent, publicResource: string, now = Date.now(),
): Promise<{ pairingId: string; browserSecret: string; expiresAt: number }> {
  if (!validId(intent.authorizationId) || !intent.clientId || intent.clientId.length > 2048
    || intent.resource !== publicResource || !publicResource.startsWith('https://')
    || intent.scopes.length !== 1 || intent.scopes[0] !== 'wenlan:query') {
    throw new Error('Unsupported authorization intent');
  }
  const pairingId = randomSecret();
  const browserSecret = randomSecret();
  const browserHash = await hash(browserSecret);
  const expiresAt = now + PAIRING_TTL_MS;
  await store.transaction(async tx => {
    const authorizationKey = `authorization-pairing:${intent.authorizationId}`;
    if (await tx.get(authorizationKey)) throw new Error('Authorization already paired');
    await tx.put(pairingKey(pairingId), {
      authorizationId: intent.authorizationId, clientId: intent.clientId,
      resource: intent.resource, scopes: [...intent.scopes], browserHash,
      expiresAt, status: 'pending',
    } satisfies PairingRecord);
    await tx.put(authorizationKey, { pairingId, expiresAt });
  });
  // Transport adapter must keep browserSecret in an HttpOnly/Secure cookie,
  // never in the pairing code, URL, model context or desktop response.
  return { pairingId, browserSecret, expiresAt };
}

export async function inspectPairing(store: PairingStore, pairingId: string, now = Date.now()) {
  if (!validId(pairingId)) return null;
  return store.transaction(async tx => {
    const record = await tx.get<PairingRecord>(pairingKey(pairingId));
    if (!live(record, now) || record.status !== 'pending') return null;
    return { pairingId, clientId: record.clientId, resource: record.resource,
      scopes: [...record.scopes], expiresAt: record.expiresAt };
  });
}

/** Browser-only status, authenticated by the HttpOnly cookie secret. */
export async function browserPairingView(store: PairingStore, pairingId: string, browserSecret: string, clock = Date.now) {
  if (!validId(pairingId) || !validId(browserSecret)) return null;
  const browserHash = await hash(browserSecret);
  return store.transaction(async tx => {
    const record = await tx.get<PairingRecord>(pairingKey(pairingId));
    if (!live(record, clock()) || !sameHash(record.browserHash, browserHash)) return null;
    return { pairingId, clientId: record.clientId, status: record.status,
      ...(record.status === 'approved' && record.approval ? { space: record.approval.space } : {}) };
  });
}

/** Called only after trusted identity authentication AND explicit consent. */
export async function approvePairing(
  store: PairingStore, pairingId: string, device: DeviceIdentity,
  consent: { clientId: string; resource: string; space: string }, clock = Date.now,
): Promise<boolean> {
  if (!validId(pairingId) || !validId(device.id)) return false;
  return store.transaction(async tx => {
    const record = await tx.get<PairingRecord>(pairingKey(pairingId));
    if (!live(record, clock()) || record.status !== 'pending'
      || record.clientId !== consent.clientId || record.resource !== consent.resource) return false;
    const route = await tx.get<ConnectorRoute>(connectorKey(device.id));
    const now = clock();
    if (!live(record, now) || !route || route.enabled !== true
      || !Number.isFinite(route.expiresAt) || route.expiresAt <= now
      || !Number.isFinite(device.credentialExpiresAt) || device.credentialExpiresAt <= now
      || route.id !== device.id || route.subject !== device.subject
      || route.generation !== device.generation
      || route.space !== consent.space || !route.space.trim()
      || !Number.isSafeInteger(route.generation) || route.generation < 0) return false;
    await tx.put(pairingKey(pairingId), { ...record, status: 'approved', approval: {
      subject: route.subject, connectorId: route.id, space: route.space,
      generation: route.generation,
    } } satisfies PairingRecord);
    return true;
  });
}

export interface ConsumedPairing {
  authorizationId: string;
  clientId: string;
  resource: string;
  // The OAuth adapter supplies verified token expiry separately on each call.
  grant: Omit<QueryGrant, 'expiresAt'>;
}

/** Consumes once before calling the OAuth library. A later issuance failure
 * requires a fresh pairing; never replay completeAuthorization after ambiguity.
 */
export async function consumePairing(
  store: PairingStore, pairingId: string, browserSecret: string, clock = Date.now,
): Promise<ConsumedPairing | null> {
  if (!validId(pairingId) || !validId(browserSecret)) return null;
  const browserHash = await hash(browserSecret);
  return store.transaction(async tx => {
    const record = await tx.get<PairingRecord>(pairingKey(pairingId));
    if (!live(record, clock()) || record.status !== 'approved' || !record.approval
      || !sameHash(record.browserHash, browserHash)) return null;
    const route = await tx.get<ConnectorRoute>(connectorKey(record.approval.connectorId));
    const now = clock();
    if (!live(record, now) || !matchingRoute(route, record.approval, now)) return null;
    await tx.put(pairingKey(pairingId), { ...record, status: 'consumed' } satisfies PairingRecord);
    return {
      authorizationId: record.authorizationId, clientId: record.clientId, resource: record.resource,
      grant: { ...record.approval, scopes: [...record.scopes] },
    };
  });
}

export async function cancelPairing(
  store: PairingStore, pairingId: string, browserSecret: string, now = Date.now(),
): Promise<boolean> {
  if (!validId(pairingId) || !validId(browserSecret)) return false;
  const browserHash = await hash(browserSecret);
  return store.transaction(async tx => {
    const record = await tx.get<PairingRecord>(pairingKey(pairingId));
    if (!live(record, now) || !sameHash(record.browserHash, browserHash)) return false;
    await tx.put(pairingKey(pairingId), { ...record, status: 'cancelled' } satisfies PairingRecord);
    return true;
  });
}
