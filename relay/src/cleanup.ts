// SPDX-License-Identifier: Apache-2.0
import type { PairingStore } from './pairing.ts';
import type { DeviceRecord } from './devices.ts';
import type { ConnectorRoute } from './proxy.ts';
import { authorizationRead } from './proxy.ts';
import type { SessionRecord } from './sessions.ts';
import { clientKey, type ClientAuthorization, type GrantReceipt } from './grants.ts';
import { hashSecret, randomSecret } from './secrets.ts';
import { CleanupBatchIncomplete } from './cleanup-kv.ts';
import { pendingReverseDevice } from './reverse-devices.ts';

const CURSOR_KEY = 'maintenance:cursor';
interface SweepCursor { after: string; hadRecords: boolean }
const PAGE_SIZE = 32;
const expired = (record: { expiresAt: number }, now: number) => !Number.isFinite(record.expiresAt) || record.expiresAt <= now;
const liveDevice = (record: DeviceRecord | undefined, now: number) => !!record && record.enabled
  && !record.pendingReverse && !expired(record, now);
const transient = ['pairing:', 'authorization-pairing:', 'oauth-request:', 'oauth-client:'];

/** One durable, bounded scan page and at most one external cleanup batch.
 * Permission denial commits before external IO. The cursor and retry lease
 * survive object eviction; all deletion decisions use fresh transactional state.
 */
export async function maintainAuthority(
  store: PairingStore, revokeTokens: (grantId: string, subject: string) => Promise<void>, now = Date.now(),
) {
  const pass = await store.transaction(async tx => {
    const saved = await tx.get<SweepCursor | string>(CURSOR_KEY);
    const cursor = typeof saved === 'string' ? saved : saved?.after;
    const hadRecords = typeof saved === 'string' ? !!saved : saved?.hadRecords === true;
    const entries = await tx.list<unknown>({ prefix: '', ...(cursor ? { startAfter: cursor } : {}), limit: PAGE_SIZE });
    let hasRecords = false;
    let job: { key: string; id: string; subject: string; lease: string } | undefined;
    for (const [key, value] of entries) {
      if (key === CURSOR_KEY) continue;
      if (transient.some(prefix => key.startsWith(prefix))) {
        hasRecords = true;
        if (expired(value as { expiresAt: number }, now)) await tx.delete(key);
      } else if (key.startsWith('device:')) {
        hasRecords = true;
        const device = value as DeviceRecord;
        if (!liveDevice(device, now) && !pendingReverseDevice(device, now)) {
          await tx.delete(key);
          await tx.delete(`connector:${key.slice('device:'.length)}`);
        }
      } else if (key.startsWith('connector:')) {
        hasRecords = true;
        const id = key.slice('connector:'.length);
        const device = await tx.get<DeviceRecord>(`device:${id}`);
        const route = value as ConnectorRoute;
        // Route TTL denies traffic, not credential-based offline recovery.
        if (!liveDevice(device, now) || !route.enabled || route.id !== id || route.subject !== device!.subject) await tx.delete(key);
      } else if (key.startsWith('mcp-session-owner:')) {
        hasRecords = true;
        const session = typeof value === 'string' ? await tx.get<SessionRecord>(`mcp-session:${value}`) : undefined;
        if (!session?.active || expired(session, now)) await tx.delete(key);
      } else if (key.startsWith('mcp-session:')) {
        hasRecords = true;
        const session = value as SessionRecord;
        if (!session.active || expired(session, now)) {
          const reverse = `mcp-session-owner:${await hashSecret(JSON.stringify([session.route, session.backendId]))}`;
          if (await tx.get<string>(reverse) === session.id) await tx.delete(reverse);
          await tx.delete(key);
        }
      } else if (key.startsWith('oauth-grant:')) {
        hasRecords = true;
        let receipt = value as GrantReceipt;
        const consent = await tx.get<ClientAuthorization>(await clientKey(receipt.subject, receipt.clientId));
        const currentConsent = !!consent && consent.authorizationId === receipt.authorizationId && !expired(consent, now);
        const route = await tx.get<ConnectorRoute>(`connector:${receipt.connectorId}`);
        const device = await tx.get<DeviceRecord>(`device:${receipt.connectorId}`);
        const currentDevice = liveDevice(device, now) && route?.enabled && route.subject === receipt.subject
          && route.id === receipt.connectorId && route.space === receipt.space && route.generation === receipt.generation;
        if (receipt.active && (!currentConsent || !currentDevice || expired(receipt, now))) {
          receipt = { ...receipt, active: false, cleanupPending: true };
          await tx.put(key, receipt);
        }
        // Keep replay tombstones through both the receipt and consent lifetime,
        // including after physical token deletion has succeeded.
        if (!receipt.active && receipt.cleanupPending === false && expired(receipt, now) && !currentConsent) {
          await tx.delete(key);
        } else if (!receipt.active && receipt.cleanupPending !== false && !job && (receipt.cleanupAfter ?? 0) <= now) {
          const lease = randomSecret();
          await tx.put(key, { ...receipt, cleanupPending: true, cleanupLease: lease, cleanupAfter: now + 60_000 });
          job = { key, id: receipt.id, subject: receipt.subject, lease };
        }
      }
    }
    const fullPage = entries.size === PAGE_SIZE;
    await tx.put(CURSOR_KEY, { after: fullPage ? [...entries.keys()].at(-1)! : '',
      hadRecords: fullPage && (hadRecords || hasRecords) } satisfies SweepCursor);
    return { checked: entries.size, more: fullPage || hadRecords || hasRecords, job };
  });
  let cleanupSucceeded: boolean | undefined;
  if (pass.job) {
    const job = pass.job;
    let partial = false;
    try { await authorizationRead(() => revokeTokens(job.id, job.subject)); cleanupSucceeded = true; }
    catch (error) { cleanupSucceeded = false; partial = error instanceof CleanupBatchIncomplete; }
    await store.transaction(async tx => {
      const receipt = await tx.get<GrantReceipt>(job.key);
      if (!receipt || receipt.active || receipt.cleanupLease !== job.lease || receipt.cleanupPending === false) return;
      const failures = cleanupSucceeded || partial ? 0 : Math.min(10, (receipt.cleanupFailures ?? 0) + 1);
      await tx.put(job.key, { ...receipt, cleanupPending: !cleanupSucceeded, cleanupFailures: failures,
        cleanupAfter: cleanupSucceeded ? 0 : now + Math.min(3_600_000, 60_000 * 2 ** Math.max(0, failures - 1)) });
    });
  }
  return { checked: pass.checked, more: pass.more, ...(cleanupSucceeded === undefined ? {} : { cleanupSucceeded }) };
}
