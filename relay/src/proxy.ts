// SPDX-License-Identifier: Apache-2.0

/** The OAuth adapter must obtain this from verified tokens, never request JSON. */
export interface QueryGrant {
  subject: string;
  connectorId: string;
  space: string;
  generation: number;
  scopes: readonly string[];
  expiresAt: number;
}

/** Read fresh from the authoritative store on every request, including DELETE. */
export interface ConnectorRoute {
  id: string;
  subject: string;
  space: string;
  generation: number;
  enabled: boolean;
  expiresAt: number;
  tunnelOrigin?: string;
  reverseConnectionId?: string;
  backendToken: string;
}

export interface ReverseQueryRequest {
  method: string;
  headers: Headers;
  body?: Uint8Array<ArrayBuffer>;
  signal: AbortSignal;
}

export interface ProxyOptions {
  publicOrigin: string;
  loadRoute: (id: string) => Promise<ConnectorRoute | null>;
  fetch?: typeof globalThis.fetch;
  fetchReverse?: (connectionId: string, request: ReverseQueryRequest, deviceId: string) => Promise<Response>;
  now?: () => number;
  authorize?: (route: ConnectorRoute) => Promise<boolean>;
}

const QUERY_TOOLS = new Set(['brief', 'recall', 'get_page_sources']);
const REQUEST_HEADERS = ['accept', 'content-type', 'mcp-session-id', 'mcp-protocol-version', 'last-event-id'];
const RESPONSE_HEADERS = ['content-type', 'mcp-session-id', 'mcp-protocol-version'];
const MAX_BODY_BYTES = 64 * 1024;
const BODY_TIMEOUT_MS = 10_000;
const UPSTREAM_TIMEOUT_MS = 30_000;
const AUTH_RECHECK_MS = 1000;
const AUTH_READ_TIMEOUT_MS = 2000;
const MAX_RESPONSE_BYTES = 2 * 1024 * 1024;

function failure(status: number, error: string): Response {
  return Response.json({ error }, { status, headers: { 'cache-control': 'no-store' } });
}

export function tunnelOrigin(value: string): string | null {
  try {
    const url = new URL(value);
    // Keep the existing quick-tunnel transport, with an actual hostname check.
    if (url.protocol !== 'https:' || url.port || url.username || url.password
      || url.pathname !== '/' || url.search || url.hash
      || !/^[a-z0-9](?:[a-z0-9-]*[a-z0-9])?\.trycloudflare\.com$/.test(url.hostname)) return null;
    return url.origin;
  } catch {
    return null;
  }
}

/** Preserve existing tunnel session hashes; reverse connections have their own
 * binding so a replacement socket cannot inherit a prior MCP session.
 */
export function connectorBinding(route: ConnectorRoute): string {
  const binding: unknown[] = [route.id, route.tunnelOrigin, route.backendToken, route.generation];
  if (route.reverseConnectionId !== undefined) binding.push('reverse', route.reverseConnectionId);
  return JSON.stringify(binding);
}

function validGrant(grant: QueryGrant | null, now: number): grant is QueryGrant {
  return !!grant && typeof grant.subject === 'string' && !!grant.subject
    && typeof grant.connectorId === 'string' && !!grant.connectorId
    && typeof grant.space === 'string' && !!grant.space.trim()
    && Number.isSafeInteger(grant.generation) && grant.generation >= 0
    && Number.isFinite(grant.expiresAt) && grant.expiresAt > now
    && Array.isArray(grant.scopes) && grant.scopes.includes('wenlan:query');
}

function routeAllows(route: ConnectorRoute | null, grant: QueryGrant, now: number): route is ConnectorRoute {
  return !!route && route.enabled === true && route.id === grant.connectorId && route.subject === grant.subject
    && route.space === grant.space && route.generation === grant.generation
    && Number.isFinite(route.expiresAt) && route.expiresAt > now;
}

export async function authorizationRead<T>(operation: () => Promise<T>): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([operation(), new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error('Authorization read timed out')), AUTH_READ_TIMEOUT_MS);
    })]);
  } finally { clearTimeout(timer); }
}

