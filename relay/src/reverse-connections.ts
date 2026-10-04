// SPDX-License-Identifier: Apache-2.0
import { deviceCredential, failure, HttpFailure } from './http.ts';
import { authorizationRead, type ReverseQueryRequest } from './proxy.ts';
import type { ConnectorProbe } from './connector-check.ts';
import type { ConnectorRoute } from './proxy.ts';
import { ReverseTransport, reverseQueryFetcher } from './reverse-transport.ts';
import { randomSecret, validSecret } from './secrets.ts';

const PROTOCOL = 'wenlan.reverse.v1';
const MAX_CONNECTIONS = 2;
const MAX_FRAMES_PER_MINUTE = 16_384;
interface Attachment { version: 1; deviceId: string; connectionId: string; generation?: number }
interface Entry extends Attachment {
  socket: WebSocket;
  transport: ReverseTransport;
  closed: boolean;
  frameWindow: number;
  frameCount: number;
}

/** Internal same-isolate classification for a missing local channel or a
 * post-authority-success transport dispatch rejection before a Response.
 * Never a wire type; carries no original details.
 */
export class ReverseChannelUnavailable extends Error {
  constructor() {
    super('Connector unavailable');
    this.name = 'ReverseChannelUnavailable';
  }
}

export interface ReverseConnectionAuthority {
  authenticate(id: string, token: string): Promise<boolean>;
  activate(id: string, token: string, connectionId: string, probe: ConnectorProbe,
    connected: () => boolean): Promise<ConnectorRoute | null>;
  current(id: string, connectionId: string, generation: number): Promise<boolean>;
}

/** DO-owned channels. Attachments contain no credential, body or knowledge data.
 * In-flight requests keep the object live; idle verified sockets may hibernate.
 * Pending verification is never restored as an authorized connection.
 */
export class ReverseConnections {
  private readonly state: DurableObjectState;
  private readonly authority: ReverseConnectionAuthority;
  private readonly entries = new Map<string, Entry>();
  private readonly query = reverseQueryFetcher(id => {
    const entry = this.entries.get(id);
    return entry && !entry.closed && entry.generation !== undefined ? entry.transport : undefined;
  });

  constructor(state: DurableObjectState, authority: ReverseConnectionAuthority,
    private readonly ownsDevice: (id: string) => boolean) {
    this.state = state;
    this.authority = authority;
    for (const socket of state.getWebSockets()) {
      try {
        const data: unknown = socket.deserializeAttachment();
        if (!this.validAttachment(data) || data.generation === undefined || this.entries.size >= MAX_CONNECTIONS
          || this.entries.has(data.connectionId)) throw new Error('Invalid connection');
        this.attach(socket, data);
      } catch { try { socket.close(1008, 'Connection unavailable'); } catch { /* Already closed. */ } }
    }
  }

  private validAttachment(value: unknown): value is Attachment {
    if (!value || typeof value !== 'object') return false;
    const data = value as Attachment;
    return data.version === 1 && validSecret(data.deviceId) && this.ownsDevice(data.deviceId) && validSecret(data.connectionId)
      && (data.generation === undefined || (Number.isSafeInteger(data.generation) && data.generation >= 0));
  }

  private attach(socket: WebSocket, data: Attachment): Entry {
    let entry: Entry;
    const transport = new ReverseTransport({ send: text => socket.send(text), close: () => this.close(entry) });
    entry = { ...data, socket, transport, closed: false, frameWindow: Math.floor(Date.now() / 60_000), frameCount: 0 };
    this.entries.set(data.connectionId, entry);
    return entry;
  }

  private close(entry: Entry): void {
    if (entry.closed) return;
    entry.closed = true;
    if (this.entries.get(entry.connectionId) === entry) this.entries.delete(entry.connectionId);
    entry.transport.disconnect();
    try { entry.socket.close(1000, 'Connection unavailable'); } catch { /* Already closed. */ }
  }

  private connected(entry: Entry): boolean {
    return !entry.closed && this.entries.get(entry.connectionId) === entry && entry.socket.readyState === WebSocket.OPEN;
  }

  private async current(entry: Entry): Promise<boolean> {
    if (!this.connected(entry) || entry.generation === undefined) return false;
    return authorizationRead(() => this.authority.current(entry.deviceId, entry.connectionId, entry.generation!));
  }

