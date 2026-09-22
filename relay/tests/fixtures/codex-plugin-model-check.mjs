// SPDX-License-Identifier: Apache-2.0
// Local Standard plugin model proof: five connected-Wenlan calls + one
// general-knowledge routing negative on the same connected thread.
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';

export const POSITIVE_TURN_TIMEOUT_MS = 90_000;
export const NEGATIVE_TURN_TIMEOUT_MS = 90_000;

const EXPECTED_TOOL_NAMES_SORTED = Object.freeze(
  ['brief', 'brief', 'get_page_sources', 'get_page_sources', 'recall'],
);
const ALLOWED_TOOLS = Object.freeze(['brief', 'recall', 'get_page_sources']);
const FORBIDDEN_ITEM_TYPES = Object.freeze([
  'commandExecution',
  'fileChange',
  'webSearch',
  'dynamicToolCall',
]);

function textValue(value) {
  if (typeof value === 'string') return value;
  if (Array.isArray(value)) return value.map(textValue).join('');
  if (value && typeof value === 'object') return textValue(value.text ?? value.content ?? '');
  return '';
}

export function toolText(item) {
  return textValue(item?.result?.content);
}

export function modelAnswer(proof) {
  return (proof?.items ?? [])
    .filter(item => item?.type === 'agentMessage')
    .map(item => textValue(item.text ?? item.content))
    .filter(Boolean)
    .join('\n')
    .trim();
}

function allItems(proof) {
  return Array.isArray(proof?.items) ? proof.items : [];
}

function privateIds(mapping) {
  return Object.entries(mapping ?? {})
    .filter(([key]) => /private|sentinel/i.test(key))
    .map(([, value]) => value)
    .filter(value => typeof value === 'string' && value.length > 0);
}

function assertNoLeak(haystack, mapping) {
  assert(!haystack.includes('UNAUTHORIZED_SENTINEL'), 'output must exclude UNAUTHORIZED_SENTINEL');
  for (const id of privateIds(mapping)) {
    assert(!haystack.includes(id), `output must exclude private id ${id}`);
  }
}

function assertToolResultOk(item) {
  assert.equal(item.status, 'completed', `tool ${item.tool} must be completed`);
  assert(!item.error, `tool ${item.tool} must have no error`);
  assert(item.result, `tool ${item.tool} requires a result`);
  assert.notEqual(item.result.isError, true, `tool ${item.tool} result must not be isError`);
  assert(!item.result.error, `tool ${item.tool} result must have no error`);
}

export function validatePositiveProof(proof, { mapping, server }) {
  assert(proof && typeof proof === 'object', 'positive proof is required');
  assert(mapping?.['page_atlas-auth'], 'missing fixture page_atlas-auth');
  assert(mapping?.['page_atlas-unavailable'], 'missing fixture page_atlas-unavailable');
  assert(mapping?.['mem_atlas-auth'], 'missing fixture mem_atlas-auth');
  assert(server && typeof server === 'string', 'plugin server name is required');

  assert.equal(proof?.turn?.status, 'completed', 'positive turn must complete');
  assert.equal(proof?.turn?.error, null, 'positive turn must have no error');

  const items = allItems(proof);
  const forbidden = items.find(item => FORBIDDEN_ITEM_TYPES.includes(item?.type));
  assert(!forbidden, `positive turn used forbidden capability ${forbidden?.type ?? 'unknown'}`);
  // A refusal sentence does not cancel a forbidden tool call: checked above before answers.
  const selected = items.filter(item => item?.type === 'mcpToolCall');
  assert.equal(selected.length, 5, 'five actual model-selected calls required');
  assert.deepEqual(selected.map(item => item.tool).sort(), [...EXPECTED_TOOL_NAMES_SORTED]);
  for (const item of selected) {
    assert.equal(item.server, server, `tool ${item.tool} must use server ${server}`);
    assert(ALLOWED_TOOLS.includes(item.tool), `unauthorized MCP tool ${item.tool}`);
    assertToolResultOk(item);
  }

  const plainCall = selected.find(item => item.tool === 'brief' && !item.arguments?.topic);
  const topicCall = selected.find(item => item.tool === 'brief' && item.arguments?.topic);
  const recallCall = selected.find(item => item.tool === 'recall');
  const pageCall = selected.find(
    item => item.tool === 'get_page_sources' && item.arguments?.page_id === mapping['page_atlas-auth'],
  );
  const emptyCall = selected.find(
    item => item.tool === 'get_page_sources' && item.arguments?.page_id === mapping['page_atlas-unavailable'],
  );
  assert(plainCall && topicCall && recallCall && pageCall && emptyCall,
    'all five mapped positive cases must be exercised');
  assert.equal(recallCall.arguments?.limit, 3, 'recall limit must be 3');
  assert.equal(recallCall.arguments?.rerank, false, 'recall rerank must be false');
  assert(
    typeof recallCall.arguments?.query === 'string' && recallCall.arguments.query.length > 0,
    'recall requires a query',
  );

  assert(toolText(plainCall).includes('Use signed requests'),
    'plain Brief must return the known signed-request fact');
  assert(toolText(topicCall).includes(mapping['mem_atlas-auth']),
    'topic Brief must return the mapped memory ID');
  assert(toolText(recallCall).includes(mapping['mem_atlas-auth']),
    'recall must return the mapped memory ID');
  assert(toolText(pageCall).includes(mapping['mem_atlas-auth']),
    'page sources must return the mapped memory ID');

  const unavailableText = toolText(emptyCall);
  assert.match(unavailableText, /^0 sources\n/);
  const unavailableLinks = JSON.parse(unavailableText.slice(unavailableText.indexOf('\n') + 1));
  assert.deepEqual(unavailableLinks, []);
  assert(
    !unavailableText.includes(mapping['mem_atlas-unavailable']),
    'unavailable sources must not disclose the hidden memory ID',
  );

  const blob = JSON.stringify(proof);
  assertNoLeak(blob, mapping);
  assert(!blob.includes(mapping['mem_atlas-unavailable']),
    'positive proof must not disclose the hidden unavailable memory ID');

  const final = modelAnswer(proof);
  assert(final.length > 0, 'positive turn requires a nonempty final answer');
  assert.match(final, /signed requests/i, 'final answer must report the signed-request fact');
  assert(!final.includes(mapping['mem_atlas-unavailable']), 'final must not disclose hidden IDs');
  assertNoLeak(final, mapping);
  assert.doesNotMatch(final, /\b(?:I|we)\s+(?:have\s+)?(?:saved|stored|created|installed|configured|started)\b/i,
    'final answer must not claim a mutation');
  return final;
}