const loadRoute = (options: ProxyOptions, id: string) => authorizationRead(() => options.loadRoute(id));

/** Watches the request while headers are pending and while a response is idle.
 * Every delivered chunk also gets a fresh authorization check. Ownership of
 * timers and upstream cancellation transfers to the wrapped response body.
 */
function responseLease(request: Request, grant: QueryGrant, route: ConnectorRoute, options: ProxyOptions, now: number) {
  const revoked = new AbortController();
  const lifetime = Math.max(1, Math.min(UPSTREAM_TIMEOUT_MS, grant.expiresAt - now, route.expiresAt - now));
  const signal = AbortSignal.any([request.signal, revoked.signal, AbortSignal.timeout(lifetime)]);
  let stopped = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  function stop() {
    stopped = true;
    clearTimeout(timer);
    signal.removeEventListener('abort', stop);
  }
  async function check() {
    try {
      await authorizationRead(async () => {
        if (signal.aborted) throw new Error('Aborted');
        const current = await options.loadRoute(grant.connectorId);
        const checkedAt = (options.now ?? Date.now)();
        if (signal.aborted || !validGrant(grant, checkedAt) || !routeAllows(current, grant, checkedAt)
          || current.tunnelOrigin !== route.tunnelOrigin || current.backendToken !== route.backendToken
          || current.reverseConnectionId !== route.reverseConnectionId) {
          throw new Error('Authorization changed');
        }
        if (options.authorize && !await options.authorize(current)) throw new Error('Authorization changed');
        if (signal.aborted) throw new Error('Aborted');
      });
    } catch {
      revoked.abort();
      throw new Error('Connector stream unavailable');
    }
  }
  async function watch() {
    if (stopped || signal.aborted) return;
    try { await check(); } catch { return; }
    if (!stopped && !signal.aborted) timer = setTimeout(watch, AUTH_RECHECK_MS);
  }
  signal.addEventListener('abort', stop, { once: true });
  if (!signal.aborted) timer = setTimeout(watch, AUTH_RECHECK_MS);

  function wrap(body: ReadableStream<Uint8Array>): ReadableStream<Uint8Array> {
    const reader = body.getReader();
    let ended = false;
    let bytes = 0;
    let controller: ReadableStreamDefaultController<Uint8Array>;
    function cleanup() {
      ended = true;
      signal.removeEventListener('abort', abort);
      stop();
    }
    function abort() {
      if (ended) return;
      cleanup();
      controller.error(new Error('Connector stream unavailable'));
      revoked.abort();
      void cancelReader();
    }
    async function cancelReader() {
      await reader.cancel().catch(() => {});
      reader.releaseLock();
    }
    return new ReadableStream<Uint8Array>({
      start(value) {
        controller = value;
        signal.addEventListener('abort', abort, { once: true });
        if (signal.aborted) abort();
      },
      async pull() {
        if (ended) return;
        try {
          const next = await reader.read();
          if (ended) return;
          if (next.done) {
            cleanup();
            controller.close();
            reader.releaseLock();
            return;
          }
          bytes += next.value.byteLength;
          if (bytes > MAX_RESPONSE_BYTES) { abort(); return; }
          await check();
          if (!ended) controller.enqueue(next.value);
        } catch { abort(); }
      },
      async cancel() {
        if (ended) return;
        cleanup();
        revoked.abort();
        await cancelReader();
      },
    });
  }
  return { signal, check, wrap, stop };
}

