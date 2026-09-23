// SPDX-License-Identifier: Apache-2.0
// Pure bounded SAFE diagnostic for MCP initialize responses. Strict allowlist:
// status, normalized content-type, body bytes, parse flag, and at most
// MAX_MESSAGES message shapes. Never retains raw bodies, headers, session IDs,
// cookies, tokens, credentials, error.message text, or error.data.
export const MAX_MESSAGES = 16;

// Exact static safe text from relay/src/proxy.ts (TOOL_UNAVAILABLE_TEXT).
export const DEVICE_UNAVAILABLE_TEXT =
  'Wenlan could not reach your local device. Make sure Wenlan is running and the device is online, then try again. This does not mean your authorization was revoked.';

function contentKind(value) {
  const raw = String(value ?? '').split(';', 1)[0].trim().toLowerCase();
  if (raw === 'application/json') return 'json';
  if (raw === 'text/event-stream') return 'sse';
  return 'other';
}

function headerContentType(headers) {
  if (!headers) return '';
  if (typeof headers === 'string') return headers;
  if (typeof headers.get === 'function') return headers.get('content-type') ?? '';
  return headers['content-type'] ?? headers['Content-Type'] ?? '';
}

function candidateMessages(parsed) {
  if (Array.isArray(parsed)) return parsed;
  if (parsed && typeof parsed === 'object') return [parsed];
  return [];
}

function shapeOf(message, expectedId) {
  const id = message?.id;
  const idType = typeof id === 'number' ? 'number' : typeof id === 'string' ? 'string' : id === undefined ? 'missing' : 'other';
  const code = message && typeof message === 'object' && message.error && typeof message.error === 'object'
    ? message.error.code
    : undefined;
  const errorCode = Number.isSafeInteger(code) ? code : null;
  let errorClass = 'none';
  if (message && typeof message === 'object' && 'error' in message && message.error !== undefined && message.error !== null) {
    errorClass = message.error?.message === DEVICE_UNAVAILABLE_TEXT ? 'device-unavailable' : 'other';
  }
  return {
    idType,
    idMatches: typeof id === 'number' && id === expectedId,
    resultPresent: !!message && typeof message === 'object' && 'result' in message && message.result !== undefined,
    errorCode,
    errorClass,
  };
}

// response: { status, headers|contentType, text }. Never reads raw headers beyond content-type.
export function diagnoseMcpResponse(response, expectedId) {
  const status = typeof response?.status === 'number' ? response.status : null;
  const kind = contentKind(headerContentType(response?.headers ?? response?.contentType));
  const text = typeof response?.text === 'string' ? response.text : '';
  const bodyBytes = Buffer.byteLength(text, 'utf8');
  let parseOk = false;
  let shapes = [];
  try {
    if (kind === 'sse') {
      const payloads = text.split('\n')
        .filter(line => line.startsWith('data:'))
        .map(line => line.slice(5).trim())
        .filter(Boolean);
      const messages = [];
      for (const payload of payloads) messages.push(...candidateMessages(JSON.parse(payload)));
      shapes = messages.slice(0, MAX_MESSAGES).map(message => shapeOf(message, expectedId));
      parseOk = payloads.length > 0;
    } else {
      const messages = candidateMessages(JSON.parse(text));
      shapes = messages.slice(0, MAX_MESSAGES).map(message => shapeOf(message, expectedId));
      parseOk = true;
    }
  } catch {
    parseOk = false;
    shapes = [];
  }
  return { status, contentType: kind, bodyBytes, parseOk, messages: shapes };
}

// Sanitized current process facts for owned known tasks only. No output tails.
export function sanitizeTasks(tasks) {
  const names = new Set(['daemon', 'MCP', 'tunnel', 'reverse-helper']);
  const signals = new Set(['SIGTERM', 'SIGKILL', 'SIGINT', 'SIGABRT', 'SIGSEGV', 'SIGBUS', 'SIGPIPE']);
  return (Array.isArray(tasks) ? tasks : []).filter(Boolean).slice(0, 4).map(task => ({
    name: names.has(task.name) ? task.name : 'unknown',
    exitCode: Number.isSafeInteger(task.child?.exitCode) ? task.child.exitCode : null,
    signalCode: signals.has(task.child?.signalCode) ? task.child.signalCode : null,
    failed: task.failed === true,
    reverseReady: task.reverseReady === true,
  }));
}
