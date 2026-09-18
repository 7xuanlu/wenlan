// SPDX-License-Identifier: Apache-2.0
// Synchronous candidate profile state + lock helpers. Node builtins only.

import {
  openSync, closeSync, readSync, writeFileSync, fsyncSync, fstatSync, fchmodSync,
  lstatSync, statSync, realpathSync, renameSync, unlinkSync, chmodSync,
} from 'node:fs';
import { constants } from 'node:fs';
import { join, isAbsolute, normalize, basename } from 'node:path';

export const LOCK_NAME = '.reviewer-launch.lock';
export const STATE_NAME = 'reviewer-profile.json';
export const STATE_BYTES_MAX = 64 * 1024;
const HEX64 = /^[0-9a-f]{64}$/;
const RESERVED_PORTS = new Set([7878, 1420, 18080, 18081, 18082, 18083]);
const SECRET_KEY = /(secret|token|password|credential|api[_-]?key)/i;

function currentUid() {
  if (typeof process.getuid !== 'function') throw new Error('POSIX ownership checks required');
  return process.getuid();
}

function requireCanonicalAbsolute(p, label) {
  if (typeof p !== 'string' || !isAbsolute(p)) throw new Error(`${label} must be absolute`);
  if (normalize(p) !== p) throw new Error(`${label} must be canonical`);
  if (p.split('/').includes('..')) throw new Error(`${label} must be canonical`);
}

function checkProfileDir(profile) {
  requireCanonicalAbsolute(profile, 'profile');
  const lst = lstatSync(profile);
  if (lst.isSymbolicLink()) throw new Error('profile must be a real directory');
  const st = statSync(profile);
  if (!st.isDirectory()) throw new Error('profile must be a real directory');
  let real;
  try { real = realpathSync(profile); } catch { throw new Error('profile must be a real directory'); }
  if (real !== profile) throw new Error('profile must be canonical real path');
  if (st.uid !== currentUid()) throw new Error('profile foreign uid');
  if ((st.mode & 0o777) & 0o077) throw new Error('profile permissions too open');
  return st;
}

function checkPort(n) {
  if (!Number.isInteger(n) || n < 1 || n > 65535) throw new Error('port out of range');
  if (RESERVED_PORTS.has(n)) throw new Error(`reserved port ${n}`);
}

function scanSecrets(value, depth = 0) {
  if (value === null || typeof value !== 'object' || depth > 3) return;
  for (const k of Object.keys(value)) {
    if (SECRET_KEY.test(k)) throw new Error(`secret field rejected: ${k}`);
    scanSecrets(value[k], depth + 1);
  }
}

export function validateProfileState(state) {
  if (state === null || typeof state !== 'object' || Array.isArray(state)) throw new Error('state must be object');
  scanSecrets(state);
  if (state.schema !== 1) throw new Error('schema must be 1');
  if (state.kind !== 'wenlan-isolated-review') throw new Error('kind mismatch');
  for (const f of ['profile', 'app', 'cache']) {
    requireCanonicalAbsolute(state[f], f);
  }
  for (const f of ['appSha256', 'preparationSha256']) {
    if (typeof state[f] !== 'string' || !HEX64.test(state[f])) throw new Error(`${f} must be lowercase hex64`);
  }
  const ports = state.ports;
  if (ports === null || typeof ports !== 'object' || Array.isArray(ports)) throw new Error('ports must be object');
  checkPort(ports.daemonPort);
  checkPort(ports.uiPort);
  if (ports.daemonPort === ports.uiPort) throw new Error('ports must be distinct');
  return true;
}

