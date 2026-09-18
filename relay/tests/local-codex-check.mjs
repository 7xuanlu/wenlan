// SPDX-License-Identifier: Apache-2.0
// Opt-in native Codex stdio evidence; an additional opt-in enables a model turn.
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, isAbsolute, join } from 'node:path';
import test from 'node:test';
import { runCodexNegativeCases } from './fixtures/codex-negative-cases.mjs';
import { launch, portClosed } from '../reviewer/launch-candidate.mjs';
import { startCodexClient, CodexRpcError } from './fixtures/codex-client.mjs';
import { prepareModelHome } from './fixtures/codex-model-home.mjs';

const MODEL_TURN_TIMEOUT_MS = 90_000;
const MODEL_CLIENT_DEADLINE_MS = 330_000;
const MODEL_CASES = Object.freeze(['positive', 'negative']);

function textValue(value) {
  if (typeof value === 'string') return value;
  if (Array.isArray(value)) return value.map(textValue).join('');
  if (value && typeof value === 'object') return textValue(value.text ?? value.content ?? '');
  return '';
}

function modelAnswer(proof) {
  return (proof?.items ?? [])
    .filter(item => item?.type === 'agentMessage')
    .map(item => textValue(item.text ?? item.content))
    .filter(Boolean)
    .join('\n')
    .trim();
}

function toolText(item) {
  return textValue(item?.result?.content);
}

function writeObservation(root, name, proof) {
  writeFileSync(join(root, name), JSON.stringify(proof, null, 2), { flag: 'wx', mode: 0o600 });
}

