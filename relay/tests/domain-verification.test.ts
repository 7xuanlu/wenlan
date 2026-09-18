// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { domainVerification } from '../src/domain-verification.ts';

const challengePath = 'https://relay.example/.well-known/openai-apps-challenge';
const request = (url = challengePath, init?: RequestInit) => new Request(url, init);

async function responseBytes(response: Response) {
  return new Uint8Array(await response.arrayBuffer());
}

test('returns the exact challenge bytes with the required headers', async () => {
  const token = '!AZaz09~';
  const response = domainVerification(request(), token);

  assert.ok(response);
  assert.equal(response.status, 200);
  assert.deepEqual(await responseBytes(response), new TextEncoder().encode(token));
  assert.equal(response.headers.get('content-type'), 'text/plain;charset=utf-8');
  assert.equal(response.headers.get('cache-control'), 'no-store');
  assert.equal(response.headers.get('x-content-type-options'), 'nosniff');
});

test('returns 404 for absent and invalid challenge values', () => {
  for (const token of [undefined, null, '', 1, {}, ' ', '\t', '\n', 'a'.repeat(4097), 'é', '\u0000', 'a b']) {
    const response = domainVerification(request(), token);
    assert.ok(response);
    assert.equal(response.status, 404, `token=${String(token)}`);
  }
});

test('accepts the printable ASCII length boundaries', async () => {
  for (const token of ['!', '~'.repeat(4096)]) {
    const response = domainVerification(request(), token);
    assert.ok(response);
    assert.equal(response.status, 200);
    assert.deepEqual(await responseBytes(response), new TextEncoder().encode(token));
  }
});

test('only GET serves the exact pathname', () => {
  for (const method of ['POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS']) {
    const response = domainVerification(request(challengePath, { method }), 'valid-token');
    assert.ok(response);
    assert.equal(response.status, 405, method);
    assert.equal(response.headers.get('allow'), 'GET');
  }

  for (const url of [
    'https://relay.example/.well-known/openai-apps-challenge/',
    'https://relay.example/.well-known/openai-apps-challenge/extra',
    'https://relay.example/.well-known/openai-apps-challenge%2F',
    'https://relay.example/.well-known/openai-apps-challenge.json',
    'https://relay.example/other/.well-known/openai-apps-challenge',
    'https://relay.example/.well-known/openai-apps-challengeish',
  ]) {
    assert.equal(domainVerification(request(url), 'valid-token'), null, url);
  }

  assert.equal(domainVerification(request('https://relay.example/.well-known/openai-apps-challenge?token=request-value'), 'valid-token')?.status, 200);
  assert.equal(domainVerification(request('http://untrusted.example/.well-known/openai-apps-challenge'), 'valid-token')?.status, 200);
});

test('does not reflect request values in the successful body', async () => {
  const response = domainVerification(
    request('https://relay.example/.well-known/openai-apps-challenge?secret=request-value', {
      headers: { 'x-request-value': 'header-value' },
    }),
    'operator-value',
  );

  assert.ok(response);
  const body = new TextDecoder().decode(await responseBytes(response));
  assert.equal(body, 'operator-value');
  assert.equal(body.includes('request-value'), false);
  assert.equal(body.includes('header-value'), false);
});