export function acquireProfileLock(profile) {
  checkProfileDir(profile);
  const lockPath = join(profile, LOCK_NAME);
  let fd;
  try {
    fd = openSync(
      lockPath,
      constants.O_CREAT | constants.O_EXCL | constants.O_WRONLY | constants.O_NOFOLLOW,
      0o600,
    );
  } catch (err) {
    throw new Error(`profile lock collision: ${err.code ?? err.message}`);
  }
  let st;
  try {
    st = fstatSync(fd);
  } catch (err) {
    try { closeSync(fd); } catch {}
    throw err;
  }
  if (!st.isFile()) {
    try { closeSync(fd); } catch {}
    throw new Error('lock is not a regular file');
  }
  try { fchmodSync(fd, 0o600); } catch (error) { closeSync(fd); throw error; }
  const own = { dev: st.dev, ino: st.ino };
  let done = false; // true only after successful release/retain
  let fdOpen = true;

  function currentMatches() {
    let lst;
    try { lst = lstatSync(lockPath); } catch { return false; }
    if (lst.isSymbolicLink()) return false;
    let cur;
    try { cur = statSync(lockPath); } catch { return false; }
    if (!cur.isFile()) return false;
    return cur.dev === own.dev && cur.ino === own.ino;
  }

  return {
    lockPath,
    release() {
      if (done) return;
      if (!currentMatches()) throw new Error('lock substituted; refusing unlink');
      unlinkSync(lockPath);
      closeSync(fd);
      fdOpen = false;
      done = true;
    },
    retain() {
      if (done) return;
      if (fdOpen) closeSync(fd);
      fdOpen = false;
      done = true;
    },
  };
}

export function saveProfileState(profile, state) {
  validateProfileState(state);
  checkProfileDir(profile);
  if (state.profile !== profile) throw new Error('state profile mismatch');
  const body = Buffer.from(JSON.stringify(state), 'utf8');
  if (body.length > STATE_BYTES_MAX) throw new Error('state oversize');
  const target = join(profile, STATE_NAME);
  try {
    const lst = lstatSync(target);
    if (lst.isSymbolicLink()) throw new Error('state target is symlink');
    const cur = statSync(target);
    if (!cur.isFile()) throw new Error('state target not regular file');
    if (cur.uid !== currentUid()) throw new Error('state foreign uid');
    if ((cur.mode & 0o777) & 0o077) throw new Error('state permissions too open');
  } catch (err) {
    if (err.code !== 'ENOENT') throw err;
  }
  const tmp = join(profile, `.${STATE_NAME}.${process.pid}.${Date.now()}.${Math.floor(Math.random() * 1e6)}.tmp`);
  if (basename(tmp).includes('/') || tmp.slice(0, profile.length) !== profile) throw new Error('bad temp path');
  let fd = -1;
  let created = false;
  try {
    fd = openSync(
      tmp,
      constants.O_CREAT | constants.O_EXCL | constants.O_WRONLY | constants.O_NOFOLLOW,
      0o600,
    );
    created = true;
    writeFileSync(fd, body);
    fsyncSync(fd);
    closeSync(fd);
    fd = -1;
    chmodSync(tmp, 0o600);
    renameSync(tmp, target);
    created = false;
  } catch (err) {
    if (fd !== -1) try { closeSync(fd); } catch {}
    if (created) try { unlinkSync(tmp); } catch {}
    throw err;
  }
}

export function readProfileState(profile) {
  checkProfileDir(profile);
  const target = join(profile, STATE_NAME);
  const lst = lstatSync(target);
  if (lst.isSymbolicLink()) throw new Error('state is symlink');
  const fd = openSync(target, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    const st = fstatSync(fd);
    if (!st.isFile()) throw new Error('state not regular file');
    if (st.uid !== currentUid()) throw new Error('state foreign uid');
    if ((st.mode & 0o777) & 0o077) throw new Error('state permissions too open');
    if (st.size > STATE_BYTES_MAX) throw new Error('state oversize');
    const chunks = [];
    let total = 0;
    const buf = Buffer.alloc(Math.min(Math.max(st.size, 1), STATE_BYTES_MAX + 1));
    for (;;) {
      const n = readSync(fd, buf, 0, buf.length, null);
      if (n === 0) break;
      total += n;
      if (total > STATE_BYTES_MAX) throw new Error('state oversize');
      chunks.push(Buffer.from(buf.subarray(0, n)));
    }
    let data;
    try { data = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(Buffer.concat(chunks))); }
    catch { throw new Error('state malformed JSON'); }
    validateProfileState(data);
    if (data.profile !== profile) throw new Error('state profile mismatch');
    return data;
  } finally {
    try { closeSync(fd); } catch {}
  }
}
