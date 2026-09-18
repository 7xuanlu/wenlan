import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const entrypoint = fileURLToPath(new URL('./entrypoint.sh', import.meta.url));

for (const [name, token] of [
  ['missing token', undefined],
  ['short token', 'not-a-real-token'],
  ['invalid token characters', 'synthetic-token-with-a-forbidden-character!'],
]) {
  test(`refuses ${name} before starting the runtime`, () => {
    const env = { PATH: '/usr/bin:/bin' };
    if (token !== undefined) env.REVIEWER_BEARER_TOKEN = token;
    const result = spawnSync('/bin/bash', [entrypoint], {
      env, encoding: 'utf8', timeout: 3000,
    });
    assert.equal(result.error, undefined);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /REVIEWER_BEARER_TOKEN (is empty|malformed)/);
    assert.equal(result.stdout, '');
    if (token) assert.ok(!result.stderr.includes(token));
  });
}
