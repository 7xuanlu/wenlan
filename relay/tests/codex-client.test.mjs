// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { test } from 'node:test';
import { startCodexClient } from './fixtures/codex-client.mjs';

class FakeStream extends EventEmitter {
  constructor() {
    super();
    this.writes = [];
    this.ended = false;
  }

  setEncoding() {}
  resume() {}

  write(chunk) {
    this.writes.push(String(chunk));
    this.onWrite?.(String(chunk));
    return true;
  }

  end() {
    this.ended = true;
    this.onEnd?.();
  }
}

class FakeChild extends EventEmitter {
  constructor() {
    super();
    this.stdout = new FakeStream();
    this.stderr = new FakeStream();
    this.stdin = new FakeStream();
    this.kills = [];
    this.exitCode = null;
    this.signalCode = null;
    this.stdin.onWrite = line => this.onClientMessage?.(JSON.parse(line));
    this.stdin.onEnd = () => queueMicrotask(() => this.exit(0));
  }

  emitServer(message) {
    this.stdout.emit('data', `${JSON.stringify(message)}\n`);
  }

  response(id, result) {
    this.emitServer({ id, result });
  }

  notify(method, params) {
    this.emitServer({ method, params });
  }

  exit(code = 0, signal = null) {
    this.exitCode = code;
    this.signalCode = signal;
    this.emit('exit', code, signal);
  }

  kill(signal) {
    this.kills.push(signal);
    this.exit(null, signal);
    return true;
  }
}

function createFakeClient({ onClientMessage } = {}) {
  const child = new FakeChild();
  child.onClientMessage = onClientMessage;
  const spawnCalls = [];
  const client = startCodexClient('fake-codex', {}, process.cwd(), {
    spawn: (...args) => {
      spawnCalls.push(args);
      return child;
    },
  });
  return { child, client, spawnCalls };
}

const turn = (id, status = 'completed') => ({
  id,
  items: [],
  itemsView: 'full',
  status,
  error: null,
  startedAt: null,
  completedAt: null,
  durationMs: null,
});

const completed = (threadId, turnId) => ({
  method: 'turn/completed',
  params: { threadId, turn: turn(turnId) },
});

const itemCompleted = (threadId, turnId, item) => ({
  method: 'item/completed',
  params: { threadId, turnId, item, completedAtMs: 1 },
});

test('OAuth completion requires the matching app-scoped success notification', async () => {
  const { child, client } = createFakeClient();
  child.notify('mcpServer/oauthLogin/completed', { name: 'other', success: true });
  child.notify('mcpServer/oauthLogin/completed', { name: 'wenlan', threadId: 'other-thread', success: true });
  await assert.rejects(client.waitForOAuth('wenlan', 0), /completion deadline/);
  child.notify('mcpServer/oauthLogin/completed', { name: 'wenlan', threadId: null, success: true, error: null });
  await client.waitForOAuth('wenlan', 0);
  await client.close();
});

test('OAuth waits for completion and redacts provider errors', async () => {
  const { child, client } = createFakeClient();
  const waiting = client.waitForOAuth('wenlan', 500);
  child.notify('mcpServer/oauthLogin/completed', { name: 'wenlan', success: false, error: 'SECRET_PROVIDER_DETAIL' });
  await assert.rejects(waiting, error => error.message === 'Codex MCP OAuth login failed');
  await client.close();
});

test('OAuth cannot succeed after app-server exit or an invalid wait bound', async () => {
  const { child, client } = createFakeClient();
  await assert.rejects(client.waitForOAuth('wenlan', Infinity), RangeError);
  child.exit();
  await assert.rejects(client.waitForOAuth('wenlan', 0), /exited/);
  await client.close();
});

