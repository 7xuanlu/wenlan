// SPDX-License-Identifier: Apache-2.0
// Local workerd fixture only. Its unauthenticated seed/fault operations must
// never be exported by a deployed Worker or used as an enrollment API.
import { beginPairing, inspectPairing, approvePairing, consumePairing,
  cancelPairing, connectorKey, type PairingStore } from '../../src/pairing.ts';
import type { ConnectorRoute } from '../../src/proxy.ts';
import { enrollDevice, authenticateDevice, revokeDevice, rotateDeviceCredential } from '../../src/devices.ts';

export class FixturePairingObject {
  constructor(private state: DurableObjectState) {}

  async fetch(request: Request): Promise<Response> {
    const { operation, args } = await request.json() as { operation: string; args: any[] };
    // Structural compatibility is checked against the real Cloudflare types.
    const store: PairingStore = this.state.storage;
    switch (operation) {
      case 'enroll': return Response.json(await enrollDevice(store, args[0], {
        fetch: (async (_input, init) => new Headers(init?.headers).has('authorization')
          ? Response.json({ contract_version: 1, server: 'wenlan-mcp',
            tool_profile: 'query-only', authentication: 'bearer', space: args[0].space })
          : new Response(null, { status: 401 })) as typeof fetch,
      }));
      case 'authenticate': return Response.json(await authenticateDevice(store, args[0], args[1]));
      case 'rotate': return Response.json(await rotateDeviceCredential(store, args[0], args[1]));
      case 'revoke': return Response.json(await revokeDevice(store, args[0], args[1]));
      case 'seed': {
        const route = args[0] as ConnectorRoute;
        await this.state.storage.put(connectorKey(route.id), route);
        return Response.json(true);
      }
      case 'begin': return Response.json(await beginPairing(store, args[0], args[1]));
      case 'inspect': return Response.json(await inspectPairing(store, args[0]));
      case 'approve': return Response.json(await approvePairing(store, args[0], args[1], args[2]));
      case 'consume': return Response.json(await consumePairing(store, args[0], args[1]));
      case 'cancel': return Response.json(await cancelPairing(store, args[0], args[1]));
      case 'fail-consume': {
        const failing: PairingStore = {
          transaction: operation => store.transaction(async tx => {
            const result = await operation(tx);
            if (result) throw new Error('synthetic failure before transaction commit');
            return result;
          }),
        };
        try {
          await consumePairing(failing, args[0], args[1]);
          return Response.json({ injected: false });
        } catch (error) {
          if (!(error instanceof Error) || !error.message.includes('synthetic failure')) throw error;
          return Response.json({ injected: true });
        }
      }
      default: return new Response('Unknown fixture operation', { status: 404 });
    }
  }
}

export default {
  fetch() { return new Response('No public API in this fixture', { status: 404 }); },
};
