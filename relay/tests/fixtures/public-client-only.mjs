// SPDX-License-Identifier: Apache-2.0
// Attended Chat-only (public-client-only) fixture helper for
// relay/tests/public-native-check.mjs. Test-only; not a product API.
// Seeds one fresh random synthetic note through the EXISTING daemon
// POST /api/memory/store endpoint (same contract as reviewer-api-seed.mjs).
// Never touches credentials, enrollment, pairing, or cleanup.
import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import { checkDaemonUrl } from './reviewer-api-seed.mjs';

const SPACE = 'atlas-review';
const AGENT = 'reviewer-real-backend';
const TIMEOUT_MS = 10_000;
const ID_RE = /^[A-Za-z0-9][A-Za-z0-9_-]{0,127}$/;
const HEX_RE = /^[a-f0-9]+$/;

function hex(bytes = 8, random = randomBytes) {
  const value = random(bytes).toString('hex');
  assert.match(value, HEX_RE, 'random fixture hex required');
  return value;
}

// Rejects misuse of WENLAN_PUBLIC_CLIENT_ONLY=1. Returns true when enabled.
export function validatePublicClientOnlyEnv(env = process.env, { transport, attendedRecovery } = {}) {
  if (env.WENLAN_PUBLIC_CLIENT_ONLY === undefined) return false;
  assert.equal(env.WENLAN_PUBLIC_CLIENT_ONLY, '1', 'WENLAN_PUBLIC_CLIENT_ONLY must be absent or exactly 1');
  assert.equal(transport, 'reverse', 'public-client-only mode requires the reverse transport');
  assert.match(env.WENLAN_PORTAL_PAIRING_ID ?? '', /^[a-f0-9]{64}$/,
    'public-client-only mode requires an attended reverse portal pairing');
  assert(typeof env.WENLAN_PORTAL_CLIENT_ID === 'string' && env.WENLAN_PORTAL_CLIENT_ID.length > 0
    && env.WENLAN_PORTAL_CLIENT_ID.length <= 2048,
    'public-client-only mode requires an attended reverse portal client');
  assert.equal(env.WENLAN_CODEX_BIN, undefined, 'public-client-only mode has no Codex client lane');
  assert.equal(env.WENLAN_CODEX_MODEL_HOME, undefined, 'public-client-only mode has no Codex model lane');
  assert(!attendedRecovery, 'public-client-only mode has no attended recovery lane');
  assert.notEqual(env.WENLAN_TEST_OFFLINE_RECOVERY, '1', 'public-client-only mode has no offline recovery lane');
  return true;
}

// Creates one fresh random synthetic memory in atlas-review. The returned
// label is safe as an unpredictable ChatGPT query label; the answer nonce is
// synthetic evidence that must NOT be included in the ChatGPT prompt.
export async function createClientFixture({ daemonUrl, fetchImpl = globalThis.fetch, random = randomBytes }) {
  const origin = checkDaemonUrl(daemonUrl);
  const label = `chat-only probe ${hex(8, random)}`;
  const answer = `synthetic answer ${hex(16, random)}`;
  let res;
  try {
    res = await fetchImpl(`${origin}/api/memory/store`, {
      method: 'POST',
      redirect: 'error',
      signal: AbortSignal.timeout(TIMEOUT_MS),
      headers: { 'content-type': 'application/json', 'X-Wenlan-Space': SPACE },
      body: JSON.stringify({
        content: `Synthetic Chat-only probe ${label}: expected marker ${answer}.`,
        title: label,
        memory_type: 'decision',
        space: SPACE,
        source_agent: AGENT,
      }),
    });
  } catch (error) {
    assert.fail(`client fixture store request failed: ${error?.message ?? error}`);
  }
  assert(res.ok, `client fixture store returned ${res.status}`);
  const body = await res.json();
  assert.equal(body?.gated, undefined, 'client fixture must not be staged');
  assert.equal(body?.queued, undefined, 'client fixture must not be staged');
  assert.equal(body?.space, SPACE, 'client fixture landed in wrong space');
  assert.equal(body?.write_outcome, 'created', 'client fixture outcome was not created');
  assert.match(body?.source_id ?? '', ID_RE, 'malformed generated ID for client fixture');
  return { sourceId: body.source_id, label, answer };
}
