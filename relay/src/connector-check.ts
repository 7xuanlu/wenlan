// SPDX-License-Identifier: Apache-2.0
import { tunnelOrigin } from './proxy.ts';

export interface ConnectorContract {
  backendToken: string;
  space: string;
}

export type ConnectorProbe = (headers: Headers, signal: AbortSignal) => Promise<Response>;

export interface ConnectorCandidate extends ConnectorContract {
  tunnelOrigin: string;
}

const BACKEND_TOKEN_RE = /^[A-Za-z0-9_-]{32,128}$/;
const ASCII_CONTROL_RE = /[\u0000-\u001f\u007f]/;
const MAX_SPACE_LENGTH = 256;
const MAX_RESPONSE_BYTES = 4096;
const DEADLINE_MS = 5_000;

export function validConnectorContract(candidate: unknown): candidate is ConnectorContract {
  try {
    if (candidate === null || typeof candidate !== 'object') return false;
    const value = candidate as { backendToken?: unknown; space?: unknown };
    return typeof value.backendToken === 'string' && BACKEND_TOKEN_RE.test(value.backendToken)
      && typeof value.space === 'string' && value.space.length > 0
      && value.space.length <= MAX_SPACE_LENGTH && value.space.trim() === value.space
      && !ASCII_CONTROL_RE.test(value.space);
  } catch {
    return false;
  }
}

export async function verifyConnectorContract(
  candidate: unknown,
  probe: ConnectorProbe,
): Promise<boolean> {
  if (!validConnectorContract(candidate)) return false;
  const { backendToken, space } = candidate;
  const signal = AbortSignal.timeout(DEADLINE_MS);
  try {
    // A successful tokened response alone cannot prove the bearer gate exists.
    const anonymous = await probe(new Headers(), signal);
    await anonymous.body?.cancel();
    if (signal.aborted || anonymous.status !== 401) return false;

    const response = await probe(new Headers({
      authorization: `Bearer ${backendToken}`,
      accept: 'application/json',
    }), signal);
    if (signal.aborted || response.status !== 200
      || response.headers.get('content-type')?.split(';')[0].trim() !== 'application/json') {
      await response.body?.cancel();
      return false;
    }
    // Bound the entire streamed response, not just Content-Length.
    const reader = response.body?.getReader();
    if (!reader) return false;
    const cancel = () => { void reader.cancel().catch(() => {}); };
    signal.addEventListener('abort', cancel, { once: true });
    const chunks: Uint8Array[] = [];
    let length = 0;
    try {
      while (true) {
        if (signal.aborted) return false;
        const { value, done } = await reader.read();
        if (signal.aborted) return false;
        if (done) break;
        length += value.byteLength;
        if (length > MAX_RESPONSE_BYTES) return false;
        chunks.push(value);
      }
      const bytes = new Uint8Array(length);
      let offset = 0;
      for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
      const info = JSON.parse(new TextDecoder('utf-8', { fatal: true, ignoreBOM: false }).decode(bytes));
      return info?.contract_version === 1 && info.server === 'wenlan-mcp'
        && info.tool_profile === 'query-only' && info.authentication === 'bearer'
        && info.space === space;
    } finally {
      signal.removeEventListener('abort', cancel);
      await reader.cancel().catch(() => {});
      reader.releaseLock();
    }
  } catch {
    // Callers must not persist raw errors containing tunnel URLs or tokens.
    return false;
  }
}

/** Enrollment must call this before accepting or updating a tunnel route.
 * This checks backend configuration, not device ownership or OAuth identity.
 */
export async function verifyConnector(
  candidate: ConnectorCandidate,
  fetcher: typeof globalThis.fetch = globalThis.fetch,
): Promise<boolean> {
  const origin = tunnelOrigin(candidate.tunnelOrigin);
  if (!origin) return false;
  return verifyConnectorContract(candidate, (headers, signal) => fetcher(`${origin}/connector-info`, {
    headers, redirect: 'manual', signal,
  }));
}
