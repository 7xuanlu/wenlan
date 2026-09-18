// SPDX-License-Identifier: Apache-2.0
import { OAuthProvider, OAuthError, getOAuthApi, type AuthRequest, type OAuthHelpers, type OAuthProviderOptions } from '@cloudflare/workers-oauth-provider';
import { cleanupKV } from './cleanup-kv.ts';
import { beginPairing, consumePairing, type PairingStore } from './pairing.ts';
import { type ConnectorRoute, type QueryGrant, type ProxyOptions } from './proxy.ts';
import { forwardSessionQuery } from './sessions.ts';
import { authorizationGrantActive, claimAuthorizationGrant, replaceClientAuthorization } from './grants.ts';
import { randomSecret, validSecret } from './secrets.ts';

export interface OAuthEnv {
  OAUTH_KV: KVNamespace;
  OAUTH_PROVIDER: OAuthHelpers;
}
interface StoredAuthorization { request: AuthRequest; expiresAt: number }
const authorizationKey = (id: string) => `oauth-request:${id}`;
export const QUERY_SCOPE = 'wenlan:query';

function canonicalOrigin(value: string): string {
  const url = new URL(value);
  if (url.protocol !== 'https:' || url.origin !== value || url.username || url.password) {
    throw new Error('Invalid public OAuth origin');
  }
  return value;
}

/** The caller must deliver the browser secret only in a Secure/HttpOnly cookie,
 * and guard finish/cancel against CSRF. This function is not a public handler.
 */
export async function startOAuthPairing(
  oauth: OAuthHelpers, store: PairingStore, request: Request, publicOrigin: string,
) {
  const origin = canonicalOrigin(publicOrigin);
  const resource = `${origin}/mcp`;
  const parsed = await oauth.parseAuthRequest(request);
  if (parsed.responseType !== 'code' || parsed.issuer !== origin
    || parsed.resource !== resource || parsed.scope.length !== 1 || parsed.scope[0] !== QUERY_SCOPE
    || parsed.codeChallengeMethod !== 'S256'
    || !parsed.codeChallenge || !/^[A-Za-z0-9_-]{43}$/.test(parsed.codeChallenge)) {
    throw new Error('Unsupported OAuth authorization request');
  }
  const authorizationId = randomSecret();
  const pair = await beginPairing(store, {
    authorizationId, clientId: parsed.clientId, resource, scopes: parsed.scope,
  }, resource);
  // No browser-visible state is returned until the validated request is durable.
  // Failure here leaves only an unusable pairing, never an issuable grant.
  await store.transaction(async tx => {
    await tx.put(authorizationKey(authorizationId), {
      request: structuredClone(parsed), expiresAt: pair.expiresAt,
    } satisfies StoredAuthorization);
  });
  return pair;
}

export async function finishOAuthPairing(
  oauth: OAuthHelpers, store: PairingStore, pairingId: string, browserSecret: string,
  publicOrigin: string,
): Promise<string | null> {
  const origin = canonicalOrigin(publicOrigin);
  const consumed = await consumePairing(store, pairingId, browserSecret);
  if (!consumed) return null;
  const saved = await store.transaction(tx => tx.get<StoredAuthorization>(authorizationKey(consumed.authorizationId)));
  if (!saved || !Number.isFinite(saved.expiresAt) || saved.expiresAt <= Date.now()
    || saved.request.clientId !== consumed.clientId || saved.request.issuer !== origin
    || saved.request.resource !== `${origin}/mcp` || consumed.resource !== `${origin}/mcp`
    || saved.request.scope.length !== 1 || saved.request.scope[0] !== QUERY_SCOPE) return null;
  await replaceClientAuthorization(store, consumed.grant.subject, consumed.clientId, consumed.authorizationId);
  const result = await oauth.completeAuthorization({
    request: saved.request, userId: consumed.grant.subject,
    metadata: { scope: QUERY_SCOPE }, scope: [QUERY_SCOPE], props: { ...consumed.grant, authorizationId: consumed.authorizationId },
  });
  return result.redirectTo;
}

function challenge(origin: string): Response {
  return new Response(null, { status: 401, headers: {
    'cache-control': 'no-store',
    'www-authenticate': `Bearer resource_metadata="${origin}/.well-known/oauth-protected-resource/mcp", error="invalid_token", scope="${QUERY_SCOPE}"`,
  } });
}

/** Must run behind OAuthProvider. The helper supplies the actual token's scope
 * and expiry, not the authorization-time props (which survive token refresh).
 */