  async open(request: Request): Promise<Response> {
    const { id, token } = deviceCredential(request);
    if (!this.ownsDevice(id)) return failure(401, 'Device authentication failed');
    if (request.headers.get('upgrade')?.toLowerCase() !== 'websocket'
      || request.headers.get('sec-websocket-protocol') !== PROTOCOL) return failure(400, 'Reverse protocol upgrade required');
    if (!await authorizationRead(() => this.authority.authenticate(id, token))) {
      return failure(401, 'Device authentication failed');
    }
    const owned = [...this.entries.values()].filter(entry => entry.deviceId === id);
    if (this.entries.size >= MAX_CONNECTIONS || owned.some(entry => entry.generation === undefined) || owned.length >= 2) {
      return failure(429, 'Connection limit reached');
    }
    const connectionId = randomSecret();
    const [client, server] = Object.values(new WebSocketPair());
    this.state.acceptWebSocket(server);
    const data: Attachment = { version: 1, deviceId: id, connectionId };
    server.serializeAttachment(data);
    const entry = this.attach(server, data);
    this.state.waitUntil((async () => {
      try {
        const route = await this.authority.activate(id, token, connectionId,
          (headers, signal) => {
            const values: Record<string, string> = {};
            headers.forEach((value, name) => { values[name] = value; });
            return entry.transport.request({ path: '/connector-info', method: 'GET', headers: values, body: '' }, signal);
          }, () => this.connected(entry));
        if (!route || !this.connected(entry)) { this.close(entry); return; }
        entry.generation = route.generation;
        server.serializeAttachment({ ...data, generation: route.generation });
        for (const other of this.entries.values()) {
          if (other !== entry && other.deviceId === id) this.close(other);
        }
      } catch { this.close(entry); }
    })());
    return new Response(null, { status: 101, webSocket: client, headers: {
      'sec-websocket-protocol': PROTOCOL, 'x-wenlan-connection-id': connectionId,
      'cache-control': 'no-store',
    } });
  }

  async status(request: Request): Promise<Response> {
    const { id, token } = deviceCredential(request);
    const connectionId = request.headers.get('x-wenlan-connection-id') ?? '';
    if (!validSecret(connectionId)) throw new HttpFailure(400, 'Invalid connection request');
    if (!this.ownsDevice(id) || !await authorizationRead(() => this.authority.authenticate(id, token))) {
      return failure(401, 'Device authentication failed');
    }
    const entry = this.entries.get(connectionId);
    const connected = entry?.deviceId === id && await this.current(entry);
    return Response.json(connected ? { connected: true, generation: entry.generation }
      : { connected: false }, { headers: { 'cache-control': 'no-store' } });
  }

  async fetch(connectionId: string, request: ReverseQueryRequest): Promise<Response> {
    const entry = this.entries.get(connectionId);
    if (!entry) throw new ReverseChannelUnavailable();
    // Authority check runs outside any transport catch: a timeout, store
    // rejection, or false propagates untyped and never becomes unavailable.
    if (!await this.current(entry)) {
      this.close(entry);
      throw new Error('Connector unavailable');
    }
    try {
      return await this.query(connectionId, request, entry.deviceId);
    } catch {
      throw new ReverseChannelUnavailable();
    }
  }

  message(socket: WebSocket, message: unknown): void {
    const entry = this.entryFor(socket);
    if (!entry) { try { socket.close(1008, 'Connection unavailable'); } catch { /* Already closed. */ } return; }
    const minute = Math.floor(Date.now() / 60_000);
    if (entry.frameWindow !== minute) { entry.frameWindow = minute; entry.frameCount = 0; }
    if (++entry.frameCount > MAX_FRAMES_PER_MINUTE) { this.close(entry); return; }
    entry.transport.receive(message);
  }

  disconnect(socket: WebSocket): void {
    const entry = this.entryFor(socket);
    if (entry) this.close(entry);
  }

  private entryFor(socket: WebSocket): Entry | undefined {
    // Resolve from platform-owned attachment data after an object wakeup,
    // without relying on JS wrapper identity across hibernation callbacks.
    try {
      const data: unknown = socket.deserializeAttachment();
      if (this.validAttachment(data)) {
        const entry = this.entries.get(data.connectionId);
        if (entry?.deviceId === data.deviceId) return entry;
      }
    } catch { /* A closing socket may no longer expose its attachment. */ }
    return [...this.entries.values()].find(entry => entry.socket === socket);
  }

  closeDevice(id: string): void {
    for (const entry of this.entries.values()) if (entry.deviceId === id) this.close(entry);
  }

  async sweep(): Promise<void> {
    await Promise.all([...this.entries.values()].map(async entry => {
      if (entry.generation === undefined) return;
      try { if (!await this.current(entry)) this.close(entry); }
      catch { this.close(entry); }
    }));
  }
}
