// SPDX-License-Identifier: Apache-2.0
// Tests for relay/reviewer/profile-state.mjs. Synthetic temp dirs only.

import { describe, it, beforeEach } from 'node:test';
import assert from 'node:assert/strict';
import {
  mkdtempSync, mkdirSync, writeFileSync, readFileSync, statSync, lstatSync,
  chmodSync, symlinkSync, unlinkSync, readdirSync, realpathSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  acquireProfileLock, saveProfileState, readProfileState, validateProfileState,
  LOCK_NAME, STATE_NAME,
} from '../reviewer/profile-state.mjs';

let dir;
beforeEach(() => {
  dir = realpathSync(mkdtempSync(join(realpathSync(tmpdir()), 'prof-state-')));
  chmodSync(dir, 0o700);
});

function profile(name = 'p') {
  const p = join(dir, name);
  mkdirSync(p, { recursive: true });
  chmodSync(p, 0o700);
  return p;
}

function validState(p) {
  return {
    schema: 1,
    kind: 'wenlan-isolated-review',
    profile: p,
    app: join(dir, 'a.app', 'Contents', 'MacOS', 'wenlan-app'),
    cache: join(dir, 'cache'),
    appSha256: 'a'.repeat(64),
    preparationSha256: 'b'.repeat(64),
    ports: { daemonPort: 19191, uiPort: 19192 },
  };
}

describe('lock', () => {
  it('exclusive collision refuses second holder', () => {
    const p = profile();
    const h = acquireProfileLock(p);
    assert.throws(() => acquireProfileLock(p), /collision/);
    h.release();
  });
  it('stale PID content still refuses (no recovery)', () => {
    const p = profile();
    writeFileSync(join(p, LOCK_NAME), 'pid=1', { mode: 0o600 });
    assert.throws(() => acquireProfileLock(p), /collision/);
  });
  it('dangling symlink collision throws and never removes incumbent', () => {
    const p = profile();
    symlinkSync(join(dir, 'nope'), join(p, LOCK_NAME));
    assert.throws(() => acquireProfileLock(p), /collision/);
    assert.ok(lstatSync(join(p, LOCK_NAME)).isSymbolicLink());
  });
  it('same-inode release removes own lock; substituted lock refused', () => {
    const p = profile();
    const h = acquireProfileLock(p);
    unlinkSync(join(p, LOCK_NAME));
    writeFileSync(join(p, LOCK_NAME), 'impostor', { mode: 0o600 });
    assert.throws(() => h.release(), /substituted/);
    assert.ok(statSync(join(p, LOCK_NAME)).isFile());
    h.retain(); // cleanup fd
  });
  it('retain preserves lock; release idempotent', () => {
    const p = profile();
    const h = acquireProfileLock(p);
    h.retain();
    assert.ok(statSync(join(p, LOCK_NAME)).isFile());
    h.retain();
    h.release(); // no-op after retain: lock preserved
    assert.ok(statSync(join(p, LOCK_NAME)).isFile());
    assert.throws(() => acquireProfileLock(p), /collision/);
    unlinkSync(join(p, LOCK_NAME));
    const h2 = acquireProfileLock(p);
    h2.release();
    h2.release();
    assert.throws(() => statSync(join(p, LOCK_NAME)), { code: 'ENOENT' });
  });
  it('rejects symlink/permissive/foreign profile', () => {
    const p = profile();
    const link = join(dir, 'plink');
    symlinkSync(p, link);
    assert.throws(() => acquireProfileLock(link), /absolute|canonical|real/);
    chmodSync(p, 0o755);
    assert.throws(() => acquireProfileLock(p), /permissions/);
    chmodSync(p, 0o700);
    const realGetuid = process.getuid;
    process.getuid = () => realGetuid() + 100000;
    try {
      assert.throws(() => acquireProfileLock(p), /foreign/);
    } finally {
      process.getuid = realGetuid;
    }
    const f = join(dir, 'fileprof');
    writeFileSync(f, 'x');
    assert.throws(() => acquireProfileLock(f), /directory/);
  });
});

