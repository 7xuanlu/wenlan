// SPDX-License-Identifier: Apache-2.0
import { decodeBody, decodeFrame, encodeBody, encodeFrame, type ReverseFrame } from './reverse-protocol.ts';
import type { ProxyOptions } from './proxy.ts';

export interface ReverseChannel {
  send(text: string): void;
  close(): void;
}

type RequestFrame = Extract<ReverseFrame, { type: 'request' }>;
export type ReverseRequest = Omit<RequestFrame, 'v' | 'type' | 'id'>;

interface Pending {
  resolve: (response: Response) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
  signal?: AbortSignal;
  abort: () => void;
  controller?: ReadableStreamDefaultController<Uint8Array>;
  pullDone?: () => void;
  head: boolean;
  resolved: boolean;
  bodyAllowed: boolean;
  credit: boolean;
  sequence: number;
  bytes: number;
}

const unavailable = () => new Error('Reverse connector unavailable');
const MAX_PENDING = 8;
const MAX_RESPONSE_BYTES = 2 * 1024 * 1024;
const MAX_CHUNKS = 4096;
const DEADLINE_MS = 30_000;

/** The owner supplies only verified, generation-bound connections. Selection
 * uses the server-side route ID, never any client header or request argument.
 */
export function reverseQueryFetcher(
  connection: (id: string) => ReverseTransport | undefined,
): NonNullable<ProxyOptions['fetchReverse']> {
  return async (id, request) => {
    const transport = connection(id);
    if (!transport || !['GET', 'POST', 'DELETE'].includes(request.method)) throw unavailable();
    const headers: Record<string, string> = {};
    request.headers.forEach((value, name) => { headers[name] = value; });
    return transport.request({
      path: '/mcp', method: request.method as ReverseRequest['method'],
      headers, body: encodeBody(request.body ?? new Uint8Array()),
    }, request.signal);
  };
}

/** Transport only: the owner must authenticate the socket and retain all
 * OAuth, device-generation, Space and revocation checks above this adapter.
 * No caller-supplied URL is accepted. This is not a public enrollment API.
 */
export class ReverseTransport {
  private readonly channel: ReverseChannel;
  private readonly deadlineMs: number;
  private readonly pending = new Map<string, Pending>();
  private readonly cancelled = new Set<string>();
  private closed = false;

  constructor(channel: ReverseChannel, options: { deadlineMs?: number } = {}) {
    const deadlineMs = options.deadlineMs ?? DEADLINE_MS;
    if (!Number.isSafeInteger(deadlineMs) || deadlineMs < 1 || deadlineMs > DEADLINE_MS) {
      throw new Error('Invalid reverse deadline');
    }
    this.channel = channel;
    this.deadlineMs = deadlineMs;
  }

  request(input: ReverseRequest, signal?: AbortSignal): Promise<Response> {
    if (this.closed || signal?.aborted || this.pending.size >= MAX_PENDING) {
      return Promise.reject(unavailable());
    }
    const id = crypto.randomUUID();
    let text: string;
    try { text = encodeFrame({ ...input, v: 1, type: 'request', id }); }
    catch { return Promise.reject(unavailable()); }

    return new Promise<Response>((resolve, reject) => {
      const abort = () => this.cancel(id);
      const state: Pending = {
        resolve, reject, abort, signal,
        timer: setTimeout(abort, this.deadlineMs),
        head: false, resolved: false, bodyAllowed: true, credit: false, sequence: 0, bytes: 0,
      };
      this.pending.set(id, state);
      signal?.addEventListener('abort', abort, { once: true });
      if (signal?.aborted) { abort(); return; }
      this.send(text);
    });
  }

  /** Call for each text WS message; binary or malformed data fails closed. */
  receive(text: unknown): void {
    if (this.closed) return;
    try {
      if (typeof text !== 'string') throw unavailable();
      const frame = decodeFrame(text);
      if (frame.type !== 'response' && frame.type !== 'chunk' && frame.type !== 'end' && frame.type !== 'cancel') {
        throw unavailable();
      }
      const state = this.pending.get(frame.id);
      if (!state) {
        // Cancellation can race data already in flight. Keep only a bounded
        // set of tombstones; unknown/completed IDs are otherwise a violation.
        if (this.cancelled.has(frame.id)) return;
        throw unavailable();
      }
      if (frame.type === 'cancel') {
        this.cancel(frame.id, false);
      } else if (frame.type === 'response') {
        if (state.head) throw unavailable();
        state.head = true;
        state.bodyAllowed = ![204, 205, 304].includes(frame.status);
        const body = state.bodyAllowed ? new ReadableStream<Uint8Array>({
          start: controller => { state.controller = controller; },
          pull: () => new Promise<void>(resolve => {
            state.pullDone = resolve;
            if (!this.pending.has(frame.id)) { resolve(); return; }
            state.credit = true;
            this.sendFrame({ v: 1, type: 'credit', id: frame.id, seq: state.sequence });
          }),
          cancel: () => { this.cancel(frame.id); },
        }, { highWaterMark: 0 }) : null;
        const headers = new Headers(frame.headers);
        headers.set('cache-control', 'no-store');
        const response = new Response(body, { status: frame.status, headers });
        state.resolved = true;
        state.resolve(response);
      } else if (frame.type === 'chunk') {
        if (!state.head || !state.bodyAllowed || !state.credit || frame.seq !== state.sequence
          || state.sequence >= MAX_CHUNKS) throw unavailable();
        const bytes = decodeBody(frame.body);
        state.bytes += bytes.byteLength;
        if (state.bytes > MAX_RESPONSE_BYTES) throw unavailable();
        state.credit = false;
        state.sequence++;
        state.controller!.enqueue(bytes);
        state.pullDone?.();
        state.pullDone = undefined;
      } else {
        if (!state.head) throw unavailable();
        this.remove(frame.id, state);
        state.controller?.close();
        state.pullDone?.();
      }
    } catch { this.disconnect(); }
  }

  /** A socket close/error, replacement or authority revocation must call this. */
  disconnect(): void {
    if (this.closed) return;
    this.closed = true;
    for (const [id, state] of this.pending) {
      this.remove(id, state);
      this.fail(state);
    }
    this.cancelled.clear();
    try { this.channel.close(); } catch { /* Already closed. */ }
  }

  private cancel(id: string, notifyPeer = true): void {
    const state = this.pending.get(id);
    if (!state) return;
    this.remove(id, state);
    this.cancelled.add(id);
    if (this.cancelled.size > 32) this.cancelled.delete(this.cancelled.values().next().value!);
    this.fail(state);
    if (notifyPeer) this.sendFrame({ v: 1, type: 'cancel', id });
  }

  private fail(state: Pending): void {
    state.controller?.error(unavailable());
    if (!state.resolved) state.reject(unavailable());
    state.pullDone?.();
    state.pullDone = undefined;
  }

  private remove(id: string, state: Pending): void {
    this.pending.delete(id);
    clearTimeout(state.timer);
    state.signal?.removeEventListener('abort', state.abort);
  }

  private sendFrame(frame: ReverseFrame): void {
    try { this.send(encodeFrame(frame)); } catch { this.disconnect(); }
  }

  private send(text: string): void {
    if (this.closed) return;
    try { this.channel.send(text); } catch { this.disconnect(); }
  }
}
