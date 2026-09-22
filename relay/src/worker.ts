// SPDX-License-Identifier: Apache-2.0
import icon from '../../app/icons/128x128.png';
import { DurableObject } from 'cloudflare:workers';
import { boundedRequest, deviceCredential, failure, handlePublicRequest, HttpFailure, pairingCSS, pairingJS, type PublicEnv } from './http.ts';
import { createOAuthProvider, forwardOAuthQuery } from './oauth.ts';
import { connectorKey } from './pairing.ts';
import type { ConnectorRoute, ReverseQueryRequest } from './proxy.ts';
import { hashSecret, validSecret } from './secrets.ts';
import { maintainAuthority } from './cleanup.ts';
import { boundedStore } from './bounded-store.ts';
import { domainVerification } from './domain-verification.ts';
import { ReverseChannelUnavailable, ReverseConnections } from './reverse-connections.ts';
import { authenticateReverseDevice, beginReverseActivation, commitReverseActivation, reverseConnectionCurrent, type OwnedState } from './reverse-devices.ts';
import { verifyConnectorContract } from './connector-check.ts';

interface Env extends PublicEnv {
  PUBLIC_ORIGIN: string;
  AUTHORITY: DurableObjectNamespace<RelayAuthority>;
  DOMAIN_VERIFICATION_TOKEN?: string;
}

const RATE_WINDOW_CAP = 4096;

/** One v1 authorization authority: device state and consent share a transaction
 * domain. Do not shard these independently or bind to the legacy route service.
 * Limits below are conservative initial limits, not a production capacity SLA.
 */
export class RelayAuthority extends DurableObject<Env> {
  private provider: ReturnType<typeof createOAuthProvider<Env>>;
  private store: ReturnType<typeof boundedStore>;
  private authorizationMutationActive = false;
  private connections: ReverseConnections;
  private central: boolean;
  constructor(private state: DurableObjectState, env: Env) {
    super(state, env);
    this.central = state.id.equals(env.AUTHORITY.idFromName('wenlan-v1'));
    state.storage.sql.exec('CREATE TABLE IF NOT EXISTS rate_window (key TEXT PRIMARY KEY, count INTEGER NOT NULL, expires INTEGER NOT NULL)');
    state.storage.sql.exec('CREATE INDEX IF NOT EXISTS rate_expiry ON rate_window(expires)');
    this.store = boundedStore(state.storage);
    this.connections = new ReverseConnections(state, {
      authenticate: (id, token) => this.authority().reverseAuthenticate(id, token),
      current: (id, connection, generation) => this.authority().reverseCurrent(id, connection, generation),
      activate: async (id, token, connection, probe, connected) => {
        const before = await this.authority().reverseBegin(id, token);
        if (!before || !connected() || !await verifyConnectorContract(before.contract, probe) || !connected()) return null;
        return this.authority().reverseCommit(id, token, connection, before);
      },
    }, id => !this.central && state.id.equals(env.AUTHORITY.idFromName(`wenlan-device:${id}`)));
    const loadRoute = async (id: string) => await state.storage.get<ConnectorRoute>(connectorKey(id)) ?? null;
    this.provider = createOAuthProvider<Env>(env.PUBLIC_ORIGIN, {
      fetch: (request, env) => handlePublicRequest(request, env, this.store, env.PUBLIC_ORIGIN,
        (grantId, subject) => this.provider.revokeStoredGrant(env, grantId, subject)),
    }, {
      fetch: (request, env) => forwardOAuthQuery(request, env.OAUTH_PROVIDER, env.PUBLIC_ORIGIN, loadRoute, this.store,
        undefined, (id, request, deviceId) => this.reverseFetch(deviceId, id, request)),
    }, loadRoute, this.store);
  }

  private authority() { return this.env.AUTHORITY.get(this.env.AUTHORITY.idFromName('wenlan-v1')); }
  private shard(id: string) {
    if (!validSecret(id)) throw new Error('Invalid device');
    return this.env.AUTHORITY.get(this.env.AUTHORITY.idFromName(`wenlan-device:${id}`));
  }

