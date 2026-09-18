// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import { lstat, realpath, unlink, writeFile } from 'node:fs/promises';
import { basename, dirname, isAbsolute, join } from 'node:path';
import { tmpdir } from 'node:os';

// Reuse only an explicitly prepared temporary login, never the user's Codex home.
export async function prepareModelHome(path, config) {
  assert(path && isAbsolute(path), 'absolute synthetic model home required');
  const canonical = await realpath(path);
  assert.equal(canonical, path, 'model home must use its canonical path');
  const parent = dirname(path);
  assert.equal(basename(path), 'codex');
  assert.match(basename(parent), /^wenlan-codex-model-[A-Za-z0-9]+$/);
  const allowedRoots = new Set([await realpath(tmpdir()), await realpath('/tmp')]);
  assert(allowedRoots.has(dirname(parent)), 'model home must be in owned temporary test scope');
  for (const dir of [parent, path]) {
    const info = await lstat(dir);
    assert(info.isDirectory() && !info.isSymbolicLink());
    assert.equal(info.uid, process.getuid());
    assert.equal(info.mode & 0o077, 0, 'model home must be private');
  }
  const credential = await lstat(join(path, 'auth.json'));
  assert(credential.isFile() && !credential.isSymbolicLink(), 'official file login required');
  assert.equal(credential.uid, process.getuid());
  assert.equal(credential.mode & 0o077, 0, 'credential file must be private');
  const configPath = join(path, 'config.toml');
  await writeFile(configPath, config, { flag: 'wx', mode: 0o600 });
  const owned = await lstat(configPath);
  return async () => {
    const current = await lstat(configPath);
    assert(current.isFile() && current.dev === owned.dev && current.ino === owned.ino,
      'refusing to remove a replaced configuration');
    await unlink(configPath);
  };
}
