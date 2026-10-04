// SPDX-License-Identifier: Apache-2.0
import type { PairingStore, Transaction } from '../../src/pairing.ts';

// Test double: serialize and rollback transactions; not a production store.
export class MemoryStore implements PairingStore {
  data = new Map<string, unknown>();
  queue: Promise<unknown> = Promise.resolve();
  transaction<T>(operation: (tx: Transaction) => Promise<T>): Promise<T> {
    const result = this.queue.then(async () => {
      const next = structuredClone(this.data);
      const value = await operation({
        get: async <U>(key: string) => structuredClone(next.get(key)) as U | undefined,
        put: async <U>(key: string, value: U) => { next.set(key, structuredClone(value)); },
        delete: async (key: string) => next.delete(key),
        list: async <U>(options: { prefix: string; startAfter?: string; limit: number }) => new Map(
          [...next.entries()].filter(([key]) => key.startsWith(options.prefix) && (!options.startAfter || key > options.startAfter))
            .sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).slice(0, options.limit)
            .map(([key, value]) => [key, structuredClone(value) as U])),
      });
      this.data = next;
      return value;
    });
    this.queue = result.catch(() => {});
    return result;
  }
}