export async function readQueryBody(request: Request): Promise<Uint8Array<ArrayBuffer>> {
  const length = request.headers.get('content-length');
  if (length !== null && (!/^\d+$/.test(length) || Number(length) > MAX_BODY_BYTES)) throw new Error('size');
  if (!request.body) throw new Error('body');
  const reader = request.body.getReader();
  const deadline = AbortSignal.any([request.signal, AbortSignal.timeout(BODY_TIMEOUT_MS)]);
  const cancel = () => { void reader.cancel().catch(() => {}); };
  deadline.addEventListener('abort', cancel, { once: true });
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    while (true) {
      if (deadline.aborted) throw new Error('aborted');
      const { value, done } = await reader.read();
      if (deadline.aborted) throw new Error('aborted');
      if (done) break;
      size += value.byteLength;
      if (size > MAX_BODY_BYTES) throw new Error('size');
      chunks.push(value);
    }
  } catch (error) {
    await reader.cancel().catch(() => {});
    throw error;
  } finally {
    deadline.removeEventListener('abort', cancel);
    reader.releaseLock();
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return bytes;
}

const TOOL_UNAVAILABLE_TEXT = 'Wenlan could not reach your local device. Make sure Wenlan is running and the device is online, then try again. This does not mean your authorization was revoked.';

/** Caller RPC id for an already allowed POST tools/call, or null for every
 * excluded shape (notifications, initialize, tools/list, malformed IDs).
 */
function toolCallId(bytes: Uint8Array): string | number | null {
  try {
    const rpc = JSON.parse(new TextDecoder('utf-8', { fatal: true, ignoreBOM: false }).decode(bytes));
    if (!rpc || Array.isArray(rpc) || rpc.jsonrpc !== '2.0') return null;
    if (rpc.method !== 'tools/call') return null;
    if (typeof rpc.id === 'string') return rpc.id;
    if (typeof rpc.id === 'number' && Number.isSafeInteger(rpc.id)) return rpc.id;
    return null;
  } catch {
    return null;
  }
}

/** Recoverable tool result for an unreachable device. Carries only the
 * caller's RPC id and safe user text: no credentials, hostnames, paths,
 * raw exception details, or WWW-Authenticate.
 */
function toolUnavailable(id: string | number): Response {
  return Response.json({ jsonrpc: '2.0', id, result: { content: [{ type: 'text', text: TOOL_UNAVAILABLE_TEXT }], isError: true } },
    { status: 200, headers: { 'cache-control': 'no-store' } });
}

function allowedRpc(bytes: Uint8Array, space: string): boolean {
  try {
    const rpc = JSON.parse(new TextDecoder('utf-8', { fatal: true, ignoreBOM: false }).decode(bytes));
    if (!rpc || Array.isArray(rpc) || rpc.jsonrpc !== '2.0') return false;
    if (['initialize', 'notifications/initialized', 'ping', 'tools/list'].includes(rpc.method)) return true;
    if (rpc.method !== 'tools/call' || !QUERY_TOOLS.has(rpc.params?.name)) return false;
    const args = rpc.params.arguments ?? {};
    if (!args || typeof args !== 'object' || Array.isArray(args)) return false;
    for (const key of ['space', 'domain']) {
      if (key in args && args[key] !== space) return false;
    }
    return true;
  } catch {
    return false;
  }
}

