// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { execFile } from 'node:child_process';
import { chmod, mkdir, mkdtemp, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { promisify } from 'node:util';
import { readPrivateJson } from '../scripts/private-json.mjs';

const posixOnly = { skip: typeof process.getuid === 'function' ? false : 'POSIX-only private input checks' };
const windowsOnly = { skip: typeof process.getuid === 'function' ? 'POSIX UID support is available' : false };
const exec = promisify(execFile);

async function withTempdir(prefix, callback) {
  const root = await mkdtemp(join(tmpdir(), prefix));
  try {
    await callback(root);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

test('reads valid JSON from an owned private file', posixOnly, async () => {
  await withTempdir('wenlan-private-json-valid-', async root => {
    const file = join(root, 'private.json');
    const value = { id: 'enrollment', nested: { enabled: true } };
    await writeFile(file, JSON.stringify(value), { mode: 0o600 });
    assert.deepEqual(await readPrivateJson(file), value);
  });
});

test('reads valid JSON whose encoded size is exactly 4096 bytes', posixOnly, async () => {
  await withTempdir('wenlan-private-json-boundary-', async root => {
    const file = join(root, 'private.json');
    const prefix = '{"payload":"';
    const suffix = '"}';
    const text = `${prefix}${'x'.repeat(4096 - Buffer.byteLength(prefix) - Buffer.byteLength(suffix))}${suffix}`;
    assert.equal(Buffer.byteLength(text), 4096);
    await writeFile(file, text, { mode: 0o600 });
    const value = await readPrivateJson(file);
    assert.equal(value.payload.length, 4082);
  });
});

test('rejects a relative path', posixOnly, async () => {
  await withTempdir('wenlan-private-json-relative-', async root => {
    const file = join(root, 'private.json');
    await writeFile(file, '{}', { mode: 0o600 });
    await assert.rejects(readPrivateJson(basename(file)), /absolute path required/);
  });
});

test('rejects a symlink even when its target is private', posixOnly, async () => {
  await withTempdir('wenlan-private-json-symlink-', async root => {
    const target = join(root, 'private.json');
    const link = join(root, 'link.json');
    await writeFile(target, '{}', { mode: 0o600 });
    await symlink(target, link);
    await assert.rejects(readPrivateJson(link));
  });
});

test('rejects a directory', posixOnly, async () => {
  await withTempdir('wenlan-private-json-directory-', async root => {
    const directory = join(root, 'private.json');
    await mkdir(directory, { mode: 0o700 });
    await assert.rejects(readPrivateJson(directory), /private input required/);
  });
});

test('rejects a file with exposed permissions', posixOnly, async () => {
  await withTempdir('wenlan-private-json-permissions-', async root => {
    const file = join(root, 'private.json');
    await writeFile(file, '{}', { mode: 0o600 });
    await chmod(file, 0o640);
    await assert.rejects(readPrivateJson(file), /private input required/);
  });
});

test('rejects input larger than 4096 bytes', posixOnly, async () => {
  await withTempdir('wenlan-private-json-oversize-', async root => {
    const file = join(root, 'private.json');
    await writeFile(file, 'x'.repeat(4097), { mode: 0o600 });
    await assert.rejects(readPrivateJson(file), /input size/);
  });
});

test('rejects malformed UTF-8 before JSON parsing', posixOnly, async () => {
  await withTempdir('wenlan-private-json-utf8-', async root => {
    const file = join(root, 'private.json');
    await writeFile(file, Buffer.concat([Buffer.from('{"payload":"'), Buffer.from([0xff]), Buffer.from('"}')]), { mode: 0o600 });
    await assert.rejects(readPrivateJson(file), TypeError);
  });
});

test('rejects malformed JSON', posixOnly, async () => {
  await withTempdir('wenlan-private-json-malformed-', async root => {
    const file = join(root, 'private.json');
    await writeFile(file, '{"secret":', { mode: 0o600 });
    await assert.rejects(readPrivateJson(file), SyntaxError);
  });
});

test('rejects a FIFO without blocking when mkfifo is available', posixOnly, async t => {
  await withTempdir('wenlan-private-json-fifo-', async root => {
    const fifo = join(root, 'private.json');
    try {
      await exec('mkfifo', [fifo]);
    } catch (error) {
      if (error.code === 'ENOENT') {
        t.skip('mkfifo unavailable');
        return;
      }
      throw error;
    }
    await assert.rejects(readPrivateJson(fifo), /private input required/);
  });
});

test('fails closed when POSIX ownership checks are unavailable', windowsOnly, async () => {
  await assert.rejects(readPrivateJson('C:\\private.json'), /POSIX/);
});