  // Binding-only RPC. The public HTTP entrypoint always addresses the central
  // authority; it cannot call these methods or select a device object.
  async reverseAuthenticate(id: string, token: string): Promise<boolean> {
    return this.central && authenticateReverseDevice(this.store, id, token);
  }
  async reverseBegin(id: string, token: string): Promise<OwnedState | null> {
    return this.central ? beginReverseActivation(this.store, id, token) : null;
  }
  async reverseCommit(id: string, token: string, connection: string, before: OwnedState): Promise<ConnectorRoute | null> {
    return this.central ? commitReverseActivation(this.store, id, token, connection, before,
      { connected: () => true }) : null;
  }
  async reverseCurrent(id: string, connection: string, generation: number): Promise<boolean> {
    return this.central && reverseConnectionCurrent(this.store, id, connection, generation);
  }
  async closeReverseDevice(id: string): Promise<void> {
    if (this.central || !this.state.id.equals(this.env.AUTHORITY.idFromName(`wenlan-device:${id}`))) return;
    this.connections.closeDevice(id);
  }

  private async reverseFetch(deviceId: string, connectionId: string, request: ReverseQueryRequest): Promise<Response> {
    const headers = new Headers(request.headers);
    // A caller must never mint the internal upstream marker; only the device
    // object may attach it after a missing-channel rejection.
    headers.delete('x-wenlan-upstream-unavailable');
    headers.set('x-wenlan-device-id', deviceId);
    headers.set('x-wenlan-connection-id', connectionId);
    const response = await this.shard(deviceId).fetch(new Request(`${this.env.PUBLIC_ORIGIN}/mcp`, {
      method: request.method, headers, body: request.body?.byteLength ? request.body : undefined, signal: request.signal,
    }));
    if (response.status === 503 && response.headers.get('x-wenlan-upstream-unavailable') === '1') {
      await response.body?.cancel().catch(() => {});
      throw new Error('Upstream unavailable');
    }
    return response;
  }

  private async channelFetch(request: Request): Promise<Response> {
    const id = request.headers.get('x-wenlan-device-id') ?? '';
    if (!validSecret(id) || !this.state.id.equals(this.env.AUTHORITY.idFromName(`wenlan-device:${id}`))) {
      return failure(403, 'Device is not authorized');
    }
    const path = new URL(request.url).pathname;
    if (path === '/devices/reverse/connect' && request.method === 'GET') {
      await this.state.storage.setAlarm(Date.now() + 60_000);
      return this.connections.open(request);
    }
    if (path === '/devices/reverse/status' && request.method === 'GET') return this.connections.status(request);
    if (path === '/mcp' && ['GET', 'POST', 'DELETE'].includes(request.method)) {
      const buffered = await boundedRequest(request, 64 * 1024);
      const headers = new Headers();
      for (const name of ['authorization', 'accept', 'content-type', 'mcp-session-id', 'mcp-protocol-version', 'last-event-id']) {
        const value = request.headers.get(name);
        if (value !== null) headers.set(name, value);
      }
      const body = new Uint8Array(await buffered.arrayBuffer());
      try {
        return await this.connections.fetch(request.headers.get('x-wenlan-connection-id') ?? '', {
          method: request.method, headers, body, signal: request.signal,
        });
      } catch (error) {
        // Internal marker only for the typed missing-channel / dispatch
        // rejection. Authority exceptions rethrow into the outer sanitized
        // failure path and are never blanket-normalized here.
        if (!(error instanceof ReverseChannelUnavailable)) throw error;
        const unavailable = failure(503, 'Wenlan connection unavailable');
        unavailable.headers.set('x-wenlan-upstream-unavailable', '1');
        return unavailable;
      }
    }
    return failure(404, 'Not found');
  }