/** Policy boundary after OAuth verification; not a standalone auth server. */
export async function forwardQuery(
  request: Request, grant: QueryGrant | null, options: ProxyOptions,
): Promise<Response> {
  const now = (options.now ?? Date.now)();
  if (!validGrant(grant, now)) return failure(401, 'Authorization required');
  const url = new URL(request.url);
  if (url.origin !== options.publicOrigin || url.pathname !== '/mcp' || url.search) return failure(404, 'Not found');
  if (!['POST', 'GET', 'DELETE'].includes(request.method)) return failure(405, 'Method not allowed');

  let body: Uint8Array<ArrayBuffer> | undefined;
  if (request.method === 'POST') {
    if (request.headers.get('content-type')?.split(';')[0].trim().toLowerCase() !== 'application/json') {
      return failure(415, 'JSON required');
    }
    try {
      body = await readQueryBody(request);
    } catch {
      return failure(413, 'Invalid or oversized request body');
    }
    if (!allowedRpc(body, grant.space)) return failure(403, 'Operation is not authorized');
  }

  // Check revocation after reading the body, not against a pre-upload snapshot.
  let route: ConnectorRoute | null;
  try {
    route = await loadRoute(options, grant.connectorId);
  } catch {
    return failure(503, 'Connector unavailable');
  }
  const dispatchTime = (options.now ?? Date.now)();
  if (!validGrant(grant, dispatchTime)) return failure(401, 'Authorization required');
  if (!routeAllows(route, grant, dispatchTime)) return failure(403, 'Connection is not authorized');
  try {
    if (options.authorize && !await authorizationRead(() => options.authorize!(route))) {
      return failure(403, 'Connection is not authorized');
    }
  } catch { return failure(503, 'Connector unavailable'); }
  const authorizedAt = (options.now ?? Date.now)();
  if (!validGrant(grant, authorizedAt)) return failure(401, 'Authorization required');
  if (!routeAllows(route, grant, authorizedAt)) return failure(403, 'Connection is not authorized');
  const reverseId = route.reverseConnectionId;
  const origin = typeof route.tunnelOrigin === 'string' ? tunnelOrigin(route.tunnelOrigin) : null;
  const reverse = typeof reverseId === 'string' && /^[A-Za-z0-9_-]{32,128}$/.test(reverseId)
    && route.tunnelOrigin === undefined && !!options.fetchReverse;
  const tunnel = origin !== null && reverseId === undefined;
  // Match the local token verifier's bounded header-safe credential contract.
  if ((!reverse && !tunnel) || !/^[A-Za-z0-9_-]{32,128}$/.test(route.backendToken)) {
    return failure(503, 'Connector unavailable');
  }

  const headers = new Headers();
  for (const name of REQUEST_HEADERS) {
    const value = request.headers.get(name);
    if (value !== null) headers.set(name, value);
  }
  // Never forward the client's OAuth bearer, cookies or identity/Space headers.
  headers.set('authorization', `Bearer ${route.backendToken}`);
  const lease = responseLease(request, grant, route, options, authorizedAt);
  let upstream: Response;
  try {
    const upstreamRequest = {
      method: request.method,
      headers,
      body,
      signal: lease.signal,
    };
    upstream = reverse
      ? await options.fetchReverse!(reverseId!, upstreamRequest, route.id)
      : await (options.fetch ?? globalThis.fetch)(`${origin}/mcp`, { ...upstreamRequest, redirect: 'manual' });
  } catch {
    // Narrow: only an upstream transport rejection before a response is
    // received. A current lease check still runs first, so revocation,
    // expiry, session changes, aborts, deadlines and store failures stay
    // fail-closed transport errors instead of a recoverable tool result.
    const id = request.method === 'POST' && body !== undefined ? toolCallId(body) : null;
    if (id === null) {
      lease.stop();
      return failure(502, 'Connector unavailable');
    }
    try {
      await lease.check();
    } catch {
      lease.stop();
      return failure(502, 'Connector unavailable');
    }
    lease.stop();
    return toolUnavailable(id);
  }
  try {
    if (!upstream.ok) {
      lease.stop();
      await upstream.body?.cancel();
      if ([400, 404, 405].includes(upstream.status)) return failure(upstream.status, 'MCP request unavailable');
      return failure(502, 'Connector unavailable');
    }
    try { await lease.check(); } catch {
      await upstream.body?.cancel();
      throw new Error('Authorization changed');
    }
    const size = upstream.headers.get('content-length');
    if (size !== null && (!/^\d+$/.test(size) || Number(size) > MAX_RESPONSE_BYTES)) {
      lease.stop();
      await upstream.body?.cancel();
      return failure(502, 'Connector unavailable');
    }
    const responseHeaders = new Headers({ 'cache-control': 'no-store', 'x-accel-buffering': 'no' });
    for (const name of RESPONSE_HEADERS) {
      const value = upstream.headers.get(name);
      if (value !== null) responseHeaders.set(name, value);
    }
    const responseBody = upstream.body ? lease.wrap(upstream.body) : null;
    if (!responseBody) lease.stop();
    return new Response(responseBody, { status: upstream.status, headers: responseHeaders });
  } catch {
    lease.stop();
    return failure(502, 'Connector unavailable');
  }
}
