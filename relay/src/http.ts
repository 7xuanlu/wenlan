// SPDX-License-Identifier: Apache-2.0
import { authenticateDevice, enrollDevice, refreshDevice, revokeDevice, rotateDeviceCredential } from './devices.ts';
import { approvePairing, browserPairingView, cancelPairing, inspectPairing, type PairingStore } from './pairing.ts';
import { finishOAuthPairing, startOAuthPairing, type OAuthEnv } from './oauth.ts';
import { AuthorityCapacityError } from './bounded-store.ts';
import { validSecret } from './secrets.ts';
import { listDeviceGrants, revokeDeviceGrant, validGrantId } from './grants.ts';
import { authorizationRead } from './proxy.ts';
import { approveSamplePairing, parseSampleAccount, type SampleAccount } from './sample-account.ts';
import { prepareReverseDevice } from './reverse-devices.ts';
export { pairingCSS } from './pairing-style.ts';
export { pairingJS } from './pairing-script.ts';

export interface PublicEnv extends OAuthEnv {
  SAMPLE_ACCOUNT?: string;
}

const COOKIE = '__Host-wenlan-pairing';
export class HttpFailure extends Error {
  constructor(public status: number, message: string) { super(message); }
}
export function failure(status: number, error: string): Response {
  return Response.json({ error }, { status, headers: { 'cache-control': 'no-store' } });
}

/** Buffer before the OAuth library or application parser. Never let a partial
 * or indefinitely streaming body hold storage/auth work open.
 */
export async function boundedRequest(request: Request, limit: number): Promise<Request> {
  const length = request.headers.get('content-length');
  if (length !== null && (!/^\d+$/.test(length) || Number(length) > limit)) throw new HttpFailure(413, 'Request too large');
  if (!request.body) return request;
  const reader = request.body.getReader();
  const deadline = AbortSignal.any([request.signal, AbortSignal.timeout(5000)]);
  const cancel = () => { void reader.cancel().catch(() => {}); };
  deadline.addEventListener('abort', cancel, { once: true });
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    while (true) {
      if (deadline.aborted) throw new HttpFailure(408, 'Request timed out');
      const { value, done } = await reader.read();
      if (deadline.aborted) throw new HttpFailure(408, 'Request timed out');
      if (done) break;
      size += value.byteLength;
      if (size > limit) throw new HttpFailure(413, 'Request too large');
      chunks.push(value);
    }
  } finally {
    deadline.removeEventListener('abort', cancel);
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  return new Request(request, { body: bytes });
}

