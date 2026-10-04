// SPDX-License-Identifier: Apache-2.0
import type { PairingStore, Transaction } from './pairing.ts';

export const AUTHORITY_RECORD_LIMIT = 8192;
export const AUTHORITY_VALUE_BYTES = 32 * 1024;
export const CAPACITY_KEY = 'maintenance:capacity';
const CURSOR_KEY = 'maintenance:cursor';
const counted = (key: string) => key !== CAPACITY_KEY && key !== CURSOR_KEY;

export class AuthorityCapacityError extends Error {
  constructor() { super('Authority capacity unavailable'); }
}

/** Count application records in the same serializable transaction as their
 * writes/deletes. No SQL mirror or eventually consistent KV counter is used.
 * All production authority writes, including cleanup, must use this adapter.
 */
export function boundedStore(source: PairingStore, limit = AUTHORITY_RECORD_LIMIT): PairingStore {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > AUTHORITY_RECORD_LIMIT) throw new Error('Invalid authority limit');
  return {
    transaction: operation => source.transaction(async tx => {
      let count = await tx.get<number>(CAPACITY_KEY);
      if (count === undefined) {
        // One-time bounded adoption for existing local candidate state. Do not
        // silently approximate an oversized store or delete unknown records.
        count = 0;
        let after: string | undefined;
        while (true) {
          const entries = await tx.list({ prefix: '', limit: 128, ...(after ? { startAfter: after } : {}) });
          for (const key of entries.keys()) if (counted(key)) count++;
          if (count > limit) throw new AuthorityCapacityError();
          if (entries.size < 128) break;
          after = [...entries.keys()].at(-1)!;
        }
        await tx.put(CAPACITY_KEY, count);
      }
      if (!Number.isSafeInteger(count) || count < 0 || count > limit) throw new AuthorityCapacityError();
      const baseline = new Map<string, Promise<boolean>>();
      const track = (key: string) => {
        if (key === CAPACITY_KEY) throw new AuthorityCapacityError();
        if (counted(key) && !baseline.has(key)) baseline.set(key, tx.get(key).then(value => value !== undefined));
      };
      const bounded: Transaction = {
        get: key => tx.get(key),
        list: options => tx.list(options),
        put: async (key, value) => {
          const encoded = JSON.stringify(value);
          if (encoded === undefined || new TextEncoder().encode(encoded).byteLength > AUTHORITY_VALUE_BYTES) {
            throw new AuthorityCapacityError();
          }
          track(key);
          if (baseline.has(key)) await baseline.get(key);
          await tx.put(key, value);
        },
        delete: async key => {
          track(key);
          if (baseline.has(key)) await baseline.get(key);
          return tx.delete(key);
        },
      };
      const result = await operation(bounded);
      let next = count;
      for (const [key, existed] of baseline) {
        next += Number(await tx.get(key) !== undefined) - Number(await existed);
      }
      // Check the net transaction, so full stores can delete/replace records.
      // Throwing rolls back every write, including partial device/session pairs.
      if (!Number.isSafeInteger(next) || next < 0 || next > limit) throw new AuthorityCapacityError();
      if (next !== count) await tx.put(CAPACITY_KEY, next);
      return result;
    }),
  };
}
