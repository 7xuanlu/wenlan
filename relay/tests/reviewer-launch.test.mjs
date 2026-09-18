// SPDX-License-Identifier: Apache-2.0
// Unit tests for relay/reviewer/launch-candidate.mjs. Fake child/runtime only.

import { describe, it, beforeEach } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, mkdirSync, readFileSync, statSync, chmodSync, symlinkSync, realpathSync, renameSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname, basename } from 'node:path';
import { EventEmitter } from 'node:events';
import { createHash } from 'node:crypto';
import {
  parseArgs, renderHelp, preflight, createProfile, buildEnv, pollReadiness,
  launch, DEFAULT_DURATION_SECONDS, profileLayout,
} from '../reviewer/launch-candidate.mjs';
import { acquireProfileLock, readProfileState, LOCK_NAME } from '../reviewer/profile-state.mjs';

let dir;
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), 'launch-cand-'));
});

function makeApp(executable = true) {
  const exe = join(dir, `W${Math.random().toString(36).slice(2)}.app`, 'Contents', 'MacOS', 'wenlan-app');
  mkdirSync(join(exe, '..'), { recursive: true });
  writeFileSync(exe, 'fake-app-bytes');
  chmodSync(exe, executable ? 0o755 : 0o644);
  return exe;
}
function makeCache() {
  const c = join(dir, 'cache');
  mkdirSync(c);
  return c;
}
function parsedFor(app, cache, profile, durationSeconds = 1) {
  return { app, sha256: createHash('sha256').update(readFileSync(app)).digest('hex'), profile, cache, durationSeconds, confirmSyntheticLibrary: true };
}
// Preflight canonicalizes the profile parent via realpath (/var -> /private/var);
// the daemon readback uses the canonical path, so tests must expect that.
function canonPages(profile) {
  return join(realpathSync(dirname(profile)), basename(profile), 'library', 'pages');
}
function canonProfile(profile) {
  return join(realpathSync(dirname(profile)), basename(profile));
}

class FakeChild extends EventEmitter {
  constructor() {
    super();
    this.pid = 4242;
    this.exitCode = null;
    this.signalCode = null;
    this.stdout = new EventEmitter();
    this.stderr = new EventEmitter();
    this.kills = [];
  }
  kill(sig) {
    this.kills.push(sig);
    return this.onKill ? this.onKill(sig) : true;
  }
}

function baseDeps(child, over = {}) {
  return {
    platform: 'darwin',
    allocatePorts: async () => ({ daemonPort: 19191, uiPort: 19192 }),
    spawnFn: () => child,
    fetchImpl: async () => ({ ok: true, json: async () => ({ path: 'unset' }) }),
    prepareFn: async () => ({ status: 'prepared' }),
    sleep: async () => {},
    tickMs: 1,
    pollMs: 60,
    pollIntervalMs: 1,
    graceMs: 5,
    killMs: 5,
    noSignalTrap: true,
    portClosedFn: async () => true,
    ...over,
  };
}
function graceful(child) {
  child.onKill = (sig) => {
    if (sig === 'SIGTERM') {
      child.exitCode = 0;
      child.emit('exit', 0, null);
    }
    return true;
  };
}

async function fakePrepare(args) {
  writeFileSync(args.output, JSON.stringify({ status: 'prepared' }), { mode: 0o600 });
  return { status: 'prepared' };
}

function preparedLaunchDeps(p, extra = {}) {
  const child = new FakeChild();
  graceful(child);
  return baseDeps(child, { fetchImpl: async () => ({ ok: true,
    json: async () => ({ path: canonPages(p.profile) }) }), prepareFn: fakePrepare,
    emit: () => {}, ...extra });
}

