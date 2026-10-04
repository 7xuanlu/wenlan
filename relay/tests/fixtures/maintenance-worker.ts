// SPDX-License-Identifier: Apache-2.0
// Synthetic runtime fixture only. Never deploy these state/fault controls.
import { RelayAuthority } from '../../src/worker.ts';
import { boundedStore } from '../../src/bounded-store.ts';
export { default } from '../../src/worker.ts';

export class MaintenanceFixture extends RelayAuthority {
  private fixtureState: DurableObjectState;
  constructor(state: DurableObjectState, env: ConstructorParameters<typeof RelayAuthority>[1]) {
    super(state, env);
    this.fixtureState = state;
  }
  async fetch(request: Request): Promise<Response> {
    if (new URL(request.url).pathname !== '/__fixture') return super.fetch(request);
    const { operation, records } = await request.json() as { operation: string; records?: Record<string, unknown> };
    if (operation === 'seed') await boundedStore(this.fixtureState.storage).transaction(async tx => {
      for (const [key, value] of Object.entries(records!)) await tx.put(key, value);
    });
    else if (operation === 'alarm') await this.alarm();
    else if (operation === 'schedule') await this.fixtureState.storage.setAlarm(Date.now() + 25);
    else if (operation === 'clear') {
      const keys = [...(await this.fixtureState.storage.list({ limit: 128 })).keys()];
      if (keys.length) await this.fixtureState.storage.delete(keys);
      this.fixtureState.storage.sql.exec('DELETE FROM rate_window');
    } else if (operation !== 'inspect') return new Response(null, { status: 400 });
    return Response.json({ records: Object.fromEntries(await this.fixtureState.storage.list({ limit: 128 })),
      alarm: await this.fixtureState.storage.getAlarm() });
  }
}