  private async limit(request: Request): Promise<number> {
    const requestedPath = new URL(request.url).pathname;
    const path = requestedPath === '/devices/reverse' ? '/devices' : requestedPath;
    const ip = request.headers.get('cf-connecting-ip') ?? 'unknown';
    const peer = await hashSecret(ip.slice(0, 128));
    const now = Date.now();
    const minute = Math.floor(now / 60_000);
    const hour = Math.floor(now / 3_600_000);
    const windows = [
      { key: `global:${minute}`, max: 600, expires: (minute + 1) * 60_000 },
      { key: `peer:${peer}:${minute}`, max: 120, expires: (minute + 1) * 60_000 },
    ];
    if (path === '/devices' || path === '/oauth/register') windows.push({
      key: `${path}:${peer}:${hour}`, max: path === '/devices' ? 3 : 10, expires: (hour + 1) * 3_600_000,
    });
    if (path === '/pairing/sample' && request.method === 'POST') windows.push(
      { key: `sample-peer:${peer}:${minute}`, max: 10, expires: (minute + 1) * 60_000 },
      { key: `sample-global:${minute}`, max: 60, expires: (minute + 1) * 60_000 },
    );
    if (request.method === 'POST' && (path === '/devices' || path === '/oauth/register')) {
      const day = Math.floor(now / 86_400_000);
      windows.push({
        key: `path-global:${path}:day:${day}`, max: path === '/devices' ? 100 : 250,
        expires: (day + 1) * 86_400_000,
      });
    }
    const retryAfter = this.state.storage.transactionSync(() => {
      const rows = windows.map(window => ({
        window,
        row: this.state.storage.sql.exec<{ count: number }>('SELECT count FROM rate_window WHERE key = ?', window.key).toArray()[0],
      }));
      for (const { window, row } of rows) {
        if (row && row.count >= window.max) return Math.max(1, Math.ceil((window.expires - now) / 1000));
      }

      let missing = rows.filter(item => !item.row).length;
      let rowCount = this.state.storage.sql.exec<{ count: number }>('SELECT COUNT(*) AS count FROM rate_window').one().count;
      if (rowCount + missing > RATE_WINDOW_CAP) {
        this.state.storage.sql.exec(
          'DELETE FROM rate_window WHERE key IN (SELECT key FROM rate_window WHERE expires <= ? LIMIT 256)', now);
        rowCount = this.state.storage.sql.exec<{ count: number }>('SELECT COUNT(*) AS count FROM rate_window').one().count;
        missing = windows.filter(window => !this.state.storage.sql.exec(
          'SELECT key FROM rate_window WHERE key = ?', window.key).toArray()[0]).length;
        if (rowCount + missing > RATE_WINDOW_CAP) return 60;
      }

      for (const window of windows) this.state.storage.sql.exec(
        'INSERT INTO rate_window (key,count,expires) VALUES (?,1,?) ON CONFLICT(key) DO UPDATE SET count=count+1',
        window.key, window.expires);
      return 0;
    });
    const nextAlarm = await this.state.storage.getAlarm();
    if (nextAlarm === null || nextAlarm > now + 60_000) await this.state.storage.setAlarm(now + 60_000);
    return retryAfter;
  }

  async alarm(): Promise<void> {
    const now = Date.now();
    // Set the recovery alarm first: an exception or eviction must not abandon
    // pending retention work when no new client requests arrive.
    await this.state.storage.setAlarm(now + 60_000);
    if (!this.central) {
      await this.connections.sweep();
      if (this.state.getWebSockets().length === 0) await this.state.storage.deleteAlarm();
      return;
    }
    const maintenance = await maintainAuthority(this.store,
      (grantId, subject) => this.provider.revokeStoredGrant(this.env, grantId, subject), now);
    await this.connections.sweep();
    this.state.storage.sql.exec('DELETE FROM rate_window WHERE key IN (SELECT key FROM rate_window WHERE expires <= ? LIMIT 256)', now);
    const next = this.state.storage.sql.exec<{ expires: number | null }>('SELECT MIN(expires) AS expires FROM rate_window').one().expires;
    if (!maintenance.more && next === null) await this.state.storage.deleteAlarm();
  }