describe('resume', { timeout: 20000 }, () => {
  it('reopens the same profile without seeding, allocating new ports or changing fixture IDs', async () => {
    const p = parsedFor(makeApp(), makeCache(), join(dir, 'resume'));
    const first = await launch(p, preparedLaunchDeps(p));
    const bytes = readFileSync(first.preparationReceipt);
    const state = readProfileState(first.profile);
    let called = false;
    const second = await launch({ ...p, resume: true }, preparedLaunchDeps(p, {
      prepareFn: async () => { called = true; throw new Error('must not seed'); },
      allocatePorts: async () => { throw new Error('must reuse verified ports'); },
    }));
    assert.equal(second.status, 'completed');
    assert.equal(second.profile, first.profile);
    assert.equal(second.daemonPort, first.daemonPort);
    assert.deepEqual(readFileSync(second.preparationReceipt), bytes);
    assert.deepEqual(readProfileState(first.profile), state);
    assert.equal(called, false);
    assert.throws(() => statSync(join(first.profile, LOCK_NAME)), { code: 'ENOENT' });
  });
  it('an incumbent lock rejects resume before spawning and remains untouched', async () => {
    const p = parsedFor(makeApp(), makeCache(), join(dir, 'locked'));
    const first = await launch(p, preparedLaunchDeps(p));
    const lock = acquireProfileLock(first.profile);
    let spawned = false;
    try {
      await assert.rejects(launch({ ...p, resume: true }, preparedLaunchDeps(p, {
        spawnFn: () => { spawned = true; throw new Error('unexpected spawn'); },
      })), /lock collision/);
      assert.equal(spawned, false);
      assert.ok(statSync(join(first.profile, LOCK_NAME)).isFile());
    } finally { lock.release(); }
  });
  it('changed config, fixture or symlinked profile directories fail before spawn', async () => {
    const app = makeApp();
    const cache = makeCache();
    for (const kind of ['config', 'fixture', 'symlink']) {
      const p = parsedFor(app, cache, join(dir, kind));
      const first = await launch(p, preparedLaunchDeps(p));
      const layout = profileLayout(first.profile);
      if (kind === 'config') writeFileSync(layout.config, JSON.stringify({ knowledge_path: '/different' }));
      if (kind === 'fixture') writeFileSync(layout.preparationReceipt, '{"status":"prepared","changed":true}');
      if (kind === 'symlink') {
        renameSync(layout.logs, `${layout.logs}.retained`);
        symlinkSync(`${layout.logs}.retained`, layout.logs);
      }
      let spawned = false;
      await assert.rejects(launch({ ...p, resume: true }, preparedLaunchDeps(p, {
        spawnFn: () => { spawned = true; throw new Error('unexpected spawn'); },
      })), /knowledge path changed|receipt changed|unsafe profile directory/);
      assert.equal(spawned, false);
      assert.ok(statSync(join(first.profile, LOCK_NAME)).isFile());
    }
  });
  it('clean cancellation permits resume, without treating it as a crashed profile', async () => {
    const p = parsedFor(makeApp(), makeCache(), join(dir, 'cancel-resume'));
    await assert.rejects(launch(p, preparedLaunchDeps(p, {
      noSignalTrap: false, emit: () => process.emit('SIGINT'),
    })), { code: 'cancelled' });
    const receipt = await launch({ ...p, resume: true }, preparedLaunchDeps(p, {
      prepareFn: async () => { throw new Error('must not seed'); },
    }));
    assert.equal(receipt.status, 'completed');
  });
});

describe('args', () => {
  it('--help alone; unknown/duplicate/missing rejected; default 900', () => {
    assert.deepEqual(parseArgs(['--help']), { help: true });
    assert.match(renderHelp(), /confirm-synthetic-library/);
    assert.throws(() => parseArgs(['--help', '--app', 'x']), /--help/);
    const app = makeApp();
    const cache = makeCache();
    const base = ['--app', app, '--sha256', 'a'.repeat(64), '--profile', join(dir, 'p'), '--cache', cache];
    assert.throws(() => parseArgs([...base, '--confirm-synthetic-library', '--bogus']), /unknown/);
    assert.throws(() => parseArgs([...base, '--confirm-synthetic-library', '--app', app]), /duplicate/);
    assert.throws(() => parseArgs(['--app']), /missing value/);
    assert.throws(() => parseArgs(base), /confirm-synthetic-library/);
    assert.equal(parseArgs([...base, '--confirm-synthetic-library']).durationSeconds, DEFAULT_DURATION_SECONDS);
    for (const bad of ['0', '3601', 'abc']) {
      assert.throws(() => parseArgs([...base, '--confirm-synthetic-library', '--duration-seconds', bad]), /duration/);
    }
  });
});