async function jsonObject(request: Request): Promise<Record<string, unknown>> {
  if (request.headers.get('content-type')?.split(';')[0].trim() !== 'application/json') {
    throw new HttpFailure(415, 'JSON required');
  }
  let value;
  try { value = await request.json(); } catch { throw new HttpFailure(400, 'Invalid JSON'); }
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new HttpFailure(400, 'JSON object required');
  return value as Record<string, unknown>;
}
function stringField(body: Record<string, unknown>, name: string, max = 2048): string {
  const value = body[name];
  if (typeof value !== 'string' || !value || value.length > max) throw new HttpFailure(400, 'Invalid request fields');
  return value;
}
function candidate(body: Record<string, unknown>) {
  return { tunnelOrigin: stringField(body, 'tunnelOrigin'),
    backendToken: stringField(body, 'backendToken', 128), space: stringField(body, 'space', 256) };
}
export function deviceCredential(request: Request) {
  // Native desktop APIs do not use cookies or accept browser-origin requests.
  if (request.headers.has('origin') || request.headers.has('cookie')) throw new HttpFailure(403, 'Native device request required');
  const id = request.headers.get('x-wenlan-device-id') ?? '';
  const header = request.headers.get('authorization') ?? '';
  const token = header.startsWith('Bearer ') ? header.slice(7) : '';
  if (!validSecret(id) || !validSecret(token)) throw new HttpFailure(401, 'Device authentication required');
  return { id, token };
}
function browserCookie(request: Request) {
  const cookies = (request.headers.get('cookie') ?? '').split(';').map(value => value.trim())
    .filter(value => value.startsWith(`${COOKIE}=`));
  if (cookies.length !== 1) return null;
  const parts = cookies[0].slice(COOKIE.length + 1).split('.');
  const [id, secret] = parts;
  return parts.length === 2 && validSecret(id) && validSecret(secret) ? { id, secret } : null;
}
function setCookie(id: string, secret: string, maxAge = 300) {
  return `${COOKIE}=${id}.${secret}; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=${maxAge}`;
}
const clearCookie = () => `${COOKIE}=; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=0`;
function htmlEscape(value: string): string {
  return value.replace(/[&<>"']/g, character => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[character]!);
}

function configuredSampleAccount(env: PublicEnv, publicOrigin: string): SampleAccount | null {
  if (typeof env.SAMPLE_ACCOUNT !== 'string' || env.SAMPLE_ACCOUNT.length > 4096) return null;
  try {
    const account = parseSampleAccount(JSON.parse(env.SAMPLE_ACCOUNT));
    return account && account.expiresAt > Date.now() && account.resource === `${publicOrigin}/mcp` ? account : null;
  } catch { return null; }
}

function sameOrigin(request: Request, publicOrigin: string) {
  if (request.headers.get('origin') !== publicOrigin
    || (request.headers.has('sec-fetch-site') && request.headers.get('sec-fetch-site') !== 'same-origin')) {
    throw new HttpFailure(403, 'Same-origin request required');
  }
}

function pairingPage(view: { pairingId: string; clientId: string; status: string; space?: string } | null, sampleAvailable = false): Response {
  const approved = view?.status === 'approved';
  const state = view ? approved ? 'approved' : 'pending' : 'unavailable';
  const body = view ? `<p class="intro">Your knowledge stays on your device. You choose what this connection can access.</p>
<p class="status" data-state="${state}" role="status" aria-live="polite"><span class="status-dot" aria-hidden="true"></span><span id="pairing-status">${approved ? 'Approved on your device' : 'Waiting for device approval'}</span></p>
<section class="pairing-step"${approved ? ' hidden' : ''}><h2>Approve in Wenlan</h2>
<p>Open <strong>Settings &gt; Connections</strong> in the Wenlan app. Paste this code into <strong>Authorize a connection</strong>, review the request, then approve.</p>
<label for="pairing-code">Pairing code</label><div class="code-row"><textarea id="pairing-code" readonly rows="2" spellcheck="false">${htmlEscape(view.pairingId)}</textarea>
<button id="copy-code" type="button">Copy code</button></div></section>
<dl id="approved-space"${approved ? '' : ' hidden'}><dt>Authorized Space</dt><dd>${htmlEscape(view.space ?? '')}</dd></dl>
<section class="permissions"><h2>This connection can</h2><ul><li>Read Briefs, search knowledge, and inspect sources in the Space you approve.</li>
<li>Record searches and access activity locally. Other Spaces stay private.</li></ul>
<p>Requests and results pass through wenlan-relay. Keep Wenlan running and your device online. Revoke access anytime in Connections.</p></section>
<details class="client-details"><summary>Connection details</summary><dl><dt>Client ID</dt><dd>${htmlEscape(view.clientId)}</dd></dl></details>
<p id="notice" role="status" aria-live="polite">${approved ? 'Ready. Continue to your AI client.' : 'This page updates after you approve in Wenlan.'}</p>
<div class="actions"><form action="/pairing/complete" method="post"><button id="continue" type="submit" class="primary"${approved ? '' : ' disabled'}>Continue</button></form>
<form action="/pairing/cancel" method="post"><button type="submit">Cancel</button></form></div>
${sampleAvailable && view.status === 'pending' ? '<a class="sample-link" href="/pairing/sample">Connect a sample library</a>' : ''}`
    : '<p class="status" data-state="unavailable">This pairing is no longer available.</p><p class="intro">Return to your AI client and start a new connection.</p>';
  return authorizationPage(body, state);
}

function samplePage(clientId: string, account: SampleAccount): Response {
  return authorizationPage(`<h2>Sample library</h2>
<dl><dt>Client</dt><dd>${htmlEscape(clientId)}</dd><dt>Space</dt><dd>${htmlEscape(account.space)}</dd>
<dt>Permission</dt><dd>Read Briefs, search knowledge and inspect supporting sources.</dd></dl>
<form id="sample-login" action="/pairing/sample" method="post" data-client-id="${htmlEscape(clientId)}" data-resource="${htmlEscape(account.resource)}" data-space="${htmlEscape(account.space)}">
<label for="sample-username">Username</label><input id="sample-username" name="username" autocomplete="username" maxlength="64" required>
<label for="sample-password">Password</label><input id="sample-password" name="password" type="password" autocomplete="current-password" maxlength="64" required>
<label class="consent"><input name="approved" type="checkbox" required><span>Allow access to this Space. Searches are recorded in its activity history.</span></label>
<button type="submit" class="primary">Sign in and authorize</button></form>
<p id="notice" role="status" aria-live="polite"></p><a href="/pairing">Back to device pairing</a>`);
}

function authorizationPage(body: string, state = 'sample'): Response {
  return new Response(`<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Connect Wenlan</title><link rel="icon" href="/icon.png"><link rel="stylesheet" href="/pairing.css"><script src="/pairing.js" defer></script></head>
<body><main data-pairing-state="${state}"><header class="brand"><img src="/icon.png" width="32" height="32" alt=""><span>Wenlan</span></header><h1>Connect Wenlan</h1>${body}
<noscript><p>JavaScript is required to complete this connection.</p></noscript>
<footer><a href="https://wenlan.app/docs/data-and-privacy" rel="noreferrer">Privacy</a><a href="https://wenlan.app/terms" rel="noreferrer">Terms</a><a href="https://wenlan.app" rel="noreferrer">Wenlan</a></footer></main></body></html>`, {
    headers: { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store',
      'content-security-policy': "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'",
      'referrer-policy': 'no-referrer', 'x-content-type-options': 'nosniff', 'x-frame-options': 'DENY' },
  });
}

export async function handlePublicRequest(
  request: Request, env: PublicEnv, store: PairingStore, publicOrigin: string,
  revokeTokens: (grantId: string, subject: string) => Promise<void>,
): Promise<Response> {
  const url = new URL(request.url);
  if (url.pathname === '/pairing/status' && request.method === 'GET') {
    if ((request.headers.has('origin') && request.headers.get('origin') !== publicOrigin)
      || request.headers.get('sec-fetch-site') === 'cross-site') throw new HttpFailure(403, 'Same-origin request required');
    const cookie = browserCookie(request);
    const view = cookie ? await browserPairingView(store, cookie.id, cookie.secret) : null;
    return Response.json(view ? { status: view.status, ...(view.space ? { space: view.space } : {}) }
      : { status: 'unavailable' }, { headers: { 'cache-control': 'no-store', 'x-content-type-options': 'nosniff' } });
  }
  if (url.pathname === '/authorize' && request.method === 'GET') {
    let pair;
    try { pair = await startOAuthPairing(env.OAUTH_PROVIDER, store, request, publicOrigin); }
    catch (error) {
      if (error instanceof AuthorityCapacityError) throw error;
      throw new HttpFailure(400, 'Authorization request rejected');
    }
    return new Response(null, { status: 303, headers: {
      location: '/pairing', 'cache-control': 'no-store',
      'set-cookie': setCookie(pair.pairingId, pair.browserSecret),
    } });
  }
  if (url.pathname === '/pairing' && request.method === 'GET') {
    const cookie = browserCookie(request);
    return pairingPage(cookie ? await browserPairingView(store, cookie.id, cookie.secret) : null,
      configuredSampleAccount(env, publicOrigin) !== null);
  }
  if (url.pathname === '/pairing/sample') {
    const account = configuredSampleAccount(env, publicOrigin);
    if (!account) return failure(404, 'Sample library unavailable');
    const cookie = browserCookie(request);
    if (request.method === 'GET') {
      const view = cookie ? await browserPairingView(store, cookie.id, cookie.secret) : null;
      return view?.status === 'pending' ? samplePage(view.clientId, account) : failure(401, 'Active pairing required');
    }
    if (request.method === 'POST') {
      sameOrigin(request, publicOrigin);
      const body = await jsonObject(request);
      if (!cookie) return failure(401, 'Sample account sign-in rejected');
      const fields = ['username', 'password', 'clientId', 'resource', 'space'];
      if (Object.keys(body).some(key => ![...fields, 'approved'].includes(key))
        || fields.some(key => typeof body[key] !== 'string' || (body[key] as string).length > 2048)) {
        return failure(401, 'Sample account sign-in rejected');
      }
      const approved = await approveSamplePairing(store, account, {
        username: body.username as string, password: body.password as string,
        clientId: body.clientId as string, resource: body.resource as string, space: body.space as string,
        approved: body.approved === true, pairingId: cookie.id, browserSecret: cookie.secret,
      });
      if (!approved) return failure(401, 'Sample account sign-in rejected');
      const redirectTo = await finishOAuthPairing(env.OAUTH_PROVIDER, store, cookie.id, cookie.secret, publicOrigin);
      return redirectTo ? Response.json({ redirectTo }, { headers: { 'cache-control': 'no-store', 'set-cookie': clearCookie() } })
        : failure(409, 'Authorization could not be completed');
    }
    return failure(405, 'Method not allowed');
  }
  if (['/pairing/complete', '/pairing/cancel'].includes(url.pathname) && request.method === 'POST') {
    sameOrigin(request, publicOrigin);
    await jsonObject(request);
    const cookie = browserCookie(request);
    if (!cookie) throw new HttpFailure(401, 'Pairing cookie required');
    if (url.pathname.endsWith('/complete')) {
      const redirectTo = await finishOAuthPairing(env.OAUTH_PROVIDER, store, cookie.id, cookie.secret, publicOrigin);
      if (redirectTo) {
        const response = Response.json({ redirectTo }, { headers: { 'cache-control': 'no-store' } });
        response.headers.set('set-cookie', clearCookie());
        return response;
      }
      // Failed completion keeps 409 for compatibility, but must not report
      // every failure as still pending. The cookie-authenticated view
      // distinguishes pending approval from an unavailable pairing without
      // exposing why the view failed.
      const view = await browserPairingView(store, cookie.id, cookie.secret);
      const error = view?.status === 'pending'
        ? 'Device approval is still pending.'
        : view?.status === 'approved'
          ? 'Authorization could not be completed. Start a new connection in your AI client.'
          : 'This pairing has expired or is no longer available. Start a new connection in your AI client.';
      return Response.json({ redirectTo: null, error, ...(view?.status !== 'pending' ? { pairingUnavailable: true } : {}) },
        { status: 409, headers: { 'cache-control': 'no-store' } });
    }
    if (await cancelPairing(store, cookie.id, cookie.secret)) {
      const response = Response.json({ cancelled: true }, { headers: { 'cache-control': 'no-store' } });
      response.headers.set('set-cookie', clearCookie());
      return response;
    }
    return Response.json({ cancelled: false, pairingUnavailable: true,
      error: 'This pairing has expired or is no longer available. Start a new connection in your AI client.' },
    { status: 409, headers: { 'cache-control': 'no-store' } });
  }
  if (url.pathname === '/devices/reverse' && request.method === 'POST') {
    if (request.headers.has('origin') || request.headers.has('cookie')) throw new HttpFailure(403, 'Native device request required');
    const body = await jsonObject(request);
    if (Object.keys(body).some(key => !['backendToken', 'space'].includes(key))) throw new HttpFailure(400, 'Invalid request fields');
    const credential = await prepareReverseDevice(store, {
      backendToken: stringField(body, 'backendToken', 128), space: stringField(body, 'space', 256),
    });
    return credential ? Response.json(credential, { status: 201, headers: { 'cache-control': 'no-store' } })
      : failure(422, 'Invalid connector contract');
  }
  if (url.pathname === '/devices' && request.method === 'POST') {
    if (request.headers.has('origin') || request.headers.has('cookie')) throw new HttpFailure(403, 'Native device request required');
    const credential = await enrollDevice(store, candidate(await jsonObject(request)));
    return credential ? Response.json(credential, { status: 201, headers: { 'cache-control': 'no-store' } })
      : failure(422, 'Protected connector could not be verified');
  }
  if (['/devices/refresh', '/devices/renew', '/devices/rotate', '/devices/revoke'].includes(url.pathname) && request.method === 'POST') {
    const { id, token } = deviceCredential(request);
    const body = await jsonObject(request);
    if (url.pathname.endsWith('/rotate')) {
      const credential = await rotateDeviceCredential(store, id, token);
      return credential ? Response.json(credential, { headers: { 'cache-control': 'no-store' } }) : failure(401, 'Device authentication failed');
    }
    const renew = url.pathname.endsWith('/renew');
    const refresh = renew || url.pathname.endsWith('/refresh');
    const expectedGeneration = body.expectedGeneration;
    if (renew && (typeof expectedGeneration !== 'number' || !Number.isSafeInteger(expectedGeneration) || expectedGeneration < 0)) {
      throw new HttpFailure(400, 'Invalid request fields');
    }
    const success = refresh
      ? await refreshDevice(store, id, token, candidate(body),
        { expectedGeneration: renew ? expectedGeneration as number : undefined })
      : await revokeDevice(store, id, token);
    return success ? Response.json({ success }, { headers: { 'cache-control': 'no-store' } }) : failure(401, 'Device operation rejected');
  }
  if (url.pathname === '/grants' && request.method === 'GET') {
    const { id, token } = deviceCredential(request);
    const cursor = url.searchParams.get('cursor') ?? undefined;
    if (cursor !== undefined && !validGrantId(cursor)) throw new HttpFailure(400, 'Invalid cursor');
    const page = await listDeviceGrants(store, id, token, cursor);
    return page ? Response.json(page, { headers: { 'cache-control': 'no-store' } }) : failure(401, 'Device authentication failed');
  }
  const grantMatch = /^\/grants\/([A-Za-z0-9_-]{16,128})\/revoke$/.exec(url.pathname);
  if (grantMatch && request.method === 'POST') {
    const { id, token } = deviceCredential(request);
    await jsonObject(request);
    const result = await revokeDeviceGrant(store, id, token, grantMatch[1],
      (grantId, subject) => authorizationRead(() => revokeTokens(grantId, subject)));
    return result ? Response.json(result, { status: result.cleanupPending ? 503 : 200,
      headers: { 'cache-control': 'no-store' } }) : failure(404, 'Connection unavailable');
  }
  const pairMatch = /^\/pairings\/([A-Za-z0-9_-]{32,128})(\/approve)?$/.exec(url.pathname);
  if (pairMatch && ((request.method === 'GET' && !pairMatch[2]) || (request.method === 'POST' && pairMatch[2]))) {
    const { id, token } = deviceCredential(request);
    const identity = await authenticateDevice(store, id, token);
    if (!identity) throw new HttpFailure(401, 'Device authentication failed');
    if (request.method === 'GET') {
      const view = await inspectPairing(store, pairMatch[1]);
      return view ? Response.json(view, { headers: { 'cache-control': 'no-store' } }) : failure(404, 'Pairing unavailable');
    }
    const body = await jsonObject(request);
    if (body.approved !== true) throw new HttpFailure(400, 'Explicit consent required');
    const success = await approvePairing(store, pairMatch[1], identity, {
      clientId: stringField(body, 'clientId'), resource: stringField(body, 'resource'), space: stringField(body, 'space', 256),
    });
    return success ? Response.json({ approved: true }, { headers: { 'cache-control': 'no-store' } }) : failure(409, 'Pairing approval rejected');
  }
  return failure(404, 'Not found');
}
