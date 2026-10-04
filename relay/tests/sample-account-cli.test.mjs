// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtemp, mkdir, writeFile, readFile, stat, symlink, chmod, rm, access } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { parseSampleAccount } from '../src/sample-account.ts';
import { hashSecret } from '../src/secrets.ts';

const exec = promisify(execFile);
const script = fileURLToPath(new URL('../scripts/prepare-sample-account.mjs', import.meta.url));
const credential = { id: 'a'.repeat(64), managementToken: 'b'.repeat(64), expiresAt: Date.now() + 86_400_000 };
const resource = 'https://wenlan-relay.example/mcp';
const posix = { skip: typeof process.getuid !== 'function' ? 'POSIX operator utility; Windows ACL support is unimplemented' : false };

test('offline CLI creates private separate files without disclosing credentials or overwriting', posix, async () => {
  const root = await mkdtemp(join(tmpdir(), 'wenlan-sample-cli-'));
  try {
    const enrollment = join(root, 'enrollment.json');
    await writeFile(enrollment, JSON.stringify(credential), { mode: 0o600 });
    const output = join(root, 'prepared');
    const args = [script, '--enrollment', enrollment, '--space', 'atlas-review', '--resource', resource,
      '--output', output, '--synthetic-library'];
    const result = await exec(process.execPath, args, { timeout: 5000 });
    assert.equal(result.stderr, '');
    const accountText = await readFile(join(output, 'sample-account.json'), 'utf8');
    const password = (await readFile(join(output, 'reviewer-password.txt'), 'utf8')).trim();
    const account = parseSampleAccount(JSON.parse(accountText));
    assert(account);
    assert.match(password, /^[a-f0-9]{64}$/);
    assert.equal(account.passwordHash, await hashSecret(`wenlan-sample-account-v1\0reviewer\0${password}`));
    assert.equal(account.managementHash, await hashSecret(credential.managementToken));
    assert.equal(account.expiresAt, credential.expiresAt);
    assert.equal(account.generation, 0);
    assert.equal(account.space, 'atlas-review');
    assert.equal(account.resource, resource);
    assert(!accountText.includes(password));
    for (const value of [password, credential.managementToken, account.passwordHash, account.managementHash]) {
      assert(!result.stdout.includes(value));
    }
    assert.equal((await stat(output)).mode & 0o777, 0o700);
    for (const name of ['sample-account.json', 'reviewer-password.txt']) assert.equal((await stat(join(output, name))).mode & 0o777, 0o600);
    await assert.rejects(exec(process.execPath, args, { timeout: 5000 }), error => error.code === 1);
    assert.equal(await readFile(join(output, 'sample-account.json'), 'utf8'), accountText);
    assert.equal((await readFile(join(output, 'reviewer-password.txt'), 'utf8')).trim(), password);
  } finally { await rm(root, { recursive: true, force: true }); }
});

test('offline CLI rejects exposed, symlinked, malformed input and missing explicit intent', posix, async () => {
  const root = await mkdtemp(join(tmpdir(), 'wenlan-sample-cli-denial-'));
  try {
    const privateFile = join(root, 'private.json');
    const exposed = join(root, 'exposed.json');
    const link = join(root, 'link.json');
    const malformed = join(root, 'malformed.json');
    const oversized = join(root, 'oversized.json');
    await writeFile(privateFile, JSON.stringify(credential), { mode: 0o600 });
    await writeFile(exposed, JSON.stringify(credential), { mode: 0o600 });
    await chmod(exposed, 0o644);
    await symlink(privateFile, link);
    await writeFile(malformed, 'SECRET_INVALID_JSON', { mode: 0o600 });
    await writeFile(oversized, 'x'.repeat(4097), { mode: 0o600 });
    for (const [index, input] of [exposed, link, malformed, oversized, privateFile].entries()) {
      const output = join(root, `denied-${index}`);
      const args = [script, '--enrollment', input, '--space', 'atlas-review', '--resource', resource, '--output', output];
      if (input !== privateFile) args.push('--synthetic-library');
      await assert.rejects(exec(process.execPath, args, { timeout: 5000 }), error => {
        assert.equal(error.code, 1);
        assert.equal(error.stdout, '');
        assert(!error.stderr.includes('SECRET_INVALID_JSON'));
        assert(!error.stderr.includes(credential.managementToken));
        return true;
      });
      await assert.rejects(access(output));
    }
    const shared = join(root, 'shared-parent');
    await mkdir(shared);
    await chmod(shared, 0o777);
    const output = join(shared, 'denied');
    await assert.rejects(exec(process.execPath, [script, '--enrollment', privateFile, '--space', 'atlas-review',
      '--resource', resource, '--output', output, '--synthetic-library'], { timeout: 5000 }), error => error.code === 1);
    await assert.rejects(access(output));
  } finally { await rm(root, { recursive: true, force: true }); }
});