describe('preflight paths', () => {
  it('hash and consent rejection occur before creating a profile or spawning', async () => {
    const p = parsedFor(makeApp(), makeCache(), join(dir, 'rejected'));
    let spawned = false;
    for (const bad of [{ ...p, sha256: '0'.repeat(64) }, { ...p, confirmSyntheticLibrary: false }]) {
      await assert.rejects(launch(bad, { platform: 'darwin', spawnFn: () => { spawned = true; } }), /hash mismatch|consent/);
    }
    assert.equal(spawned, false);
    assert.throws(() => statSync(p.profile), { code: 'ENOENT' });
  });
  it('isolates paths and drops inherited credentials and personal runtime overrides', () => {
    const p = parsedFor(makeApp(), makeCache(), join(dir, 'isolated'));
    const canon = preflight(p, { platform: 'darwin' });
    const layout = createProfile(canon);
    const env = buildEnv(canon, layout, { daemonPort: 19191, uiPort: 19192 }, {
      PATH: '/usr/bin', LANG: 'en_US.UTF-8', TMPDIR: '/tmp', HOME: '/personal',
      WENLAN_DATA_DIR: '/personal/library', ORIGIN_DATA_DIR: '/old/library',
      OPENAI_API_KEY: 'test-secret', WENLAN_RELAY_TOKEN: 'test-token',
      WENLAN_TELEMETRY_DISABLED: '0',
    });
    assert.equal(env.PATH, '/usr/bin');
    assert.equal(env.HOME, layout.home);
    assert.equal(env.USERPROFILE, layout.home);
    assert.equal(env.WENLAN_DATA_DIR, layout.library);
    assert.equal(env.WENLAN_NO_AUTOSTART, '1');
    assert.equal(env.WENLAN_TELEMETRY_DISABLED, '1');
    assert.equal(env.WENLAN_SPACE, 'atlas-review');
    for (const name of ['OPENAI_API_KEY', 'WENLAN_RELAY_TOKEN', 'ORIGIN_DATA_DIR']) assert.equal(env[name], undefined);
    assert.equal(statSync(layout.profile).mode & 0o777, 0o700);
    assert.equal(statSync(layout.config).mode & 0o777, 0o600);
    assert.equal(JSON.parse(readFileSync(layout.config, 'utf8')).knowledge_path, layout.pages);
  });
  it('requires exact bundle suffix, absolute path, exec bits', () => {
    const cache = makeCache();
    const good = makeApp();
    const p = parsedFor(good, cache, join(dir, 'p'));
    assert.doesNotThrow(() => preflight(p, { platform: 'darwin' }));
    const wrong = join(dir, 'wenlan-app');
    writeFileSync(wrong, 'x');
    chmodSync(wrong, 0o755);
    assert.throws(() => preflight(parsedFor(wrong, cache, join(dir, 'p2')), { platform: 'darwin' }), /exact/);
    const noexec = makeApp(false);
    assert.throws(() => preflight(parsedFor(noexec, cache, join(dir, 'p3')), { platform: 'darwin' }), /executable/);
    assert.throws(() => preflight({ ...p, app: 'relative/path' }, { platform: 'darwin' }), /absolute/);
    assert.throws(() => preflight({ ...p, sha256: 'ZZZ' }, { platform: 'darwin' }), /sha256/);
    assert.throws(() => preflight(p, { platform: 'linux' }), /darwin/);
  });
  it('canonicalizes via realpath (/var -> /private/var)', () => {
    const app = makeApp();
    const cache = makeCache();
    const sha = createHash('sha256').update(readFileSync(app)).digest('hex');
    const p = { app: '/var/fake-app.app/Contents/MacOS/wenlan-app', sha256: sha, profile: join(dir, 'p'), cache, durationSeconds: 1, confirmSyntheticLibrary: true };
    const out = preflight(p, {
      platform: 'darwin',
      realpathSync: (x) => (x === '/var/fake-app.app/Contents/MacOS/wenlan-app' ? '/private/var/fake-app.app/Contents/MacOS/wenlan-app' : x),
      statSync: (x) => {
        if (x === '/private/var/fake-app.app/Contents/MacOS/wenlan-app') return { isFile: () => true, isDirectory: () => false, mode: 0o755 };
        return statSync(x);
      },
      accessSync: () => {},
      hashFile: () => sha,
    });
    assert.equal(out.app, '/private/var/fake-app.app/Contents/MacOS/wenlan-app');
    // Live TMPDIR canonicalizes the same way: real /var symlink resolves under /private/var.
    assert.equal(canonPages(join(dir, 'p')), profileLayout(canonProfile(join(dir, 'p'))).pages);
  });
  it('refuses existing profile, dangling symlink, missing parent', async () => {
    const app = makeApp();
    const cache = makeCache();
    const sha = createHash('sha256').update(readFileSync(app)).digest('hex');
    const mk = (profile) => ({ app, sha256: sha, profile, cache, durationSeconds: 1, confirmSyntheticLibrary: true });
    const existing = join(dir, 'exists');
    mkdirSync(existing);
    await assert.rejects(launch(mk(existing), baseDeps(new FakeChild())), /existing profile/);
    const target = join(dir, 't');
    writeFileSync(target, 'x');
    const link = join(dir, 'link');
    symlinkSync(target, link);
    await assert.rejects(launch(mk(link), baseDeps(new FakeChild())), /existing profile/);
    const dangling = join(dir, 'dangling');
    symlinkSync(join(dir, 'nope'), dangling);
    await assert.rejects(launch(mk(dangling), baseDeps(new FakeChild())), /existing profile/);
    await assert.rejects(launch(mk(join(dir, 'no-parent', 'p')), baseDeps(new FakeChild())), /parent must already exist/);
  });
  it('chmod/write failure surfaces, profile preserved', () => {
    const app = makeApp();
    const cache = makeCache();
    const canon = preflight(parsedFor(app, cache, join(dir, 'prof')), { platform: 'darwin' });
    assert.throws(() => createProfile(canon, { chmodSync: () => { throw new Error('chmod boom'); } }), /chmod boom/);
    assert.ok(statSync(canon.profile).isDirectory(), 'profile preserved on failure');
  });
  it('occupied daemon port refused before spawn', async () => {
    const app = makeApp();
    const cache = makeCache();
    const p = parsedFor(app, cache, join(dir, 'occ'));
    let spawned = 0;
    const child = new FakeChild();
    await assert.rejects(launch(p, baseDeps(child, {
      spawnFn: () => { spawned += 1; return child; },
      portClosedFn: async () => false,
    })), /occupied daemon port/);
    assert.equal(spawned, 0);
  });
});