describe('state', () => {
  it('bounds writes before replacing valid state and roundtrips UTF-8 paths', () => {
    const p = profile();
    const s = { ...validState(p), cache: join(dir, '\u6587\u703e') };
    saveProfileState(p, s);
    assert.deepEqual(readProfileState(p), s);
    assert.throws(() => saveProfileState(p, { ...s, extra: 'x'.repeat(65536) }), /oversize/);
    assert.deepEqual(readProfileState(p), s);
    assert.throws(() => saveProfileState(p, { ...s, profile: dir }), /profile mismatch/);
  });
  it('valid atomic read/write; no unrelated deletion', () => {
    const p = profile();
    const keep = join(p, 'keep.txt');
    writeFileSync(keep, 'keep');
    saveProfileState(p, validState(p));
    assert.equal(statSync(join(p, STATE_NAME)).mode & 0o777, 0o600);
    assert.deepEqual(readProfileState(p), validState(p));
    assert.equal(readFileSync(keep, 'utf8'), 'keep');
    saveProfileState(p, { ...validState(p), ports: { daemonPort: 19193, uiPort: 19194 } });
    assert.equal(readProfileState(p).ports.daemonPort, 19193);
    assert.equal(readFileSync(keep, 'utf8'), 'keep');
  });
  it('rejects invalid hashes/ports/schema and secrets', () => {
    const p = profile();
    const s = validState(p);
    assert.throws(() => validateProfileState({ ...s, appSha256: 'ZZZ' }), /hex64/);
    assert.throws(() => validateProfileState({ ...s, preparationSha256: 'ABC' }), /hex64/);
    assert.throws(() => validateProfileState({ ...s, ports: { daemonPort: 1, uiPort: 1 } }), /distinct/);
    for (const bad of [7878, 1420, 18080, 18083]) {
      assert.throws(() => validateProfileState({ ...s, ports: { daemonPort: bad, uiPort: 19192 } }), /reserved/);
    }
    assert.throws(() => validateProfileState({ ...s, ports: { daemonPort: 0, uiPort: 1 } }), /range/);
    assert.throws(() => validateProfileState({ ...s, ports: { daemonPort: 70000, uiPort: 1 } }), /range/);
    assert.throws(() => validateProfileState({ ...s, schema: 2 }), /schema/);
    assert.throws(() => validateProfileState({ ...s, kind: 'x' }), /kind/);
    assert.throws(() => validateProfileState({ ...s, profile: 'relative' }), /absolute/);
    assert.throws(() => validateProfileState({ ...s, extra: 1, OPENAI_API_KEY: 'x' }), /secret/);
    assert.doesNotThrow(() => validateProfileState({ ...s, extraField: 'ok' }));
    assert.throws(() => saveProfileState(p, { ...s, schema: 2 }), /schema/);
    assert.throws(() => statSync(join(p, STATE_NAME)), { code: 'ENOENT' });
  });
  it('oversize and malformed state rejected', () => {
    const p = profile();
    writeFileSync(join(p, STATE_NAME), 'not json', { mode: 0o600 });
    assert.throws(() => readProfileState(p), /malformed/);
    writeFileSync(join(p, STATE_NAME), Buffer.alloc(64 * 1024 + 1, 'x'), { mode: 0o600 });
    assert.throws(() => readProfileState(p), /oversize/);
  });
  it('symlink/permissive/foreign state rejected; temp cleaned, original preserved', () => {
    const p = profile();
    const s = validState(p);
    saveProfileState(p, s);
    const before = readFileSync(join(p, STATE_NAME), 'utf8');
    const realGetuid = process.getuid;
    process.getuid = () => realGetuid() + 100000;
    try {
      assert.throws(() => readProfileState(p), /foreign/);
      assert.throws(() => saveProfileState(p, s), /foreign/);
    } finally {
      process.getuid = realGetuid;
    }
    assert.equal(readFileSync(join(p, STATE_NAME), 'utf8'), before);
    chmodSync(join(p, STATE_NAME), 0o644);
    assert.throws(() => readProfileState(p), /permissions/);
    chmodSync(join(p, STATE_NAME), 0o600);
    unlinkSync(join(p, STATE_NAME));
    symlinkSync(join(dir, 'elsewhere'), join(p, STATE_NAME));
    assert.throws(() => readProfileState(p), /symlink/);
    assert.throws(() => saveProfileState(p, s), /symlink/);
    unlinkSync(join(p, STATE_NAME));
    writeFileSync(join(p, STATE_NAME), before, { mode: 0o600 });
    assert.throws(() => saveProfileState(p, { ...s, schema: 2 }), /schema/);
    assert.equal(readFileSync(join(p, STATE_NAME), 'utf8'), before);
    assert.ok(!readdirSync(p).some((n) => n.endsWith('.tmp')));
  });
});
