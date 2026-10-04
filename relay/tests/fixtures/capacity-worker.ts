// SPDX-License-Identifier: Apache-2.0
// Isolated synthetic storage controls. Never deploy this entrypoint.
import { RelayAuthority } from '../../src/worker.ts';
import { boundedStore, CAPACITY_KEY, AUTHORITY_RECORD_LIMIT } from '../../src/bounded-store.ts';
export { default } from '../../src/worker.ts';

export class CapacityFixture extends RelayAuthority {
  constructor(private fixtureState: DurableObjectState, env: ConstructorParameters<typeof RelayAuthority>[1]) {
    super(fixtureState, env);
  }
  async fetch(request: Request): Promise<Response> {
    if (new URL(request.url).pathname !== '/__capacity_fixture') return super.fetch(request);
    const { operation } = await request.json() as { operation: string };
    const store = boundedStore(this.fixtureState.storage);
    if (operation === 'fill') await store.transaction(async tx => {
      const count = await tx.get<number>(CAPACITY_KEY) ?? 0;
      for (let n = count; n < AUTHORITY_RECORD_LIMIT; n++) await tx.put(`z:fixture:${n}`, { synthetic: true });
    });
    else if (operation === 'release-one') await store.transaction(async tx => {
      const entries = await tx.list({ prefix: 'z:fixture:', limit: 1 });
      for (const key of entries.keys()) await tx.delete(key);
    });
    else if (operation === 'alarm') await this.alarm();
    else if (operation !== 'inspect') return new Response(null, { status: 400 });
    return Response.json({ count: await this.fixtureState.storage.get(CAPACITY_KEY),
      devices: (await this.fixtureState.storage.list({ prefix: 'device:', limit: 10 })).size,
      connectors: (await this.fixtureState.storage.list({ prefix: 'connector:', limit: 10 })).size });
  }
}
