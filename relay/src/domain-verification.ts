// SPDX-License-Identifier: Apache-2.0

const CHALLENGE_PATH = '/.well-known/openai-apps-challenge';

function validToken(token: unknown): token is string {
  if (typeof token !== 'string' || token.length < 1 || token.length > 4096) return false;
  for (let index = 0; index < token.length; index++) {
    const code = token.charCodeAt(index);
    if (code < 0x21 || code > 0x7e) return false;
  }
  return true;
}

export function domainVerification(request: Request, token: unknown): Response | null {
  let pathname: string;
  try {
    pathname = new URL(request.url).pathname;
  } catch {
    return null;
  }

  if (pathname !== CHALLENGE_PATH) return null;
  if (request.method !== 'GET') return new Response(null, { status: 405, headers: { allow: 'GET' } });
  if (!validToken(token)) return new Response(null, { status: 404 });

  return new Response(token, { headers: {
    'content-type': 'text/plain;charset=utf-8',
    'cache-control': 'no-store',
    'x-content-type-options': 'nosniff',
  } });
}
