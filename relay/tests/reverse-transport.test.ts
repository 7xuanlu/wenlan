// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { decodeFrame, encodeBody, encodeFrame, type ReverseFrame } from '../src/reverse-protocol.ts';
import { ReverseTransport } from '../src/reverse-transport.ts';

function setup(deadlineMs = 2000) {
  const sent: ReverseFrame[] = [];
  const listeners = new Set<(frame: ReverseFrame) => void>();
  let closes = 0;
  const transport = new ReverseTransport({
    send(text) {
      const frame = decodeFrame(text);
      sent.push(frame);
      for (const listener of listeners) listener(frame);
    },
    close() { closes++; },
  }, { deadlineMs });
  return {
    transport, sent, get closes() { return closes; },
    credit(id: string, seq: number): Promise<void> {
      const matches = (frame: ReverseFrame) => frame.type === 'credit' && frame.id === id && frame.seq === seq;
      if (sent.some(matches)) return Promise.resolve();
      return new Promise(resolve => {
        const listener = (frame: ReverseFrame) => {
          if (matches(frame)) { listeners.delete(listener); resolve(); }
        };
        listeners.add(listener);
      });
    },
    request(signal?: AbortSignal) {
      const result = transport.request({ path: '/mcp', method: 'POST', headers: {}, body: '' }, signal);
      const id = sent.at(-1)!.id;
      return { result, id };
    },
    frame(frame: ReverseFrame) { transport.receive(encodeFrame(frame)); },
    head(id: string, status = 200) {
      transport.receive(encodeFrame({ v: 1, type: 'response', id, status,
        headers: { 'content-type': 'text/event-stream' } }));
    },
    chunk(id: string, seq: number, body: string | Uint8Array) {
      transport.receive(encodeFrame({ v: 1, type: 'chunk', id, seq,
        body: encodeBody(typeof body === 'string' ? new TextEncoder().encode(body) : body) }));
    },
    end(id: string) { transport.receive(encodeFrame({ v: 1, type: 'end', id })); },
    cancel(id: string) { transport.receive(encodeFrame({ v: 1, type: 'cancel', id })); },
  };
}

test('SSE is delivered incrementally with one pull-driven credit and no prefetch', async () => {
  const s = setup();
  const { id, result } = s.request();
  s.head(id);
  const response = await result;
  assert.equal(response.headers.get('cache-control'), 'no-store');
  assert.equal(s.sent.length, 1);
  const reader = response.body!.getReader();
  const first = reader.read();
  await Promise.resolve();
  assert.deepEqual(s.sent.at(-1), { v: 1, type: 'credit', id, seq: 0 });
  s.chunk(id, 0, 'data: first\n\n');
  assert.equal(new TextDecoder().decode((await first).value), 'data: first\n\n');
  await Promise.resolve();
  assert.equal(s.sent.length, 2);
  const second = reader.read();
  await Promise.resolve();
  assert.deepEqual(s.sent.at(-1), { v: 1, type: 'credit', id, seq: 1 });
  s.chunk(id, 1, 'data: second\n\n');
  assert.equal(new TextDecoder().decode((await second).value), 'data: second\n\n');
  s.end(id);
  assert.equal((await reader.read()).done, true);
  s.transport.disconnect();
});

test('interleaved requests cannot deliver chunks into another response', async () => {
  const s = setup();
  const a = s.request();
  const b = s.request();
  s.head(a.id); s.head(b.id);
  const ra = (await a.result).body!.getReader();
  const rb = (await b.result).body!.getReader();
  const pa = ra.read(); const pb = rb.read();
  await Promise.resolve();
  s.chunk(b.id, 0, 'B'); s.chunk(a.id, 0, 'A');
  assert.equal(new TextDecoder().decode((await pa).value), 'A');
  assert.equal(new TextDecoder().decode((await pb).value), 'B');
  s.end(a.id); s.end(b.id);
  s.transport.disconnect();
});

test('caller cancellation rejects pending headers and ignores validated late data', async () => {
  const s = setup();
  const controller = new AbortController();
  const { id, result } = s.request(controller.signal);
  const failure = assert.rejects(result, /Reverse connector unavailable/);
  controller.abort();
  await failure;
  assert.deepEqual(s.sent.at(-1), { v: 1, type: 'cancel', id });
  s.head(id); s.chunk(id, 0, 'late'); s.end(id);
  assert.equal(s.closes, 0);
  const next = s.request(); s.head(next.id, 204); s.end(next.id);
  assert.equal((await next.result).status, 204);
  s.transport.disconnect();
});

