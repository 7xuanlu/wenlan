// SPDX-License-Identifier: Apache-2.0
// Opt-in full App lifecycle and real local source readback, not OAuth/UI proof.
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { launch, portClosed } from '../reviewer/launch-candidate.mjs';
import { readProfileState } from '../reviewer/profile-state.mjs';

test('full candidate preserves real source-linked fixture across profile restart', { timeout: 180000 }, async (t) => {
  assert.equal(process.env.WENLAN_TEST_NATIVE_APP, '1', 'explicit isolated native App test approval required');
  assert.equal(process.platform, 'darwin');
  const root = mkdtempSync('/private/tmp/wenlan-resume-');
  t.diagnostic(`Retained native resume evidence: ${root}`);
  const args = { app: process.env.WENLAN_APP_TEST_BIN, sha256: process.env.WENLAN_APP_TEST_SHA256,
    cache: process.env.WENLAN_TEST_FASTEMBED_CACHE, profile: join(root, 'profile'),
    durationSeconds: 1, confirmSyntheticLibrary: true };
  const observations = [];
  async function observe(line) {
    assert.ok(line.startsWith('READY '));
    const ready = JSON.parse(line.slice(6));
    const prepared = JSON.parse(readFileSync(ready.preparationReceipt, 'utf8'));
    assert.equal(prepared.status, 'prepared');
    assert.equal(prepared.test_cases.length, 5);
    assert.equal(prepared.negative_test_cases.length, 3);
    const ids = prepared.fixture.mapping;
    async function sources(page) {
      const response = await fetch(`${ready.daemonUrl}/api/pages/${encodeURIComponent(ids[page])}/sources`, {
        headers: { 'X-Wenlan-Space': 'atlas-review' }, redirect: 'error', signal: AbortSignal.timeout(5000),
      });
      assert.equal(response.status, 200);
      return response.json();
    }
    const auth = await sources('page_atlas-auth');
    assert.equal(auth.length, 1);
    assert.equal(auth[0].memory.source_id, ids['mem_atlas-auth']);
    const unavailable = await sources('page_atlas-unavailable');
    assert.equal(unavailable.length, 1);
    assert.equal(unavailable[0].memory, null);
    observations.push({ resumed: ready.resumed, daemonUrl: ready.daemonUrl,
      ids, authAvailable: auth.length, unavailableAvailable: 0 });
  }
  const first = await launch(args, { emit: observe });
  assert.equal(first.status, 'completed');
  assert.ok(await portClosed(first.daemonPort));
  const fixtureBytes = readFileSync(first.preparationReceipt);
  const state = readProfileState(first.profile);
  const second = await launch({ ...args, resume: true }, { emit: observe,
    prepareFn: async () => { throw new Error('resume must never seed again'); } });
  assert.equal(second.status, 'completed');
  assert.ok(await portClosed(second.daemonPort));
  assert.deepEqual(readFileSync(second.preparationReceipt), fixtureBytes);
  assert.deepEqual(readProfileState(second.profile), state);
  assert.equal(observations.length, 2);
  assert.equal(observations[0].resumed, false);
  assert.equal(observations[1].resumed, true);
  assert.deepEqual(observations[0].ids, observations[1].ids);
  assert.equal(observations[0].daemonUrl, observations[1].daemonUrl);
  writeFileSync(join(root, 'native-resume-receipt.json'), JSON.stringify({ first, second, observations,
    verification: { localSourcePersistence: 'passed', relay: 'unchecked', oauth: 'unchecked',
      model: 'unchecked', ui: 'unchecked', signedInstallation: 'unchecked' } }, null, 2), { mode: 0o600, flag: 'wx' });
});
