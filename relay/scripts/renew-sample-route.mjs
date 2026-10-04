// SPDX-License-Identifier: Apache-2.0
import { parseArgs } from 'node:util';
import { pathToFileURL } from 'node:url';
import { readPrivateJson } from './private-json.mjs';
import { parseSampleAccount } from '../src/sample-account.ts';
import { hashSecret, sameHash, validSecret } from '../src/secrets.ts';
import { tunnelOrigin } from '../src/proxy.ts';

/** One authenticated attempt, never enrollment, credential rotation or a loop.
 * Dependencies are injectable solely for isolated transport/deadline tests.
 */
export async function renewSampleRoute(input, { fetcher = fetch, clock = Date.now,
  signal = AbortSignal.timeout(10_000) } = {}) {
  let response;
  try {
    const { enrollment, connector, account: rawAccount, relayOrigin, nextTunnelOrigin } = input;
    const account = parseSampleAccount(rawAccount);
    const now = clock();
    const relay = new URL(relayOrigin);
    if (!account || !Number.isSafeInteger(now) || now <= 0 || account.expiresAt <= now
      || account.generation !== 0 || relay.protocol !== 'https:' || relayOrigin !== relay.origin
      || account.resource !== `${relayOrigin}/mcp`
      || !enrollment || Object.keys(enrollment).sort().join(',') !== 'expiresAt,id,managementToken'
      || !validSecret(enrollment.id) || !validSecret(enrollment.managementToken)
      || !Number.isSafeInteger(enrollment.expiresAt) || enrollment.expiresAt <= now
      || account.expiresAt > enrollment.expiresAt || account.deviceId !== enrollment.id
      || !sameHash(account.managementHash, await hashSecret(enrollment.managementToken))
      || !connector || Object.keys(connector).sort().join(',') !== 'backendToken,space,tunnelOrigin'
      || typeof connector.tunnelOrigin !== 'string' || tunnelOrigin(connector.tunnelOrigin) !== connector.tunnelOrigin
      || typeof nextTunnelOrigin !== 'string' || tunnelOrigin(nextTunnelOrigin) !== nextTunnelOrigin
      || !validSecret(connector.backendToken) || connector.space !== account.space
      || signal.aborted) return 'invalid';
    // A dedicated endpoint fails closed on older Workers instead of having
    // them silently ignore a new precondition on their existing refresh API.
    response = await fetcher(`${relayOrigin}/devices/renew`, {
      method: 'POST', redirect: 'manual', credentials: 'omit', signal,
      headers: { 'content-type': 'application/json', accept: 'application/json',
        authorization: `Bearer ${enrollment.managementToken}`, 'x-wenlan-device-id': enrollment.id },
      body: JSON.stringify({ tunnelOrigin: nextTunnelOrigin, backendToken: connector.backendToken,
        space: connector.space, expectedGeneration: account.generation }),
    });
    if (response.status !== 200) return 'rejected';
    if (response.headers.get('content-type')?.split(';')[0].trim() !== 'application/json') return 'uncertain';
    const reader = response.body?.getReader();
    if (!reader) return 'uncertain';
    const abort = () => { void reader.cancel().catch(() => {}); };
    signal.addEventListener('abort', abort, { once: true });
    const bytes = new Uint8Array(4096);
    let length = 0;
    try {
      while (true) {
        if (signal.aborted) return 'uncertain';
        const { value, done } = await reader.read();
        if (signal.aborted) return 'uncertain';
        if (done) break;
        if (length + value.byteLength > bytes.length) return 'uncertain';
        bytes.set(value, length); length += value.byteLength;
      }
      const body = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(0, length)));
      return body?.success === true && Object.keys(body).join(',') === 'success' ? 'renewed' : 'uncertain';
    } finally {
      bytes.fill(0);
      signal.removeEventListener('abort', abort);
      await reader.cancel().catch(() => {});
      reader.releaseLock();
    }
  } catch { return 'uncertain'; }
  finally { if (response?.body && !response.body.locked) await response.body.cancel().catch(() => {}); }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const { values } = parseArgs({ options: {
      enrollment: { type: 'string' }, connector: { type: 'string' }, account: { type: 'string' },
      'relay-origin': { type: 'string' }, 'tunnel-origin': { type: 'string' },
    }, strict: true, allowPositionals: false });
    const result = await renewSampleRoute({ enrollment: await readPrivateJson(values.enrollment),
      connector: await readPrivateJson(values.connector), account: await readPrivateJson(values.account),
      relayOrigin: values['relay-origin'], nextTunnelOrigin: values['tunnel-origin'] });
    if (result !== 'renewed') throw new Error('renewal not confirmed');
    process.stdout.write('Route renewed with the existing credential and consent boundary. No enrollment or rotation performed.\n');
  } catch {
    process.stderr.write('Renewal not confirmed. Check private inputs, credential/account expiry and relay availability. No automatic retry, enrollment or rotation performed; do not infer the route is offline from a lost response.\n');
    process.exitCode = 1;
  }
}