test('peer cancellation rejects pending headers without echoing or closing', async () => {
  const s = setup();
  const { id, result } = s.request();
  const failure = assert.rejects(result, /Reverse connector unavailable/);
  s.cancel(id);
  await failure;
  assert.equal(s.sent.filter(frame => frame.type === 'cancel').length, 0);
  assert.equal(s.closes, 0);
  s.head(id); s.chunk(id, 0, 'late'); s.end(id);
  assert.equal(s.closes, 0);
  s.transport.disconnect();
});

test('peer cancellation errors a pending response body and preserves another request', async () => {
  const s = setup();
  const cancelled = s.request();
  const survivor = s.request();
  s.head(cancelled.id);
  const reader = (await cancelled.result).body!.getReader();
  const pendingRead = reader.read();
  await Promise.resolve();
  const failure = assert.rejects(pendingRead, /Reverse connector unavailable/);
  s.cancel(cancelled.id);
  await failure;
  assert.equal(s.sent.filter(frame => frame.type === 'cancel').length, 0);
  s.head(survivor.id, 204); s.end(survivor.id);
  assert.equal((await survivor.result).status, 204);
  assert.equal(s.closes, 0);
  s.transport.disconnect();
});

test('repeated peer cancellation and validated late frames stay tombstoned', async () => {
  const s = setup();
  const { id, result } = s.request();
  const failure = assert.rejects(result, /Reverse connector unavailable/);
  s.cancel(id);
  await failure;
  const sent = s.sent.length;
  s.cancel(id); s.head(id); s.chunk(id, 0, 'late'); s.end(id);
  assert.equal(s.sent.length, sent);
  assert.equal(s.closes, 0);
  s.transport.disconnect();
});

test('unknown or completed peer cancellation closes the channel', async () => {
  for (const mode of ['unknown', 'completed']) {
    const s = setup();
    let completedId;
    if (mode === 'completed') {
      const completed = s.request();
      completedId = completed.id;
      s.head(completed.id, 204); s.end(completed.id);
      assert.equal((await completed.result).status, 204);
    } else {
      const pending = s.request();
      const failure = assert.rejects(pending.result, /Reverse connector unavailable/);
      s.cancel(mode);
      await failure;
      assert.equal(s.closes, 1);
      continue;
    }
    s.cancel(completedId!);
    assert.equal(s.closes, 1);
  }
});

test('peer cancellation releases capacity for a replacement request', async () => {
  const s = setup();
  const pending = Array.from({ length: 8 }, () => s.request());
  const cancellation = assert.rejects(pending[0].result, /Reverse connector unavailable/);
  s.cancel(pending[0].id);
  await cancellation;
  const replacement = s.request();
  s.head(replacement.id, 204); s.end(replacement.id);
  assert.equal((await replacement.result).status, 204);
  assert.equal(s.sent.filter(frame => frame.type === 'cancel').length, 0);
  const failures = pending.slice(1).map(request => assert.rejects(request.result, /Reverse connector unavailable/));
  s.transport.disconnect();
  for (const failure of failures) await failure;
});

test('reader cancellation propagates a cancel frame and releases capacity', async () => {
  const s = setup();
  const { id, result } = s.request();
  s.head(id);
  await (await result).body!.cancel();
  assert.deepEqual(s.sent.at(-1), { v: 1, type: 'cancel', id });
  s.transport.disconnect();
});

test('deadline bounds both pending headers and idle streaming responses', async () => {
  const s = setup(20);
  const pending = s.request();
  await assert.rejects(pending.result, /Reverse connector unavailable/);
  const stream = s.request(); s.head(stream.id);
  await assert.rejects((await stream.result).body!.getReader().read(), /Reverse connector unavailable/);
  assert.equal(s.sent.filter(f => f.type === 'cancel').length, 2);
  s.transport.disconnect();
});

test('concurrency limit rejects the ninth request without sending it', async () => {
  const s = setup();
  const pending = Array.from({ length: 8 }, () => s.request());
  await assert.rejects(s.transport.request({ path: '/mcp', method: 'GET', headers: {}, body: '' }));
  assert.equal(s.sent.length, 8);
  const failures = pending.map(p => assert.rejects(p.result, /Reverse connector unavailable/));
  s.transport.disconnect();
  for (const failure of failures) await failure;
});