test('waitForTurn uses an early completion notification, not the turn/start RPC result', async () => {
  const { child, client, spawnCalls } = createFakeClient({
    onClientMessage: message => {
      if (message.method === 'turn/start') child.response(message.id, { turn: turn('turn-early', 'inProgress') });
    },
  });

  assert.deepEqual(spawnCalls[0][1], ['app-server', '--stdio', '--strict-config']);
  const started = await client.rpc('turn/start', { threadId: 'thread-early' });
  assert.equal(started.turn.status, 'inProgress');
  child.emitServer(completed('thread-early', 'turn-early'));

  const result = await client.waitForTurn('thread-early', 'turn-early');
  assert.equal(result.turn.id, 'turn-early');
  assert.equal(result.turn.status, 'completed');
  assert.deepEqual(result.items, []);
  await client.close();
});

test('waitForTurn ignores unrelated thread/turn notifications and collects completed tools', async () => {
  const { child, client } = createFakeClient();
  const waiting = client.waitForTurn('thread-match', 'turn-match', 500);

  child.emitServer(completed('thread-other', 'turn-match'));
  child.emitServer(completed('thread-match', 'turn-other'));
  child.emitServer(itemCompleted('thread-other', 'turn-match', { type: 'mcpToolCall', id: 'other' }));
  child.emitServer(itemCompleted('thread-match', 'turn-match', { type: 'mcpToolCall', id: 'tool-1', tool: 'brief' }));
  child.emitServer(itemCompleted('thread-match', 'turn-match', { type: 'mcpToolCall', id: 'tool-2', tool: 'recall' }));
  child.emitServer(completed('thread-match', 'turn-match'));

  const result = await waiting;
  assert.equal(result.turn.id, 'turn-match');
  assert.deepEqual(result.items.map(item => item.id), ['tool-1', 'tool-2']);
  await client.close();
});

test('waitForTurn interrupts on timeout, waits for terminal notification, and still rejects the evidence', async () => {
  const { child, client } = createFakeClient({
    onClientMessage: message => {
      if (message.method === 'turn/interrupt') {
        assert.deepEqual(message.params, { threadId: 'thread-timeout', turnId: 'turn-timeout' });
        child.response(message.id, {});
        queueMicrotask(() => child.emitServer(completed('thread-timeout', 'turn-timeout')));
      }
    },
  });

  const error = await client.waitForTurn('thread-timeout', 'turn-timeout', 5)
    .then(() => assert.fail('waitForTurn unexpectedly resolved'), error => error);
  assert.match(error.message, /deadline/);
  assert.equal(error.terminal.turn.status, 'completed');
  assert.equal(child.kills.length, 0);
  await client.close();
});

test('unexpected server requests are denied and fail the waiter', async () => {
  const { child, client } = createFakeClient();
  const waiting = client.waitForTurn('thread-request', 'turn-request', 500);
  child.emitServer({ id: 77, method: 'item/commandExecution/requestApproval', params: {} });

  await assert.rejects(waiting, /server request/);
  const responses = child.stdin.writes.map(line => JSON.parse(line));
  assert.deepEqual(responses.at(-1), {
    id: 77,
    error: { code: -32601, message: 'Test client does not authorize server requests' },
  });
  assert.equal(responses.at(-1).result, undefined);
  await client.close();
});

test('notification capture is bounded and terminates the owned process on overflow', async () => {
  const { child, client } = createFakeClient();
  const waiting = client.waitForTurn('thread-bound', 'turn-bound', 1_000);
  for (let index = 0; index < 513; index += 1) child.notify('warning', { index });

  await assert.rejects(waiting, /notification capture limit exceeded/);
  assert.deepEqual(child.kills, ['SIGTERM']);
  await client.close();
});

test('buffered completion cannot hide a later protocol failure', async () => {
  const { child, client } = createFakeClient();
  child.emitServer(completed('thread-stale', 'turn-stale'));
  child.emitServer({ id: 88, method: 'item/commandExecution/requestApproval', params: {} });
  await assert.rejects(client.waitForTurn('thread-stale', 'turn-stale'), /server request/);
  await client.close();
});

test('process exit rejects pending proof and resolves exited', async () => {
  const { child, client } = createFakeClient();
  const waiting = client.waitForTurn('thread-exit', 'turn-exit', 500);
  child.exit(17, 'SIGTERM');

  await assert.rejects(waiting, /app-server exited/);
  await client.exited;
  assert.equal(child.exitCode, 17);
  await client.close();
});
