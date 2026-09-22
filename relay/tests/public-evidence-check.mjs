// SPDX-License-Identifier: Apache-2.0
// Pure unit check: public lane uses API fixture + synthetic-only receipts.
import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(join(here, 'public-native-check.mjs'), 'utf8');

test('public lane seeds via normal API fixture, not the seed example', () => {
  assert.match(source, /seedReviewerViaApi/);
  assert.doesNotMatch(source, /seed_reviewer_library/);
  assert.match(source, /expectedKnowledgePath/);
  assert.match(source, /knowledge_path/);
});

test('public lane persists synthetic-only evidence and preserves scratch', () => {
  for (const name of ['fixture-mapping.json', 'binary-hashes.json', 'cleanup-status.json',
    'model-positive-proof.json', 'model-negative-proof.json']) {
    assert(source.includes(name), `evidence receipt ${name} required`);
  }
  assert.match(source, /model-positive-failure\.json/);
  assert.match(source, /model-negative-failure\.json/);
  assert.match(source, /reviewerInstallation.*unchecked/);
  assert.match(source, /gui.*unchecked/i);
  assert.doesNotMatch(source, /await rm\(scratch, \{ recursive: true/);
});
