// SPDX-License-Identifier: Apache-2.0

export class CleanupBatchIncomplete extends Error {
  constructor() { super('OAuth cleanup batch incomplete'); }
}

/** Apply a work budget to the pinned OAuth library's revokeGrant helper rather
 * than duplicating its private storage schema. A partial deletion deliberately
 * rejects: the durable denial remains, and the next pass resumes by listing the
 * remaining records. No other OAuth operation may use this restricted view.
 */
export function cleanupKV(kv: KVNamespace): KVNamespace {
  let operations = 0;
  const deadline = Date.now() + 1500;
  const take = () => {
    if (operations >= 12) throw new CleanupBatchIncomplete();
    if (Date.now() >= deadline) throw new Error('OAuth cleanup batch incomplete');
    operations++;
  };
  return new Proxy(kv, {
    get(target, property) {
      if (property === 'list') return (options: KVNamespaceListOptions = {}) => {
        take();
        return target.list({ ...options, limit: Math.min(options.limit ?? 4, 4) });
      };
      if (property === 'delete') return async (key: string) => { take(); return target.delete(key); };
      throw new Error('Unsupported OAuth cleanup operation');
    },
  });
}
