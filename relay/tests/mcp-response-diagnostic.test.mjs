// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import { diagnoseMcpResponse, sanitizeTasks, DEVICE_UNAVAILABLE_TEXT } from './fixtures/mcp-response-diagnostic.mjs';

const SECRET = 'SECRET_CANARY_9f8e7d6c5b4a';

const json = (text, status = 200) => ({ status, headers: { 'content-type': 'application/json' }, text });
const flat = value => JSON.stringify(value);

test('successful JSON initialize preserves shape without secrets', () => {
  const d = diagnoseMcpResponse(json(flat({ jsonrpc: '2.0', id: 3, result: { serverInfo: { name: 'x' } } })), 3);
  assert.equal(d.status, 200);
  assert.equal(d.contentType, 'json');
  assert.equal(d.parseOk, true);
  assert.deepEqual(d.messages, [{ idType: 'number', idMatches: true, resultPresent: true, errorCode: null, errorClass: 'none' }]);
  assert(!flat(d).includes('serverInfo'));
});

test('200 JSON-RPC error with known -32000 message classifies safely', () => {
  const d = diagnoseMcpResponse(json(flat({ jsonrpc: '2.0', id: 1, error: { code: -32000, message: DEVICE_UNAVAILABLE_TEXT } })), 1);
  assert.deepEqual(d.messages, [{ idType: 'number', idMatches: true, resultPresent: false, errorCode: -32000, errorClass: 'device-unavailable' }]);
});

test('unrecognized error message maps to other with numeric code only', () => {
  const d = diagnoseMcpResponse(json(flat({ jsonrpc: '2.0', id: 1, error: { code: -32600, message: `boom ${SECRET}`, data: SECRET } })), 1);
  assert.deepEqual(d.messages[0].errorCode, -32600);
  assert.equal(d.messages[0].errorClass, 'other');
  assert(!flat(d).includes(SECRET));
});

test('string id never preserved and never matches numeric id', () => {
  const d = diagnoseMcpResponse(json(flat({ jsonrpc: '2.0', id: `abc-${SECRET}`, result: {} })), 7);
  assert.deepEqual(d.messages, [{ idType: 'string', idMatches: false, resultPresent: true, errorCode: null, errorClass: 'none' }]);
  assert(!flat(d).includes(SECRET));
});

test('numeric id mismatch reports false', () => {
  const d = diagnoseMcpResponse(json(flat({ jsonrpc: '2.0', id: 9, result: {} })), 7);
  assert.equal(d.messages[0].idMatches, false);
});

test('invalid JSON and SSE without data fail parse with empty shapes', () => {
  assert.deepEqual(diagnoseMcpResponse(json('{oops'), 1).parseOk, false);
  assert.deepEqual(diagnoseMcpResponse(json('{oops'), 1).messages, []);
  const sse = { status: 200, headers: { 'content-type': 'text/event-stream' }, text: ': keepalive\n\n' };
  const d = diagnoseMcpResponse(sse, 1);
  assert.equal(d.contentType, 'sse');
  assert.equal(d.parseOk, false);
  assert.deepEqual(d.messages, []);
});

test('SSE data lines parse like JSON', () => {
  const text = `data: ${flat({ jsonrpc: '2.0', id: 2, result: {} })}\n\n`;
  const d = diagnoseMcpResponse({ status: 200, headers: { 'content-type': 'text/event-stream; charset=utf-8' }, text }, 2);
  assert.equal(d.parseOk, true);
  assert.equal(d.messages[0].idMatches, true);
});

test('entries are bounded to 16', () => {
  const batch = Array.from({ length: 40 }, (_, i) => ({ jsonrpc: '2.0', id: i, result: {} }));
  const d = diagnoseMcpResponse(json(flat(batch)), 0);
  assert.equal(d.messages.length, 16);
});

test('no secret survives in any field', () => {
  const bodies = [
    flat({ jsonrpc: '2.0', id: 1, result: { token: SECRET, cookie: SECRET, session: SECRET } }),
    flat({ jsonrpc: '2.0', id: 1, error: { code: -32000, message: SECRET, data: { token: SECRET } } }),
    `data: ${flat({ jsonrpc: '2.0', id: 1, result: { authorization: SECRET } })}\n\n`,
  ];
  for (const [i, text] of bodies.entries()) {
    const headers = i === 2 ? { 'content-type': 'text/event-stream' } : { 'content-type': `application/json; session=${SECRET}` };
    const d = diagnoseMcpResponse({ status: 200, headers, text }, 1);
    assert(!flat(d).includes(SECRET), `body ${i}`);
    assert(!flat(d).includes('authorization'));
  }
});

test('task facts exclude output tails', () => {
  const tasks = [{ name: 'MCP', failed: false, reverseReady: false, child: { exitCode: null, signalCode: null }, tail: SECRET }];
  const facts = sanitizeTasks(tasks);
  assert.deepEqual(facts, [{ name: 'MCP', exitCode: null, signalCode: null, failed: false, reverseReady: false }]);
});

test('task facts reject arbitrary names, values and unbounded lists', () => {
  const task = { name: SECRET, child: { exitCode: SECRET, signalCode: SECRET }, failed: SECRET, reverseReady: SECRET };
  const facts = sanitizeTasks(Array(20).fill(task));
  assert.equal(facts.length, 4);
  assert(!flat(facts).includes(SECRET));
  assert.deepEqual(facts[0], { name: 'unknown', exitCode: null, signalCode: null, failed: false, reverseReady: false });
});