export async function forwardOAuthQuery(
  request: Request, oauth: OAuthHelpers, publicOrigin: string,
  loadRoute: (id: string) => Promise<ConnectorRoute | null>,
  store: PairingStore,
  fetcher?: typeof globalThis.fetch,
  fetchReverse?: ProxyOptions['fetchReverse'],
): Promise<Response> {
  const origin = canonicalOrigin(publicOrigin);
  const bearer = request.headers.get('authorization');
  if (!bearer?.startsWith('Bearer ') || bearer.length > 4096) return challenge(origin);
  let token;
  try { token = await oauth.unwrapToken<unknown>(bearer.slice(7)); }
  catch { return challenge(origin); }
  if (!token || token.audience !== `${origin}/mcp`
    || !Number.isFinite(token.expiresAt) || token.expiresAt * 1000 <= Date.now()
    || !Array.isArray(token.scope) || !token.scope.includes(QUERY_SCOPE)) return challenge(origin);
  const props = token.grant.props;
  if (!props || typeof props !== 'object') return challenge(origin);
  const bound = props as Partial<QueryGrant>;
  if (bound.subject !== token.userId || typeof bound.connectorId !== 'string'
    || !validSecret(bound.connectorId) || typeof bound.space !== 'string' || !bound.space.trim()
    || !Number.isSafeInteger(bound.generation) || (bound.generation ?? -1) < 0) return challenge(origin);
  const queryGrant: QueryGrant = {
    subject: token.userId, connectorId: bound.connectorId, space: bound.space,
    generation: bound.generation!, scopes: token.scope, expiresAt: token.expiresAt * 1000,
  };
  const identity = { grantId: token.grantId, clientId: token.grant.clientId };
  return forwardSessionQuery(request, queryGrant, identity, store,
    { publicOrigin: origin, loadRoute, fetch: fetcher, fetchReverse,
      authorize: async () => {
        if (!await authorizationGrantActive(store, identity, queryGrant)) return false;
        const current = await oauth.unwrapToken<unknown>(bearer.slice(7));
        return !!current && current.id === token.id && current.userId === token.userId
          && current.grantId === token.grantId && current.grant.clientId === token.grant.clientId
          && current.audience === `${origin}/mcp` && current.expiresAt * 1000 > Date.now()
          && current.scope.includes(QUERY_SCOPE);
      } });
}

/** DCR is the currently tested registration path. CIMD remains disabled until
 * the deployed runtime's strictly-public fetch flag and its flows are verified.
 */
export function createOAuthProvider<Env extends OAuthEnv>(
  publicOrigin: string, defaultHandler: ExportedHandler<Env>,
  apiHandler: { fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> },
  loadRoute: (id: string) => Promise<ConnectorRoute | null>,
  store: PairingStore,
) {
  const origin = canonicalOrigin(publicOrigin);
  const options: OAuthProviderOptions<Env> = {
    apiRoute: '/mcp', apiHandler, defaultHandler,
    authorizeEndpoint: `${origin}/authorize`, tokenEndpoint: `${origin}/oauth/token`,
    clientRegistrationEndpoint: `${origin}/oauth/register`,
    accessTokenTTL: 900, refreshTokenTTL: 30 * 24 * 60 * 60,
    clientRegistrationTTL: 90 * 24 * 60 * 60,
    scopesSupported: [QUERY_SCOPE], allowPlainPKCE: false, allowImplicitFlow: false,
    allowTokenExchangeGrant: false, clientIdMetadataDocumentEnabled: false,
    resourceMetadata: { resource: `${origin}/mcp`, authorization_servers: [origin],
      scopes_supported: [QUERY_SCOPE], bearer_methods_supported: ['header'], resource_name: 'Wenlan' },
    tokenExchangeCallback: async ({ props, userId, requestedScope, grantType, grantId, clientId }) => {
      if (requestedScope.length !== 1 || requestedScope[0] !== QUERY_SCOPE) {
        throw new OAuthError('invalid_scope', { description: 'Wenlan query permission is required' });
      }
      const bound = props as (Partial<QueryGrant> & { authorizationId?: string }) | null;
      if (!bound || typeof bound !== 'object' || bound.subject !== userId
        || typeof bound.connectorId !== 'string' || !validSecret(bound.connectorId)
        || !Number.isSafeInteger(bound.generation) || (bound.generation ?? -1) < 0
        || typeof bound.authorizationId !== 'string' || !validSecret(bound.authorizationId)) {
        throw new OAuthError('invalid_grant', { description: 'Wenlan authorization is invalid' });
      }
      const route = await loadRoute(bound.connectorId);
      if (!route || route.enabled !== true || route.id !== bound.connectorId
        || route.subject !== userId || route.space !== bound.space || route.generation !== bound.generation) {
        throw new OAuthError('invalid_grant', { description: 'Wenlan authorization has been revoked' });
      }
      const identity = { grantId, clientId };
      const grant = { subject: userId, connectorId: bound.connectorId, space: route.space, generation: route.generation };
      const valid = grantType === 'authorization_code'
        ? await claimAuthorizationGrant(store, identity, grant, bound.authorizationId)
        : await authorizationGrantActive(store, identity, grant);
      if (!valid) throw new OAuthError('invalid_grant', { description: 'Wenlan authorization is no longer valid' });
    },
    // Avoid the library's default warning logs containing request error details.
    onError: () => {},
  };
  const provider = new OAuthProvider<Env>(options);
  return {
    fetch: (request: Request, env: Env, ctx: ExecutionContext) => provider.fetch(request, env, ctx),
    // Alarms after eviction have no prior fetch to populate env.OAUTH_PROVIDER.
    revokeStoredGrant: (env: Env, grantId: string, subject: string) => getOAuthApi(options,
      { ...env, OAUTH_KV: cleanupKV(env.OAUTH_KV) }).revokeGrant(grantId, subject),
  };
}
