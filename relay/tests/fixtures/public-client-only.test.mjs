// SPDX-License-Identifier: Apache-2.0
// Focused unit tests for the Chat-only env-mode gate. No daemon, no network.
import assert from 'node:assert/strict';
import test from 'node:test';
import { createClientFixture, validatePublicClientOnlyEnv } from './public-client-only.mjs';

const PAIR = 'a'.repeat(64);
const base = () => ({
  WENLAN_PUBLIC_CLIENT_ONLY: '1',
  WENLAN_PORTAL_PAIRING_ID: PAIR,
  WENLAN_PORTAL_CLIENT_ID: 'chat-client',
});

test('absent mode returns false without further checks', () => {
  assert.equal(validatePublicClientOnlyEnv({}, { transport: 'tunnel' }), false);
});

test('valid attended reverse portal pair is accepted', () => {
  assert.equal(validatePublicClientOnlyEnv(base(), { transport: 'reverse', attendedRecovery: false }), true);
});

test('non-1 opt-in value is rejected', () => {
  assert.throws(() => validatePublicClientOnlyEnv({ ...base(), WENLAN_PUBLIC_CLIENT_ONLY: 'yes' },
    { transport: 'reverse' }));
});

test('tunnel transport is rejected', () => {
  assert.throws(() => validatePublicClientOnlyEnv(base(), { transport: 'tunnel' }));
});

test('missing portal pairing is rejected', () => {
  const env = base();
  delete env.WENLAN_PORTAL_PAIRING_ID;
  assert.throws(() => validatePublicClientOnlyEnv(env, { transport: 'reverse' }));
});

test('Codex client lane is rejected', () => {
  assert.throws(() => validatePublicClientOnlyEnv({ ...base(), WENLAN_CODEX_BIN: '/bin/codex' },
    { transport: 'reverse' }));
});

test('Codex model lane is rejected', () => {
  assert.throws(() => validatePublicClientOnlyEnv({ ...base(), WENLAN_CODEX_MODEL_HOME: '/tmp/m' },
    { transport: 'reverse' }));
});

test('attended recovery lane is rejected', () => {
  assert.throws(() => validatePublicClientOnlyEnv(base(), { transport: 'reverse', attendedRecovery: true }));
});

test('client fixture stores via existing daemon API and returns fresh IDs', async () => {
  const seen = [];
  const fetchImpl = async (url, init) => {
    seen.push({ url, init });
    assert.match(url, /^http:\/\/127\.0\.0\.1:\d+\/api\/memory\/store$/);
    const body = JSON.parse(init.body);
    assert.equal(body.space, 'atlas-review');
    assert.equal(body.memory_type, 'decision');
    assert.match(body.title, /^chat-only probe [a-f0-9]+$/);
    assert.match(body.content, /synthetic answer [a-f0-9]+/);
    return { ok: true, status: 200, json: async () => ({ source_id: 'src_abc123', space: 'atlas-review', write_outcome: 'created' }) };
  };
  const first = await createClientFixture({ daemonUrl: 'http://127.0.0.1:8123', fetchImpl });
  const second = await createClientFixture({ daemonUrl: 'http://127.0.0.1:8123', fetchImpl });
  assert.equal(first.sourceId, 'src_abc123');
  assert.notEqual(first.label, second.label, 'query labels must be unpredictable per run');
  assert.notEqual(first.answer, second.answer, 'answer nonces must be unpredictable per run');
  assert.equal(seen.length, 2);
});

test('client fixture rejects default daemon port', async () => {
  await assert.rejects(() => createClientFixture({
    daemonUrl: 'http://127.0.0.1:7878',
    fetchImpl: async () => { throw new Error('must not call'); },
  }));
});
