// SPDX-License-Identifier: Apache-2.0
// Synthetic UI controls exist ONLY in this local fixture, not the real Worker.
import { handlePublicRequest, pairingCSS, pairingJS } from '../../src/http.ts';
import { beginPairing, approvePairing, connectorKey, cancelPairing } from '../../src/pairing.ts';
import { MemoryStore } from './memory-store.ts';
import icon from '../../../app/icons/128x128.png';

const origin = 'https://relay.example';
const store = new MemoryStore();
let pair: Awaited<ReturnType<typeof beginPairing>>;
let unavailable = false;
const device = { id: 'a'.repeat(64), subject: 'synthetic-reviewer', generation: 1, credentialExpiresAt: 0 };
async function reset() {
  unavailable = false;
  device.credentialExpiresAt = Date.now() + 3600000;
  await store.transaction(tx => tx.put(connectorKey(device.id), { ...device, space: 'atlas-review', enabled: true,
    expiresAt: device.credentialExpiresAt, backendToken: 'b'.repeat(64), tunnelOrigin: 'https://synthetic.trycloudflare.com' }));
  pair = await beginPairing(store, { authorizationId: crypto.randomUUID(), clientId: 'synthetic-client-r22',
    resource: origin + '/mcp', scopes: ['wenlan:query'] }, origin + '/mcp');
}
export default { async fetch(req: Request) {
  if (!pair) await reset();
  const url = new URL(req.url);
  if (req.headers.has('origin') && req.headers.get('origin') !== url.origin) return new Response(null, { status: 403 });
  if (url.pathname.startsWith('/__fixture/')) {
    if (url.pathname === '/__fixture/mobile' || url.pathname === '/__fixture/dark') {
      const dark = url.pathname.endsWith('/dark');
      return new Response('<!doctype html><meta name="viewport" content="width=device-width,initial-scale=1"><title>Responsive authorization fixture</title>'
        + '<iframe title="Authorization preview" src="/pairing' + (dark ? '?theme=dark' : '')
        + '" style="display:block;width:' + (dark ? '560' : '320') + 'px;height:740px;border:0"></iframe>',
      { headers: { 'content-type': 'text/html' } });
    }
    if (url.pathname === '/__fixture/pending') await reset();
    else if (url.pathname === '/__fixture/approve') await approvePairing(store, pair.pairingId, device,
      { clientId: 'synthetic-client-r22', resource: origin + '/mcp', space: 'atlas-review' });
    else if (url.pathname === '/__fixture/expire') await cancelPairing(store, pair.pairingId, pair.browserSecret);
    else if (url.pathname === '/__fixture/error') unavailable = true;
    else if (url.pathname === '/__fixture/recover') unavailable = false;
    return new Response('<a href="/pairing">View pairing</a>', { headers: { 'content-type': 'text/html' } });
  }
  if (url.pathname === '/pairing.css') return new Response(url.searchParams.get('theme') === 'dark'
    ? pairingCSS.replace('@media(prefers-color-scheme:dark)', '@media all') : pairingCSS,
  { headers: { 'content-type': 'text/css' } });
  if (url.pathname === '/pairing.js') return new Response(pairingJS, { headers: { 'content-type': 'text/javascript' } });
  if (url.pathname === '/icon.png') return new Response(icon, { headers: { 'content-type': 'image/png' } });
  if (url.pathname === '/pairing/status' && unavailable) return new Response(null, { status: 503 });
  if (url.pathname === '/pairing/complete') return Response.json({ redirectTo: '/done' });
  if (url.pathname === '/done') return new Response('<h1>Synthetic callback completed</h1>', { headers: { 'content-type': 'text/html' } });
  if (!['/pairing', '/pairing/status', '/pairing/cancel'].includes(url.pathname)) return new Response(null, { status: 404 });
  const request = new Request(origin + url.pathname, { method: req.method, headers: {
    cookie: '__Host-wenlan-pairing=' + pair.pairingId + '.' + pair.browserSecret, origin, 'content-type': 'application/json',
  }, ...(req.method === 'POST' ? { body: '{}' } : {}) });
  const response = await handlePublicRequest(request, {} as never, store, origin, async () => {});
  if (url.pathname === '/pairing') {
    const headers = new Headers(response.headers);
    // Only the loopback preview permits its own iframe. Production remains DENY.
    headers.set('content-security-policy', headers.get('content-security-policy')!.replace("frame-ancestors 'none'", "frame-ancestors 'self'"));
    headers.delete('x-frame-options');
    let html = await response.text();
    if (url.searchParams.get('theme') === 'dark') html = html.replace('href="/pairing.css"', 'href="/pairing.css?theme=dark"');
    return new Response(html, { status: response.status, headers });
  }
  return response;
} };