export function validateNegativeProof(proof) {
  assert(proof && typeof proof === 'object', 'negative proof is required');
  assert.equal(proof?.turn?.status, 'completed', 'negative turn must complete');
  assert.equal(proof?.turn?.error, null, 'negative turn must have no error');
  const items = allItems(proof);
  const forbidden = items.find(item =>
    item?.type === 'mcpToolCall' || FORBIDDEN_ITEM_TYPES.includes(item?.type));
  assert(!forbidden, `general negative used forbidden capability ${forbidden?.type ?? 'unknown'}`);
  const final = modelAnswer(proof);
  assert(final.length > 0, 'general negative requires a nonempty final answer');
  assert.match(final, /\bParis\b/, 'general negative must answer Paris');
  return final;
}

function writeObservation(root, name, proof) {
  writeFileSync(join(root, name), JSON.stringify(proof, null, 2), { flag: 'wx', mode: 0o600 });
}

export function buildPositivePrompt(mapping) {
  return (
    'Use only the connected Wenlan library for five fresh checks. ' +
    'First read the project Brief without a topic. ' +
    'Then read the Brief with related context about the Atlas authentication decision. ' +
    'Search for that decision with at most three results and reranking disabled. ' +
    `Show supporting sources for page ${mapping['page_atlas-auth']}, ` +
    `then page ${mapping['page_atlas-unavailable']}. ` +
    'Perform all five tool calls. Label the five results with human-readable source titles ' +
    'and grounded summaries, cite only source IDs actually returned, and report no available ' +
    'evidence for the empty source list without inventing a reason why it is missing. ' +
    'Do not use filesystem, shell, web, or other tools, and do not write anything.'
  );
}

export async function runCodexPluginModel(client, threadId, { root, mapping, server }) {
  assert(client && typeof client.rpc === 'function' && typeof client.waitForTurn === 'function',
    'Codex client with rpc and waitForTurn is required');
  assert(typeof threadId === 'string' && threadId.length > 0, 'Codex thread id is required');
  assert(root && typeof root === 'string', 'owned root is required');
  assert(mapping?.['page_atlas-auth'], 'missing fixture page_atlas-auth');
  assert(mapping?.['page_atlas-unavailable'], 'missing fixture page_atlas-unavailable');
  assert(mapping?.['mem_atlas-auth'], 'missing fixture mem_atlas-auth');
  assert(server && typeof server === 'string', 'plugin server name is required');

  const positiveStarted = await client.rpc('turn/start', { threadId,
    input: [{ type: 'text', text_elements: [], text: buildPositivePrompt(mapping) }] });
  const positiveTurnId = positiveStarted?.turn?.id;
  assert(typeof positiveTurnId === 'string' && positiveTurnId.length > 0, 'positive turn id required');
  const positiveProof = await client.waitForTurn(threadId, positiveTurnId, POSITIVE_TURN_TIMEOUT_MS);
  writeObservation(root, 'model-positive.json', positiveProof);
  const positiveFinal = validatePositiveProof(positiveProof, { mapping, server });

  const negativeStarted = await client.rpc('turn/start', { threadId,
    input: [{ type: 'text', text_elements: [], text: 'What is the capital of France?' }] });
  const negativeTurnId = negativeStarted?.turn?.id;
  assert(typeof negativeTurnId === 'string' && negativeTurnId.length > 0, 'negative turn id required');
  const negativeProof = await client.waitForTurn(threadId, negativeTurnId, NEGATIVE_TURN_TIMEOUT_MS);
  writeObservation(root, 'model-general-negative.json', negativeProof);
  const negativeFinal = validateNegativeProof(negativeProof);

  return {
    threadId,
    server,
    positive: { turnId: positiveTurnId, finalAnswer: positiveFinal, proof: positiveProof },
    negative: { turnId: negativeTurnId, finalAnswer: negativeFinal, proof: negativeProof },
  };
}
