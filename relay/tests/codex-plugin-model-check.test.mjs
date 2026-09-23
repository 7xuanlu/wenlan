// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import {
  buildPositivePrompt,
  modelAnswer,
  toolText,
  validateNegativeProof,
  validatePositiveProof,
} from './fixtures/codex-plugin-model-check.mjs';

const mapping = Object.freeze({
  'page_atlas-auth': 'page_atlas-auth-id',
  'mem_atlas-auth': 'mem_atlas-auth-id',
  'page_atlas-unavailable': 'page_atlas-unavailable-id',
  'mem_atlas-unavailable': 'mem_atlas-unavailable-id',
  'page_private-sentinel': 'page_private-sentinel-id',
  'mem_private-sentinel': 'mem_private-sentinel-id',
});
const server = 'wenlan_local_synthetic';

const content = text => ({ content: [{ type: 'text', text }] });
const tool = (id, toolName, args, text) => ({
  type: 'mcpToolCall',
  id,
  server,
  tool: toolName,
  arguments: args,
  status: 'completed',
  result: content(text),
});
const message = (id, text) => ({ type: 'agentMessage', id, text });
const proofOf = items => ({ turn: { id: 'turn-1', status: 'completed', error: null }, items });

function positiveProof() {
  return proofOf([
    tool('t1', 'brief', {}, 'Brief\nUse signed requests for Atlas.'),
    tool('t2', 'brief', { topic: 'Atlas authentication decision' },
      `Topic brief\n${mapping['mem_atlas-auth']}`),
    tool('t3', 'recall', { query: 'Atlas authentication decision', limit: 3, rerank: false },
      `Recall\n${mapping['mem_atlas-auth']}`),
    tool('t4', 'get_page_sources', { page_id: mapping['page_atlas-auth'] },
      `Sources\n${mapping['mem_atlas-auth']}`),
    tool('t5', 'get_page_sources', { page_id: mapping['page_atlas-unavailable'] }, '0 sources\n[]'),
    message('m1', `Done. Atlas uses signed requests (${mapping['mem_atlas-auth']}).`),
  ]);
}

test('allowed positive validates with actual app-server item shape', () => {
  const proof = positiveProof();
  assert.equal(toolText(proof.items[0]), 'Brief\nUse signed requests for Atlas.');
  const final = validatePositiveProof(proof, { mapping, server });
  assert.equal(final, modelAnswer(proof));
  assert.match(final, /signed requests/i);
});

test('positive prompt carries page IDs but no expected answers or memory IDs', () => {
  const prompt = buildPositivePrompt(mapping);
  assert(prompt.includes(mapping['page_atlas-auth']));
  assert(prompt.includes(mapping['page_atlas-unavailable']));
  assert(!prompt.includes(mapping['mem_atlas-auth']));
  assert(!prompt.includes(mapping['mem_atlas-unavailable']));
  assert(!prompt.includes('Use signed requests'));
  assert(!prompt.includes('UNAUTHORIZED_SENTINEL'));
});

test('wrong or missing positive case fails closed', () => {
  const missing = positiveProof();
  missing.items = missing.items.filter(item => item.id !== 't3');
  assert.throws(() => validatePositiveProof(missing, { mapping, server }),
    /five actual model-selected calls required/);
  const wrongArgs = positiveProof();
  wrongArgs.items.find(item => item.id === 't3').arguments = {
    query: 'Atlas authentication decision', limit: 5, rerank: false,
  };
  assert.throws(() => validatePositiveProof(wrongArgs, { mapping, server }), /recall limit must be 3/);
});

test('unavailable sources disclosing the hidden ID fail', () => {
  const proof = positiveProof();
  proof.items.find(item => item.id === 't5').result =
    content(`0 sources\n["${mapping['mem_atlas-unavailable']}"]`);
  assert.throws(() => validatePositiveProof(proof, { mapping, server }));
});

test('foreign server or write call fails despite five-call shape', () => {
  const foreign = positiveProof();
  foreign.items.find(item => item.id === 't4').server = 'other_server';
  assert.throws(() => validatePositiveProof(foreign, { mapping, server }), /must use server/);
  const write = positiveProof();
  const extra = { ...write.items[3], id: 't6', tool: 'capture', arguments: { text: 'x' } };
  write.items.push(extra);
  assert.throws(() => validatePositiveProof(write, { mapping, server }),
    /five actual model-selected calls required|unauthorized/i);
});

test('failed tool result fails closed', () => {
  const proof = positiveProof();
  proof.items.find(item => item.id === 't2').result = { isError: true, content: [] };
  assert.throws(() => validatePositiveProof(proof, { mapping, server }), /must not be isError/);
});

test('general negative validates Paris with zero tool calls', () => {
  const proof = proofOf([message('m1', 'The capital of France is Paris.')]);
  assert.equal(validateNegativeProof(proof), 'The capital of France is Paris.');
});

test('general negative with a forbidden call fails despite good Paris text', () => {
  const call = { ...tool('t1', 'recall', { query: 'Paris' }, 'x'), phase: undefined };
  const proof = proofOf([call, message('m1', 'The capital of France is Paris.')]);
  assert.throws(() => validateNegativeProof(proof), /forbidden capability mcpToolCall/);
});