describe('readiness', () => {
  it('malformed readiness rejected (non-JSON, wrong path, empty)', async () => {
    const url = 'http://127.0.0.1:19191';
    await pollReadiness({ daemonUrl: url, expectedPath: '/a', fetchImpl: async () => ({ ok: true, json: async () => ({ path: '/a' }) }), timeoutMs: 50, intervalMs: 1, sleepFn: async () => {} });
    await assert.rejects(pollReadiness({ daemonUrl: url, expectedPath: '/a', fetchImpl: async () => ({ ok: true, json: async () => ({ path: '/b' }) }), timeoutMs: 50, intervalMs: 1, sleepFn: async () => {} }), /mismatch/);
    await assert.rejects(pollReadiness({ daemonUrl: url, expectedPath: '/a', fetchImpl: async () => ({ ok: true, json: async () => { throw new Error('bad'); } }), timeoutMs: 50, intervalMs: 1, sleepFn: async () => {} }), /non-JSON/);
    await assert.rejects(pollReadiness({ daemonUrl: url, expectedPath: '/a', fetchImpl: async () => ({ ok: true, json: async () => ({}) }), timeoutMs: 50, intervalMs: 1, sleepFn: async () => {} }), /malformed path/);
  });
});

describe('lifecycle', { timeout: 15000 }, () => {
  it('an error from a running child is not mistaken for proof that it exited', async () => {
    const p = parsedFor(makeApp(), makeCache(), join(dir, 'running-error'));
    const child = new FakeChild();
    graceful(child);
    await assert.rejects(launch(p, baseDeps(child, {
      fetchImpl: async () => { child.emit('error', new Error('running child error')); throw new Error('unavailable'); },
    })), /running child error/);
    assert.deepEqual(child.kills, ['SIGTERM']);
    assert.equal(child.exitCode, 0);
  });
  it('receipt write failure is surfaced after owned child cleanup', async () => {
    const p = parsedFor(makeApp(), makeCache(), join(dir, 'receipt-failure'));
    const child = new FakeChild();
    graceful(child);
    const layout = profileLayout(canonProfile(p.profile));
    await assert.rejects(launch(p, baseDeps(child, {
      fetchImpl: async () => ({ ok: true, json: async () => ({ path: layout.pages }) }),
      prepareFn: async () => { throw new Error('seed failed'); },
      writeFileSync: (path, ...args) => {
        if (path === layout.launchReceipt) throw new Error('receipt disk error');
        return writeFileSync(path, ...args);
      },
    })), /seed failed.*receipt disk error/);
    assert.deepEqual(child.kills, ['SIGTERM']);
  });
  it('happy path: READY points at existing preparation receipt, status completed', async () => {
    const app = makeApp();
    const cache = makeCache();
    const profile = join(dir, 'happy');
    const p = parsedFor(app, cache, profile);
    const child = new FakeChild();
    graceful(child);
    let readyLine = '';
    const layout = profileLayout(canonProfile(profile));
    const receipt = await launch(p, baseDeps(child, {
      fetchImpl: async () => ({ ok: true, json: async () => ({ path: canonPages(profile) }) }),
      prepareFn: fakePrepare,
      emit: (l) => { readyLine = l; },
    }));
    assert.equal(receipt.status, 'completed');
    assert.deepEqual(child.kills, ['SIGTERM']);
    const body = JSON.parse(readyLine.replace(/^READY /, ''));
    assert.equal(body.preparationReceipt, layout.preparationReceipt);
    assert.ok(statSync(body.preparationReceipt).isFile());
    assert.deepEqual(receipt.verification, { relay: 'unchecked', oauth: 'unchecked' });
    assert.equal(statSync(join(canonProfile(profile), 'reviewer-launch-receipt.json')).mode & 0o777, 0o600);
  });
  it('async spawn error retains receipt and releases (no unhandled rejection)', async () => {
    const app = makeApp();
    const cache = makeCache();
    const profile = join(dir, 'asyncerr');
    const p = parsedFor(app, cache, profile);
    const child = new FakeChild();
    const deps = baseDeps(child, { fetchImpl: async () => { throw new Error('conn refused'); } });
    child.pid = undefined;
    const promise = launch(p, deps);
    setImmediate(() => child.emit('error', new Error('spawn EACCES')));
    await assert.rejects(promise, /exited before readiness|conn refused|spawn EACCES/);
    const onDisk = JSON.parse(readFileSync(join(canonProfile(profile), 'reviewer-launch-receipt.json'), 'utf8'));
    assert.equal(onDisk.status, 'failed');
  });
  it('forced kill then exit 0 still fails with forcedKill=true', async () => {
    const app = makeApp();
    const cache = makeCache();
    const profile = join(dir, 'forced');
    const p = parsedFor(app, cache, profile);
    const child = new FakeChild();
    child.onKill = (sig) => {
      if (sig === 'SIGTERM') return true; // ignore
      if (sig === 'SIGKILL') {
        child.exitCode = 0;
        child.emit('exit', 0, null);
      }
      return true;
    };
    const layout = profileLayout(canonProfile(profile));
    await assert.rejects(launch(p, baseDeps(child, {
      fetchImpl: async () => ({ ok: true, json: async () => ({ path: canonPages(profile) }) }),
      prepareFn: fakePrepare,
    })), /forced kill|graceful/i);
    const onDisk = JSON.parse(readFileSync(join(canonProfile(profile), 'reviewer-launch-receipt.json'), 'utf8'));
    assert.equal(onDisk.forcedKill, true);
    assert.equal(onDisk.exit.code, 0);
    assert.equal(onDisk.status, 'failed');
  });
  it('preparation failure plus leaked daemon reports BOTH errors', async () => {
    const app = makeApp();
    const cache = makeCache();
    const profile = join(dir, 'prepfail');
    const p = parsedFor(app, cache, profile);
    const child = new FakeChild();
    graceful(child);
    const layout = profileLayout(canonProfile(profile));
    let calls = 0;
    await assert.rejects(launch(p, baseDeps(child, {
      fetchImpl: async () => ({ ok: true, json: async () => ({ path: canonPages(profile) }) }),
      prepareFn: async () => { throw new Error('seed boom'); },
      portClosedFn: async () => { calls += 1; return calls === 1; }, // open after shutdown
    })), /seed boom/);
    const onDisk = JSON.parse(readFileSync(join(canonProfile(profile), 'reviewer-launch-receipt.json'), 'utf8'));
    assert.match(onDisk.error, /seed boom/);
    assert.match(onDisk.cleanupError, /still open/);
  });
  it('receipt/log write failure fails the run', async () => {
    const app = makeApp();
    const cache = makeCache();
    const profile = join(dir, 'writefail');
    const p = parsedFor(app, cache, profile);
    const child = new FakeChild();
    graceful(child);
    const layout = profileLayout(canonProfile(profile));
    await assert.rejects(launch(p, baseDeps(child, {
      fetchImpl: async () => ({ ok: true, json: async () => ({ path: canonPages(profile) }) }),
      prepareFn: fakePrepare,
      writeFileSync: (path, ...args) => {
        if (path === layout.log) throw new Error('disk RO');
        return writeFileSync(path, ...args);
      },
    })), /disk RO/);
    const receipt = JSON.parse(readFileSync(layout.launchReceipt, 'utf8'));
    assert.equal(receipt.status, 'failed');
    assert.match(receipt.evidenceError, /disk RO/);
  });
  it('SIGINT yields cancelled with clean shutdown', { timeout: 2000 }, async () => {
    const app = makeApp();
    const cache = makeCache();
    const profile = join(dir, 'sigint');
    const p = parsedFor(app, cache, profile, 3600);
    const child = new FakeChild();
    graceful(child);
    const layout = profileLayout(canonProfile(profile));
    const deps = {
      ...baseDeps(child, {
        fetchImpl: async () => ({ ok: true, json: async () => ({ path: canonPages(profile) }) }),
        prepareFn: fakePrepare,
      }),
      noSignalTrap: false,
      emit: () => { process.emit('SIGINT'); },
    };
    await assert.rejects(launch(p, deps), /cancelled/);
    const onDisk = JSON.parse(readFileSync(join(canonProfile(profile), 'reviewer-launch-receipt.json'), 'utf8'));
    assert.equal(onDisk.status, 'cancelled');
  });
  for (const signals of [['SIGINT'], ['SIGINT', 'SIGTERM']]) {
    it(`${signals.join(' then ')} sends one graceful stop while the App drains`, { timeout: 2000 }, async () => {
      const p = parsedFor(makeApp(), makeCache(), join(dir, 'delayed-cancel'), 3600);
      const child = new FakeChild();
      let timer;
      child.onKill = (signal) => {
        if (signal === 'SIGTERM' && child.kills.length === 1) {
          timer = setTimeout(() => child.emit('exit', 0, null), 20);
        } else {
          clearTimeout(timer);
          child.emit('exit', 1, null);
        }
        return true;
      };
      try {
        await assert.rejects(launch(p, baseDeps(child, {
          fetchImpl: async () => ({ ok: true, json: async () => ({ path: canonPages(p.profile) }) }),
          prepareFn: fakePrepare,
          noSignalTrap: false,
          graceMs: 200,
          emit: () => { for (const signal of signals) process.emit(signal); },
        })), { code: 'cancelled' });
        assert.deepEqual(child.kills, ['SIGTERM']);
        const receipt = JSON.parse(readFileSync(profileLayout(canonProfile(p.profile)).launchReceipt, 'utf8'));
        assert.equal(receipt.status, 'cancelled');
        assert.deepEqual(receipt.exit, { code: 0, signal: null });
        assert.equal(receipt.cleanupError, undefined);
      } finally {
        clearTimeout(timer);
      }
    });
  }
});
