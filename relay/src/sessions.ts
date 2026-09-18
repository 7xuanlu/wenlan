// SPDX-License-Identifier: Apache-2.0
import type { PairingStore } from './pairing.ts';
import { authorizationRead, connectorBinding, forwardQuery, readQueryBody, type ConnectorRoute, type ProxyOptions, type QueryGrant } from './proxy.ts';
import { hashSecret, randomSecret, validSecret } from './secrets.ts';

/** Supplied only by the verified OAuth token summary; stable across refresh. */
export interface SessionIdentity { grantId: string; clientId: string }
export interface SessionRecord {
  id: string;
  owner: string;
  route: string;
  backendId: string;
  expiresAt: number;
  active: boolean;
}
const key = (id: string) => `mcp-session:${id}`;
const MAX_SESSION_MS = 24 * 60 * 60 * 1000;
const error = (status: number, message: string) => Response.json({ error: message }, {
  status, headers: { 'cache-control': 'no-store' },
});

/** Stateful HTTP boundary above the query proxy. Neither raw backend IDs nor
 * IDs minted for a different OAuth grant may select a backend session.
 */
export async function forwardSessionQuery(
  request: Request, grant: QueryGrant, identity: SessionIdentity,
  store: PairingStore, options: ProxyOptions,
): Promise<Response> {
  const now = options.now ?? Date.now;
  if (!identity.grantId || !identity.clientId || !Number.isFinite(grant.expiresAt) || grant.expiresAt <= now()) {
    return error(401, 'Authorization required');
  }
  let initializing = false;
  let body: Uint8Array<ArrayBuffer> | undefined;
  if (request.method === 'POST') {
    try {
      body = await readQueryBody(request);
      initializing = JSON.parse(new TextDecoder('utf-8', { fatal: true, ignoreBOM: false }).decode(body))?.method === 'initialize';
    } catch { return error(400, 'Invalid MCP request'); }
  }
  const id = request.headers.get('mcp-session-id');
  if (initializing && id !== null) return error(400, 'Initialize without a session');
  if (!initializing && id === null) return error(400, 'MCP session required');
  if (id !== null && !validSecret(id)) return error(404, 'MCP session unavailable');
  const owner = await hashSecret(JSON.stringify([identity.grantId, identity.clientId,
    grant.subject, grant.connectorId, grant.space, grant.generation]));
  let session: SessionRecord | undefined;
  let routeHash: string;
  try {
    const route = await authorizationRead(() => options.loadRoute(grant.connectorId));
    if (!route || !route.enabled || route.subject !== grant.subject || route.id !== grant.connectorId
      || route.space !== grant.space || route.generation !== grant.generation || route.expiresAt <= now()) {
      return error(403, 'Connection is not authorized');
    }
    // Backend restart/credential changes cannot silently rebind an old session.
    routeHash = await hashSecret(connectorBinding(route));
    if (id) {
      session = await authorizationRead(() => store.transaction(tx => tx.get<SessionRecord>(key(id))));
      if (!session || !session.active || session.owner !== owner || session.route !== routeHash
        || session.expiresAt <= now()) return error(404, 'MCP session unavailable');
    }
  } catch { return error(503, 'Connector unavailable'); }
  const headers = new Headers(request.headers);
  headers.delete('mcp-session-id');
  if (session) headers.set('mcp-session-id', session.backendId);
  const forwarded = new Request(request.url, { method: request.method, headers, body, signal: request.signal });
  const authorize = async (route: ConnectorRoute) => {
    if (await hashSecret(connectorBinding(route)) !== routeHash) return false;
    if (options.authorize && !await options.authorize(route)) return false;
    if (!session) return initializing;
    const current = await store.transaction(tx => tx.get<SessionRecord>(key(session!.id)));
    return !!current && current.active && current.owner === owner && current.route === routeHash
      && current.backendId === session.backendId && current.expiresAt > now();
  };
  const response = await forwardQuery(forwarded, grant, { ...options, authorize });
  const deactivate = () => authorizationRead(() => store.transaction(async tx => {
    const current = session ? await tx.get<SessionRecord>(key(session.id)) : undefined;
    if (current?.owner === owner) await tx.put(key(current.id), { ...current, active: false });
  }));
  if (!response.ok) {
    if (response.status === 404 && session) {
      try { await deactivate(); } catch { return error(503, 'Connector unavailable'); }
    }
    return response;
  }
  const backendId = response.headers.get('mcp-session-id');
  try {
    if (initializing) {
      if (!backendId || !/^[\x21-\x7e]{1,256}$/.test(backendId)) throw new Error('Invalid backend session');
      const candidate: SessionRecord = { id: randomSecret(), owner, route: routeHash, backendId,
        expiresAt: now() + MAX_SESSION_MS, active: true };
      const reverseKey = `mcp-session-owner:${await hashSecret(JSON.stringify([routeHash, backendId]))}`;
      const accepted = await authorizationRead(() => store.transaction(async tx => {
        const priorId = await tx.get<string>(reverseKey);
        const prior = priorId ? await tx.get<SessionRecord>(key(priorId)) : undefined;
        if (prior?.active && prior.expiresAt > now()) return false;
        if (await tx.get(key(candidate.id))) return false;
        await tx.put(key(candidate.id), candidate);
        await tx.put(reverseKey, candidate.id);
        return true;
      }));
      if (!accepted) throw new Error('Backend session already claimed');
      session = candidate;
    } else if (backendId !== null && backendId !== session!.backendId) {
      throw new Error('Backend changed session');
    }
    if (request.method === 'DELETE') {
      await response.body?.cancel();
      await deactivate();
    }
    const resultHeaders = new Headers(response.headers);
    resultHeaders.delete('mcp-session-id');
    if (initializing) resultHeaders.set('mcp-session-id', session!.id);
    return new Response(request.method === 'DELETE' ? null : response.body, { status: response.status, headers: resultHeaders });
  } catch {
    await response.body?.cancel().catch(() => {});
    return error(502, 'Connector unavailable');
  }
}
