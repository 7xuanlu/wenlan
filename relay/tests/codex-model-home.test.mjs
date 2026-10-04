// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { chmod, mkdir, mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { prepareModelHome, validateModelHome } from './fixtures/codex-model-home.mjs';

async function fixture(t) {
  const root = await mkdtemp('/tmp/wenlan-codex-model-');
  const canonical = await import('node:fs/promises').then(fs => fs.realpath(root));
  t.after(() => rm(canonical, { recursive: true, force: true }));
  const home = join(canonical, 'codex');
  await mkdir(home, { mode: 0o700 });
  await writeFile(join(home, 'auth.json'), 'synthetic credential fixture', { mode: 0o600 });
  return home;
}

test('isolated model config cleanup preserves the preexisting login', async t => {
  const home = await fixture(t);
  const cleanup = await prepareModelHome(home, 'model = "gpt-5.6-luna"\n');
  assert.match(await readFile(join(home, 'config.toml'), 'utf8'), /luna/);
  await cleanup();
  assert.equal(await readFile(join(home, 'auth.json'), 'utf8'), 'synthetic credential fixture');
  await assert.rejects(readFile(join(home, 'config.toml')), { code: 'ENOENT' });
});

test('existing configuration is never overwritten', async t => {
  const home = await fixture(t);
  await writeFile(join(home, 'config.toml'), 'existing');
  await assert.rejects(prepareModelHome(home, 'replacement'), { code: 'EEXIST' });
  assert.equal(await readFile(join(home, 'config.toml'), 'utf8'), 'existing');
});

test('nonprivate credential fails before configuration writes', async t => {
  const home = await fixture(t);
  await chmod(join(home, 'auth.json'), 0o644);
  await assert.rejects(prepareModelHome(home, 'test'), /credential file must be private/);
  await assert.rejects(readFile(join(home, 'config.toml')), { code: 'ENOENT' });
});

test('credential symlinks fail closed', async t => {
  const home = await fixture(t);
  await rm(join(home, 'auth.json'));
  await symlink('missing', join(home, 'auth.json'));
  await assert.rejects(prepareModelHome(home, 'test'), /official file login required/);
});

test('non-test scope is rejected without reading credentials', async () => {
  await assert.rejects(prepareModelHome('/private/tmp', 'test'));
});

test('model home validation does not create or rewrite configuration', async t => {
  const home = await fixture(t);
  assert.equal(await validateModelHome(home), home);
  await assert.rejects(readFile(join(home, 'config.toml')), { code: 'ENOENT' });
  await writeFile(join(home, 'config.toml'), 'preserve');
  await validateModelHome(home);
  assert.equal(await readFile(join(home, 'config.toml'), 'utf8'), 'preserve');
});