test('malformed data, wrong-direction frames and unknown IDs fail all pending calls closed', async () => {
  for (const text of ['PRIVATE_BAD_FRAME', new Uint8Array([1]),
    encodeFrame({ v: 1, type: 'request', id: 'request', path: '/mcp', method: 'GET', headers: {}, body: '' }),
    encodeFrame({ v: 1, type: 'end', id: 'unknown' }),
    encodeFrame({ v: 1, type: 'credit', id: 'unknown', seq: 0 })]) {
    const s = setup();
    const a = s.request(); const b = s.request();
    const failures = [a, b].map(p => assert.rejects(p.result, error => {
      assert.equal((error as Error).message, 'Reverse connector unavailable'); return true;
    }));
    s.transport.receive(text);
    for (const failure of failures) await failure;
    assert.equal(s.closes, 1);
    await assert.rejects(s.transport.request({ path: '/mcp', method: 'GET', headers: {}, body: '' }));
  }
});

test('unsolicited, out-of-order, duplicate-head and no-body chunks close the connection', async () => {
  for (const mode of ['unsolicited', 'sequence', 'duplicate', 'no-body']) {
    const s = setup();
    const { id, result } = s.request();
    s.head(id, mode === 'no-body' ? 204 : 200);
    const response = await result;
    const reader = response.body?.getReader();
    const pendingRead = mode === 'sequence' ? reader!.read() : undefined;
    const failure = pendingRead ? assert.rejects(pendingRead) : undefined;
    await Promise.resolve();
    if (mode === 'duplicate') s.head(id);
    else s.chunk(id, mode === 'sequence' ? 1 : 0, 'invalid');
    if (failure) await failure;
    else if (reader) await assert.rejects(reader.read());
    assert.equal(s.closes, 1, mode);
  }
});

test('response byte limit is enforced across chunks, not from headers', async () => {
  const s = setup();
  const { id, result } = s.request(); s.head(id);
  const reader = (await result).body!.getReader();
  const chunk = new Uint8Array(16 * 1024);
  for (let seq = 0; seq < 128; seq++) {
    const pending = reader.read(); await s.credit(id, seq);
    s.chunk(id, seq, chunk);
    assert.equal((await pending).value!.length, chunk.length);
  }
  const failure = assert.rejects(reader.read()); await s.credit(id, 128);
  s.chunk(id, 128, new Uint8Array([1]));
  await failure;
  assert.equal(s.closes, 1);
});

test('send failures reject instead of leaving a timer or pending response', async () => {
  let closed = false;
  const transport = new ReverseTransport({ send() { throw new Error('PRIVATE'); }, close() { closed = true; } });
  await assert.rejects(transport.request({ path: '/mcp', method: 'GET', headers: {}, body: '' }),
    { message: 'Reverse connector unavailable' });
  assert.equal(closed, true);
});

test('response construction failures reject the original request immediately', async t => {
  const s = setup();
  const { id, result } = s.request();
  t.mock.method(globalThis, 'Response', function () { throw new Error('PRIVATE_HEADER_ERROR'); });
  const failure = assert.rejects(result, { message: 'Reverse connector unavailable' });
  s.head(id);
  await failure;
  assert.equal(s.closes, 1);
});

test('fragmented responses have a finite chunk count even below the byte limit', async () => {
  const s = setup();
  const { id, result } = s.request(); s.head(id);
  const reader = (await result).body!.getReader();
  for (let seq = 0; seq < 4096; seq++) {
    const pending = reader.read(); await s.credit(id, seq);
    s.chunk(id, seq, 'x'); await pending;
  }
  const failure = assert.rejects(reader.read()); await s.credit(id, 4096);
  s.chunk(id, 4096, 'x');
  await failure;
  assert.equal(s.closes, 1);
});

test('pre-aborted calls and invalid outbound requests send nothing', async () => {
  const s = setup();
  const input = { path: '/mcp' as const, method: 'GET' as const, headers: {}, body: '' };
  await assert.rejects(s.transport.request(input, AbortSignal.abort()));
  await assert.rejects(s.transport.request({ ...input, headers: { cookie: 'PRIVATE' } }));
  assert.equal(s.sent.length, 0);
  s.transport.disconnect();
});
