// SPDX-License-Identifier: Apache-2.0
// Native Codex app-server client for isolated integration tests, not production.
import { spawn } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';

export class CodexRpcError extends Error {
  constructor(method, code) {
    super(`Codex ${method} returned RPC error ${Number.isInteger(code) ? code : 'unknown'}`);
  }
}

const MAX_NOTIFICATIONS = 512;
const MAX_NOTIFICATION_BYTES = 4 * 1024 * 1024;
const INTERRUPT_TERMINAL_WAIT_MS = 5_000;

export function startCodexClient(binary, env, cwd, { spawn: spawnProcess = spawn } = {}) {
  const child = spawnProcess(binary, ['app-server', '--stdio', '--strict-config'], {
    env, cwd, stdio: ['pipe', 'pipe', 'pipe'],
  });
  const pending = new Map();
  const waiters = new Set();
  const notifications = [];
  let nextId = 0, buffer = '', ended = false, failure = null, notificationBytes = 0;

  function fail(error) {
    failure ??= error;
    for (const item of pending.values()) { clearTimeout(item.timer); item.reject(failure); }
    pending.clear();
    for (const waiter of waiters) { clearTimeout(waiter.timer); waiter.reject(failure); }
    waiters.clear();
  }

  function terminate() {
    if (!ended) child.kill('SIGTERM');
  }

  function turnCompletion(threadId, turnId) {
    const completed = notifications.find(message => message.method === 'turn/completed'
      && message.params?.threadId === threadId && message.params?.turn?.id === turnId);
    if (!completed) return null;
    return {
      turn: completed.params.turn,
      items: notifications
        .filter(message => message.method === 'item/completed'
          && message.params?.threadId === threadId && message.params?.turnId === turnId)
        .map(message => message.params.item),
    };
  }

  function settleWaiters() {
    for (const waiter of waiters) {
      const result = turnCompletion(waiter.threadId, waiter.turnId);
      if (!result) continue;
      waiters.delete(waiter);
      clearTimeout(waiter.timer);
      waiter.resolve(result);
    }
  }

  function waitForCompletion(threadId, turnId, timeoutMs, onTimeout) {
    if (failure) return Promise.reject(failure);
    const result = turnCompletion(threadId, turnId);
    if (result) return Promise.resolve(result);
    if (ended) return Promise.reject(new Error('Codex app-server is not running'));

    return new Promise((resolve, reject) => {
      const waiter = { threadId, turnId, resolve, reject, timer: null };
      waiter.timer = setTimeout(() => {
        waiters.delete(waiter);
        const timeout = onTimeout?.() ?? Promise.reject(new Error(`Codex turn ${turnId} deadline`));
        Promise.resolve(timeout).then(resolve, reject);
      }, timeoutMs);
      waiters.add(waiter);
    });
  }

  const exited = new Promise(resolve => {
    const done = () => { ended = true; fail(new Error('Owned Codex app-server exited')); resolve(); };
    child.once('exit', done);
    child.once('error', done);
  });
  child.stderr.resume();
  child.stdout.setEncoding('utf8');
  child.stdin.on('error', () => fail(new Error('Codex input closed')));
  child.stdout.on('data', data => {
    buffer += data;
    if (Buffer.byteLength(buffer) > 4 * 1024 * 1024) {
      fail(new Error('Codex protocol buffer limit exceeded'));
      child.kill('SIGTERM');
      return;
    }
    let newline;
    while ((newline = buffer.indexOf('\n')) >= 0) {
      const line = buffer.slice(0, newline);
      buffer = buffer.slice(newline + 1);
      if (!line.trim()) continue;
      let message;
      try { message = JSON.parse(line); }
      catch { fail(new Error('Invalid Codex JSON response')); continue; }
      if (message.method) {
        if (message.id !== undefined) {
          // Never approve an unexpected server-initiated action or elicitation.
          child.stdin.write(`${JSON.stringify({ id: message.id,
            error: { code: -32601, message: 'Test client does not authorize server requests' } })}\n`);
          fail(new Error(`Unexpected Codex server request: ${message.method}`));
          terminate();
          continue;
        }
        const bytes = Buffer.byteLength(line, 'utf8');
        if (notifications.length >= MAX_NOTIFICATIONS
          || notificationBytes + bytes > MAX_NOTIFICATION_BYTES) {
          fail(new Error('Codex notification capture limit exceeded'));
          terminate();
          return;
        }
        notifications.push(message);
        notificationBytes += bytes;
        settleWaiters();
        continue;
      }
      const item = pending.get(message.id);
      if (!item) continue;
      pending.delete(message.id);
      clearTimeout(item.timer);
      if (message.error) item.reject(new CodexRpcError(item.method, message.error.code));
      else item.resolve(message.result);
    }
  });
  const notify = (method, params) => child.stdin.write(`${JSON.stringify({ method, ...(params ? { params } : {}) })}\n`);
  function rpc(method, params) {
    if (failure) return Promise.reject(failure);
    if (ended) return Promise.reject(new Error('Codex app-server is not running'));
    const id = ++nextId;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { pending.delete(id); reject(new Error(`Codex ${method} deadline`)); }, 45_000);
      pending.set(id, { resolve, reject, timer, method });
      child.stdin.write(`${JSON.stringify({ id, method, params })}\n`);
    });
  }

  async function waitForTurn(threadId, turnId, timeoutMs = 90_000) {
    if (!Number.isFinite(timeoutMs) || timeoutMs < 0) {
      throw new RangeError('Codex turn wait timeout must be a non-negative finite number');
    }
    return waitForCompletion(threadId, turnId, timeoutMs, async () => {
      const timeoutError = new Error(`Codex turn ${turnId} deadline`);
      let interruptError;
      try {
        await rpc('turn/interrupt', { threadId, turnId });
      } catch (error) {
        interruptError = error;
      }
      try {
        timeoutError.terminal = await waitForCompletion(
          threadId, turnId, INTERRUPT_TERMINAL_WAIT_MS,
        );
      } catch (error) {
        timeoutError.terminalError = error;
      }
      if (interruptError) timeoutError.interruptError = interruptError;
      throw timeoutError;
    });
  }

  return {
    rpc, notify,
    waitForTurn,
    async waitForOAuth(name, timeoutMs = 30_000) {
      if (!Number.isFinite(timeoutMs) || timeoutMs < 0 || timeoutMs > 60_000) {
        throw new RangeError('Codex OAuth wait must be bounded');
      }
      const deadline = Date.now() + timeoutMs;
      while (true) {
        if (failure) throw failure;
        const completed = notifications.find(message => message.method === 'mcpServer/oauthLogin/completed'
          && message.params?.name === name && message.params?.threadId == null);
        if (completed) {
          if (completed.params.success !== true || completed.params.error) {
            throw new Error('Codex MCP OAuth login failed');
          }
          return;
        }
        if (ended) throw new Error('Codex app-server is not running');
        if (Date.now() >= deadline) throw new Error('Codex MCP OAuth completion deadline');
        await delay(Math.min(50, deadline - Date.now()));
      }
    },
    child,
    exited,
    async close() {
      child.stdin.end();
      if (!await Promise.race([exited.then(() => true), delay(5000, false, { ref: false })])) {
        child.kill('SIGTERM');
        if (!await Promise.race([exited.then(() => true), delay(5000, false, { ref: false })])) {
          child.kill('SIGKILL');
          await Promise.race([exited, delay(3000, undefined, { ref: false })]);
          throw new Error('Owned Codex app-server required forced termination');
        }
      }
    },
  };
}
