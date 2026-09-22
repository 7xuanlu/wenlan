// SPDX-License-Identifier: Apache-2.0
// LOCAL TEST ONLY: returns the browser secret as JSON instead of setting a
// browser cookie, and uses a synthetic connector. Never deploy this fixture.
import { createOAuthProvider, startOAuthPairing, finishOAuthPairing, forwardOAuthQuery,
  type OAuthEnv } from '../../src/oauth.ts';
import { enrollDevice, authenticateDevice, revokeDevice } from '../../src/devices.ts';
import { approvePairing, connectorKey } from '../../src/pairing.ts';
import type { ConnectorRoute } from '../../src/proxy.ts';

const origin = 'https://wenlan-relay.example';
const candidate = { tunnelOrigin: 'https://synthetic.trycloudflare.com',
  backendToken: 'b'.repeat(43), space: 'review' };
interface Env extends OAuthEnv { AUTHORITY: DurableObjectNamespace }

export class FixtureOAuthAuthority {
  provider;
  constructor(private state: DurableObjectState, private env: Env) {
    this.provider = createOAuthProvider<Env>(origin, {
      fetch: async (request, env) => {
        const url = new URL(request.url);
        if (url.pathname === '/authorize') {
          try { return Response.json(await startOAuthPairing(env.OAUTH_PROVIDER, state.storage, request, origin)); }
          catch { return new Response('Authorization rejected', { status: 400 }); }
        }
        const args = await request.json() as Record<string, string>;
        if (url.pathname === '/fixture/enroll') return Response.json(await enrollDevice(state.storage, candidate, {
          fetch: (async (_input, init) => new Headers(init?.headers).has('authorization')
            ? Response.json({ contract_version: 1, server: 'wenlan-mcp', tool_profile: 'query-only',
              authentication: 'bearer', space: candidate.space })
            : new Response(null, { status: 401 })) as typeof fetch,
        }));
        if (url.pathname === '/fixture/approve') {
          const identity = await authenticateDevice(state.storage, args.deviceId, args.managementToken);
          return Response.json(identity && await approvePairing(state.storage, args.pairingId, identity,
            { clientId: args.clientId, resource: `${origin}/mcp`, space: candidate.space }));
        }
        if (url.pathname === '/fixture/revoke') return Response.json(
          await revokeDevice(state.storage, args.deviceId, args.managementToken));
        if (url.pathname === '/fixture/finish') return Response.json(
          await finishOAuthPairing(env.OAUTH_PROVIDER, state.storage, args.pairingId, args.browserSecret, origin));
        return new Response('Fixture route missing', { status: 404 });
      },
    }, {
      fetch: async (request, env) => forwardOAuthQuery(request, env.OAUTH_PROVIDER, origin,
        async id => await state.storage.get<ConnectorRoute>(connectorKey(id)) ?? null,
        state.storage,
        (async (_input, init) => {
          if (new Headers(init?.headers).get('authorization') !== `Bearer ${candidate.backendToken}`) {
            throw new Error('OAuth credential reached backend');
          }
          return Response.json({ jsonrpc: '2.0', id: 1, result: { synthetic: true } }, {
            headers: new Headers(init?.headers).has('mcp-session-id') ? {} : { 'mcp-session-id': crypto.randomUUID() },
          });
        }) as typeof fetch),
    }, async id => await state.storage.get<ConnectorRoute>(connectorKey(id)) ?? null, state.storage);
  }
  fetch(request: Request) {
    const ctx = { waitUntil: (promise: Promise<unknown>) => this.state.waitUntil(promise),
      passThroughOnException() {}, props: {} } as ExecutionContext;
    return this.provider.fetch(request, this.env, ctx);
  }
}

export default {
  fetch(request: Request, env: Env) {
    return env.AUTHORITY.get(env.AUTHORITY.idFromName('synthetic-authority')).fetch(request);
  },
};