test('isolated Codex calls the real local Wenlan through stdio without relay', { timeout: 360_000 }, async t => {
  assert.equal(process.env.WENLAN_TEST_LOCAL_CODEX, '1');
  const app = process.env.WENLAN_APP_TEST_BIN;
  const codex = process.env.WENLAN_CODEX_BIN;
  const cache = process.env.WENLAN_TEST_FASTEMBED_CACHE;
  for (const value of [app, codex, cache]) assert(value && isAbsolute(value));
  const root = mkdtempSync('/private/tmp/wenlan-local-codex-');
  const modelEnabled = process.env.WENLAN_TEST_MODEL_EXECUTION === '1';
  const modelCases = process.env.WENLAN_CODEX_MODEL_CASES ?? 'positive';
  assert(MODEL_CASES.includes(modelCases), 'model cases must be positive or negative');
  if (modelCases === 'negative') {
    assert(modelEnabled, 'negative model cases require explicit model execution opt-in');
  }
  const home = modelEnabled ? process.env.WENLAN_TEST_MODEL_HOME : join(root, 'codex');
  if (!modelEnabled) mkdirSync(home, { mode: 0o700 });
  let modelProof;
  const calls = [];
  t.diagnostic(`Retained evidence: ${root}`);
  const lifecycle = await launch({ app, sha256: process.env.WENLAN_APP_TEST_SHA256,
    cache, profile: join(root, 'profile'), durationSeconds: 1, confirmSyntheticLibrary: true }, {
    emit: async line => {
      const ready = JSON.parse(line.slice('READY '.length));
      const prepared = JSON.parse(readFileSync(ready.preparationReceipt, 'utf8'));
      const ids = prepared.fixture.mapping;
      const quoted = JSON.stringify;
      const config = `cli_auth_credentials_store = "file"\nmodel_reasoning_effort = "low"\nweb_search = "disabled"\n` +
        `[features]\nview_image = false\nshell_tool = false\nunified_exec = false\n` +
        `js_repl = false\napps = false\nplugins = false\nbrowser_use = false\ncomputer_use = false\n` +
        `[mcp_servers.wenlan_local_synthetic]\ncommand = ${quoted(join(dirname(app), 'wenlan-mcp'))}\n` +
        `args = ${quoted(['--origin-url', ready.daemonUrl, '--agent-name', 'local-codex-check'])}\n` +
        `enabled_tools = ["brief", "recall", "get_page_sources"]\n` +
        `[mcp_servers.wenlan_local_synthetic.env]\nWENLAN_SPACE = "atlas-review"\nWENLAN_NO_AUTOSTART = "1"\n`;
      let cleanupConfig;
      if (modelEnabled) cleanupConfig = await prepareModelHome(home, config);
      else writeFileSync(join(home, 'config.toml'), config, { flag: 'wx', mode: 0o600 });
      const client = startCodexClient(codex, { PATH: process.env.PATH, HOME: home, USERPROFILE: home,
        CODEX_HOME: home, BROWSER: '/usr/bin/false', WENLAN_NO_AUTOSTART: '1',
        WENLAN_MCP_CACHE_DIR: join(root, 'mcp-cache') }, root);
      const deadline = setTimeout(() => { void client.close().catch(() => {}); },
        modelEnabled && modelCases === 'negative' ? MODEL_CLIENT_DEADLINE_MS : 140_000);
      try {
        await client.rpc('initialize', { clientInfo: { name: 'wenlan-local-check', version: '1' } });
        client.notify('initialized');
        if (modelEnabled) assert((await client.rpc('account/read', { refreshToken: false })).account);
        const session = await client.rpc('thread/start', { cwd: root, ephemeral: true,
          model: 'gpt-5.6-luna', allowProviderModelFallback: false,
          sandbox: 'read-only', approvalPolicy: 'on-request' });
        const threadId = session.thread.id;
        const listing = await client.rpc('mcpServerStatus/list', { threadId, detail: 'toolsAndAuthOnly' });
        assert.equal(listing.data.length, 1);
        assert.deepEqual(Object.values(listing.data[0].tools).map(x => x.name).sort(), ['brief', 'get_page_sources', 'recall']);
        async function call(tool, args) {
          const output = await client.rpc('mcpServer/tool/call', { threadId,
            server: 'wenlan_local_synthetic', tool, arguments: args });
          calls.push({ tool, args, output });
          assert(!JSON.stringify(output).includes('UNAUTHORIZED_SENTINEL'));
          return output;
        }
        const brief = await call('brief', {});
        assert.notEqual(brief.isError, true);
        assert(JSON.stringify(brief).includes('Use signed requests'));
        const recall = await call('recall', { query: 'Atlas authentication decision', limit: 3, rerank: false });
        assert.notEqual(recall.isError, true);
        assert(JSON.stringify(recall).includes(ids['mem_atlas-auth']));
        const sources = await call('get_page_sources', { page_id: ids['page_atlas-auth'] });
        assert.notEqual(sources.isError, true);
        assert(JSON.stringify(sources).includes(ids['mem_atlas-auth']));
        // Standard stdio uses the strict process pin, ignoring inbound Space.
        assert.deepEqual(await call('brief', { space: 'private-sentinel' }), brief);
        for (const [tool, args] of [['get_page_sources', { page_id: ids['page_private-sentinel'] }]]) {
          try { assert.equal((await call(tool, args)).isError, true); }
          catch (error) { if (!(error instanceof CodexRpcError)) throw error; calls.push({ tool, args, deniedByRpc: true }); }
        }
        if (modelEnabled && modelCases === 'positive') {
          assert.equal(session.model, 'gpt-5.6-luna');
          assert.equal(session.reasoningEffort, 'low');
          const started = await client.rpc('turn/start', { threadId, input: [{ type: 'text', text_elements: [],
            text: 'Use only the connected Wenlan synthetic library for five fresh checks. First read the project ' +
              'Brief without a topic. Then read the Brief with related context about the Atlas authentication ' +
              'decision. Search for that decision with at most three results and reranking disabled. Show supporting ' +
              'sources for page ' + ids['page_atlas-auth'] + ', then page ' + ids['page_atlas-unavailable'] + '. ' +
              'Perform all five tool calls. Label the five results, cite only source IDs actually returned, and ' +
              'report no available evidence for the empty source list without guessing why. Do not disclose hidden ' +
              'source IDs. Do not use filesystem, shell, web, or other tools, and do not invent evidence.',
          }] });
          modelProof = await client.waitForTurn(threadId, started.turn.id, MODEL_TURN_TIMEOUT_MS);
          writeObservation(root, 'model-observations.json', modelProof);
          assert.equal(modelProof.turn.status, 'completed');
          assert.equal(modelProof.turn.error, null);
          const selected = modelProof.items.filter(item => item.type === 'mcpToolCall');
          assert.equal(selected.length, 5, 'five actual model-selected calls required');
          assert(selected.every(item => item.server === 'wenlan_local_synthetic' &&
            item.status === 'completed' && !item.error && item.result));
          assert.deepEqual(selected.map(item => item.tool).sort(),
            ['brief', 'brief', 'get_page_sources', 'get_page_sources', 'recall']);
          const plainCall = selected.find(item => item.tool === 'brief' && !item.arguments.topic);
          const topicCall = selected.find(item => item.tool === 'brief' && item.arguments.topic);
          const recallCall = selected.find(item => item.tool === 'recall');
          const pageCall = selected.find(item => item.tool === 'get_page_sources' &&
            item.arguments.page_id === ids['page_atlas-auth']);
          const emptyCall = selected.find(item => item.tool === 'get_page_sources' &&
            item.arguments.page_id === ids['page_atlas-unavailable']);
          assert(plainCall && topicCall && recallCall && pageCall && emptyCall,
            'all five mapped positive cases must be exercised');
          assert.equal(recallCall.arguments.limit, 3);
          assert.equal(recallCall.arguments.rerank, false);
          // Standard stdio returns human-readable content, not the relay JSON envelope.
          assert.equal(toolText(plainCall), textValue(brief.content));
          assert(toolText(topicCall).includes(ids['mem_atlas-auth']));
          assert(toolText(recallCall).includes(ids['mem_atlas-auth']));
          assert.equal(toolText(pageCall), textValue(sources.content));
          // Local Standard retains the link with memory:null; QueryOnly omits it.
          const unavailableText = toolText(emptyCall);
          assert.match(unavailableText, /^\d+ sources\n/);
          const unavailable = JSON.parse(unavailableText.slice(unavailableText.indexOf('\n') + 1));
          assert.equal(unavailable.length, 1);
          assert.equal(unavailable[0].memory, null);
          assert.equal(unavailable[0].source.page_id, ids['page_atlas-unavailable']);
          const final = modelAnswer(modelProof);
          assert(/signed requests/i.test(final));
          assert(final.includes(ids['mem_atlas-auth']));
          assert(!final.includes(ids['mem_atlas-unavailable']));
          assert(!JSON.stringify(modelProof).includes('UNAUTHORIZED_SENTINEL'));
          assert(!modelProof.items.some(item =>
            ['commandExecution', 'fileChange', 'webSearch', 'dynamicToolCall'].includes(item.type)));
          t.diagnostic('Actual gpt-5.6-luna low model-selected five positive local cases passed');
        }
        if (modelEnabled && modelCases === 'negative') {
          assert.equal(session.model, 'gpt-5.6-luna');
          assert.equal(session.reasoningEffort, 'low');
          try {
            modelProof = await runCodexNegativeCases(client, threadId);
            writeObservation(root, 'model-negative-observations.json', modelProof);
          } catch (error) {
            writeObservation(root, 'model-negative-failure.json', {
              failure: error.negativeCaseEvidence ?? { message: error.message },
              completed: error.completedNegativeCases ?? [],
            });
            throw error;
          }
          t.diagnostic('Actual gpt-5.6-luna low passed all three submission routing negative cases');
        }
      } finally {
        clearTimeout(deadline);
        try { await client.close(); } finally { if (cleanupConfig) await cleanupConfig(); }
        assert(client.child.exitCode !== null || client.child.signalCode !== null);
        writeFileSync(join(root, 'tool-observations.json'), JSON.stringify(calls, null, 2), { flag: 'wx', mode: 0o600 });
      }
    },
  });
  assert.equal(lifecycle.status, 'completed');
  assert(await portClosed(lifecycle.daemonPort));
  writeFileSync(join(root, 'local-codex-receipt.json'), JSON.stringify({ lifecycle,
    verification: { stdioTools: 'passed', model: modelProof ? 'passed' : 'unchecked', modelCases, ui: 'unchecked', relay: 'unchecked' },
    calls }, null, 2), { flag: 'wx', mode: 0o600 });
});