  async fetch(request: Request): Promise<Response> {
    try {
      const url = new URL(request.url);
      if (url.origin !== this.env.PUBLIC_ORIGIN || url.href.length > 8192) return failure(400, 'Invalid request target');
      if (!this.central) return await this.channelFetch(request);
      if (url.pathname === '/mcp' && request.headers.has('origin') && request.headers.get('origin') !== this.env.PUBLIC_ORIGIN) {
        return failure(403, 'MCP origin is not authorized');
      }
      if (!['GET', 'POST', 'DELETE'].includes(request.method)) return failure(405, 'Method not allowed');
      if ((request.headers.get('authorization')?.length ?? 0) > 4096 || (request.headers.get('cookie')?.length ?? 0) > 4096) {
        return failure(431, 'Request headers too large');
      }
      const retryAfter = await this.limit(request);
      if (retryAfter) {
        const response = failure(429, 'Request limit reached');
        response.headers.set('retry-after', String(retryAfter));
        return response;
      }
      if (url.pathname.startsWith('/devices/reverse') && (url.search || (request.method === 'GET' && request.body))) {
        return failure(400, 'Invalid connection request');
      }
      if (['/devices/reverse/connect', '/devices/reverse/status'].includes(url.pathname) && request.method === 'GET') {
        const { id, token } = deviceCredential(request);
        if (!await authenticateReverseDevice(this.store, id, token)) return failure(401, 'Device authentication failed');
        return await this.shard(id).fetch(request);
      }
      if (request.method === 'GET') {
        if (url.pathname === '/icon.png') return new Response(icon, { headers: { 'content-type': 'image/png' } });
        if (url.pathname === '/pairing.css') return new Response(pairingCSS, { headers: { 'content-type': 'text/css; charset=utf-8' } });
        if (url.pathname === '/pairing.js') return new Response(pairingJS, { headers: { 'content-type': 'text/javascript; charset=utf-8' } });
      }
      const buffered = await boundedRequest(request, url.pathname === '/mcp' ? 64 * 1024 : 16 * 1024);
      const ctx = { waitUntil: (promise: Promise<unknown>) => this.state.waitUntil(promise),
        passThroughOnException() {}, props: {} } as ExecutionContext;
      const exchanging = url.pathname === '/oauth/token' && request.method === 'POST';
      const completing = ['/pairing/complete', '/pairing/sample'].includes(url.pathname) && request.method === 'POST';
      const mutating = exchanging || completing;
      // Issuance and reauthorization share this authority. Do not interleave
      // the provider's KV read/modify/write flows, or hold this guard while
      // buffering an untrusted body or streaming an MCP response.
      if (mutating && this.authorizationMutationActive) {
        const response = failure(503, exchanging ? 'temporarily_unavailable' : 'Authorization temporarily unavailable');
        response.headers.set('retry-after', '1');
        return response;
      }
      if (mutating) this.authorizationMutationActive = true;
      try {
        const response = await this.provider.fetch(buffered, this.env, ctx);
        if (response.ok && request.method === 'POST'
          && ['/devices/revoke', '/devices/rotate', '/devices/refresh', '/devices/renew'].includes(url.pathname)) {
          const id = request.headers.get('x-wenlan-device-id') ?? '';
          await this.shard(id).closeReverseDevice(id);
        }
        response.headers.set('x-content-type-options', 'nosniff');
        response.headers.set('referrer-policy', 'no-referrer');
        return response;
      } finally { if (mutating) this.authorizationMutationActive = false; }
    } catch (error) {
      return error instanceof HttpFailure ? failure(error.status, error.message) : failure(503, 'Wenlan connection unavailable');
    }
  }

  webSocketMessage(socket: WebSocket, message: string | ArrayBuffer): void { this.connections.message(socket, message); }
  webSocketClose(socket: WebSocket): void { this.connections.disconnect(socket); }
  webSocketError(socket: WebSocket): void { this.connections.disconnect(socket); }
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    try {
      const origin = new URL(env.PUBLIC_ORIGIN);
      if (origin.protocol !== 'https:' || origin.origin !== env.PUBLIC_ORIGIN
        || new URL(request.url).origin !== origin.origin) return failure(400, 'Invalid request target');
      const verification = domainVerification(request, env.DOMAIN_VERIFICATION_TOKEN);
      if (verification) return verification;
      return await env.AUTHORITY.get(env.AUTHORITY.idFromName('wenlan-v1')).fetch(request);
    } catch { return failure(503, 'Wenlan connection unavailable'); }
  },
};
