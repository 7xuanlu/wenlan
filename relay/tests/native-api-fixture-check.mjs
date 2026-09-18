// SPDX-License-Identifier: Apache-2.0
// Opt-in local preparation evidence; never enrolls a device or accesses a personal runtime.
import assert from 'node:assert/strict';
import test from 'node:test';
import { execFile, spawn } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { mkdir, mkdtemp, readFile, realpath, writeFile } from 'node:fs/promises';
import { createServer, createConnection } from 'node:net';
import { tmpdir } from 'node:os';
import { isAbsolute, join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { seedReviewerViaApi } from './fixtures/reviewer-api-seed.mjs';

async function assertListenerClosed(port) {
  await new Promise((resolve, reject) => {
    const socket = createConnection({ host: '127.0.0.1', port });
    socket.once('connect', () => { socket.destroy(); reject(new Error(`owned listener ${port} remains open`)); });
    socket.once('error', error => error.code === 'ECONNREFUSED' ? resolve() : reject(error));
    socket.setTimeout(3000, () => { socket.destroy(); reject(new Error(`listener ${port} probe timed out`)); });
  });
}

test('prepare a fresh local reviewer library through normal daemon APIs', { timeout: 240_000 }, async t => {
  assert.equal(process.env.WENLAN_TEST_API_FIXTURE, '1', 'explicit isolated native test required');
  assert.notEqual(process.platform, 'win32', 'this process driver is POSIX-only evidence');
  const binary = process.env.WENLAN_TEST_SERVER_BIN;
  const cache = process.env.WENLAN_TEST_FASTEMBED_CACHE;
  for (const path of [binary, cache]) assert(path && isAbsolute(path), 'absolute binary and existing model cache required');
  const hash = createHash('sha256').update(await readFile(binary)).digest('hex');
  const root = await realpath(await mkdtemp(join(tmpdir(), 'wenlan-api-fixture-')));
  t.diagnostic(`Isolated fixture and process receipts: ${root}`);
  const home = join(root, 'home');
  const pages = join(root, 'pages');
  for (const path of [home, pages]) await mkdir(path, { mode: 0o700 });
  await writeFile(join(root, 'config.json'), JSON.stringify({
    knowledge_path: pages, setup_completed: true, reranker_mode: 'off',
  }), { flag: 'wx', mode: 0o600 });
  const listener = createServer();
  await new Promise((resolve, reject) => listener.once('error', reject).listen(0, '127.0.0.1', resolve));
  const port = listener.address().port;
  await new Promise(resolve => listener.close(resolve));
  assert.notEqual(port, 7878);
  const daemonUrl = `http://127.0.0.1:${port}`;
  const child = spawn(binary, [], {
    cwd: root,
    env: {
      PATH: process.env.PATH, LANG: 'en_US.UTF-8',
      ...(process.env.TMPDIR ? { TMPDIR: process.env.TMPDIR } : {}),
      HOME: home, USERPROFILE: home, WENLAN_DATA_DIR: root,
      WENLAN_NO_AUTOSTART: '1', WENLAN_PORT: String(port),
      WENLAN_BIND_ADDR: `127.0.0.1:${port}`, WENLAN_RERANKER_MODE: 'off',
      WENLAN_TEST_FASTEMBED_CACHE: cache,
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let output = '';
  let startupError;
  for (const pipe of [child.stdout, child.stderr]) pipe.on('data', data => {
    output = (output + data).slice(-32_768);
  });
  const exited = new Promise(resolve => {
    child.once('error', error => { startupError = error; resolve({ error: error.message }); });
    child.once('exit', (code, signal) => resolve({ code, signal }));
  });
  const request = async (path, body) => {
    const response = await fetch(`${daemonUrl}${path}`, {
      method: body === undefined ? 'GET' : 'POST',
      body: body === undefined ? undefined : JSON.stringify(body),
      redirect: 'error', signal: AbortSignal.timeout(3000),
      headers: { 'content-type': 'application/json', 'X-Agent-Name': 'reviewer-api-check', 'X-Wenlan-Space': 'atlas-review' },
    });
    assert.equal(response.ok, true, `${path}: ${response.status}`);
    return response.json();
  };
  try {
    let ready = false;
    const deadline = Date.now() + 45_000;
    while (Date.now() < deadline) {
      if (startupError) throw startupError;
      assert.equal(child.exitCode, null, 'daemon exited during startup');
      assert.equal(child.signalCode, null, 'daemon terminated during startup');
      try {
        const location = await request('/api/knowledge/path');
        assert.deepEqual(location, { path: pages });
        ready = true;
        break;
      } catch (error) {
        if (error instanceof assert.AssertionError && error.actual !== false) throw error;
      }
      await delay(200);
    }
    assert(ready, `daemon did not become ready; retained logs at ${root}`);
    let mapping;
    if (process.env.WENLAN_TEST_REVIEWER_PREPARE === '1') {
      const receiptPath = join(root, 'reviewer-preparation.json');
      const preparationScript = process.env.WENLAN_TEST_REVIEWER_PREPARE_BIN
        ?? fileURLToPath(new URL('../reviewer/prepare.mjs', import.meta.url));
      assert(isAbsolute(preparationScript), 'preparation script path must be absolute');
      await promisify(execFile)(process.execPath, [
        preparationScript,
        '--daemon-url', daemonUrl, '--knowledge-path', pages, '--output', receiptPath,
        '--confirm-synthetic-library',
      ], {
        cwd: root, timeout: 90_000, maxBuffer: 16_384,
        env: { PATH: process.env.PATH, HOME: home, USERPROFILE: home,
          WENLAN_DATA_DIR: root, WENLAN_NO_AUTOSTART: '1' },
      });
      const prepared = JSON.parse(await readFile(receiptPath, 'utf8'));
      assert.equal(prepared.status, 'prepared');
      mapping = prepared.fixture;
      assert.equal(prepared.test_cases.length, 5);
      assert.equal(prepared.negative_test_cases.length, 3);
      const testText = JSON.stringify(prepared.test_cases);
      assert(testText.includes(mapping.mapping['page_atlas-auth']));
      assert(testText.includes(mapping.mapping['page_atlas-unavailable']));
      assert(!testText.includes('page_atlas-auth') && !testText.includes('mem_atlas-auth'));
      assert(!testText.includes(mapping.mapping['mem_private-sentinel']));
      t.diagnostic(`Reviewer preparation CLI receipt: ${receiptPath}`);
    } else {
      mapping = await seedReviewerViaApi({ daemonUrl, expectedKnowledgePath: pages });
    }
    assert(mapping && typeof mapping === 'object');
    const spaces = await request('/api/spaces');
    assert(Array.isArray(spaces), 'App list_spaces consumes an array from the same daemon endpoint');
    for (const name of ['atlas-review', 'private-sentinel']) {
      const space = spaces.find(item => item.name === name);
      assert(space, `prepared project ${name} must be selectable`);
      assert.equal(space.suggested, false, `${name} must appear in confirmed Spaces`);
    }
    const brief = await request('/api/brief', { space: 'atlas-review' });
    assert.equal(brief.state, 'ready');
    assert.equal(brief.space, 'atlas-review');
    assert.match(JSON.stringify(brief), /Use signed requests/);
    assert.match(JSON.stringify(brief), /Document offline fallback/);
    assert(!JSON.stringify(brief).includes('UNAUTHORIZED_SENTINEL'));
    const sources = {};
    for (const key of ['page_atlas-auth', 'page_atlas-unavailable']) {
      const id = mapping.mapping[key];
      assert.match(id, /^[A-Za-z0-9_-]+$/);
      const result = await request(`/api/pages/${id}/sources`);
      assert(Array.isArray(result));
      assert(!JSON.stringify(result).includes('UNAUTHORIZED_SENTINEL'));
      const available = result.filter(source => source.memory != null);
      if (key === 'page_atlas-auth') {
        assert.equal(available.length, 1, 'normal API must retain the real source link');
        assert.equal(available[0].memory.source_id, mapping.mapping['mem_atlas-auth']);
      } else {
        assert.equal(available.length, 0, 'moved source must not expose memory in the shared Space');
      }
      sources[key] = { links: result.length, availableMemories: available.length };
    }
    await writeFile(join(root, 'fixture-mapping.json'), JSON.stringify(mapping, null, 2), { flag: 'wx', mode: 0o600 });
    console.log(JSON.stringify({
      evidence: 'local HTTP fixture preparation only', binarySha256: hash, root,
      mapping, sources, reviewerInstallation: 'unchecked', chatgpt: 'unchecked',
      pageApproval: 'not bypassed; normal App review may still be required',
    }));
    await assert.rejects(seedReviewerViaApi({ daemonUrl, expectedKnowledgePath: pages }), /exist|already/i);
    const mcpBin = process.env.WENLAN_TEST_MCP_BIN;
    if (!mcpBin) {
      t.diagnostic('WENLAN_TEST_MCP_BIN unset; authenticated MCP evidence skipped');
    } else {
      assert(isAbsolute(mcpBin), 'WENLAN_TEST_MCP_BIN must be absolute');
      const mcpHash = createHash('sha256').update(await readFile(mcpBin)).digest('hex');
      const mcpCache = join(root, 'mcp-cache');
      await mkdir(mcpCache, { mode: 0o700 });
      const probe = createServer();
      await new Promise((resolve, reject) => probe.once('error', reject).listen(0, '127.0.0.1', resolve));
      const mcpPort = probe.address().port;
      await new Promise(resolve => probe.close(resolve));
      assert.notEqual(mcpPort, 7878);
      assert.notEqual(mcpPort, port);
      const mcpUrl = `http://127.0.0.1:${mcpPort}/mcp`;
      const tokenVar = 'WENLAN_TEST_MCP_TOKEN';
      const token = randomBytes(32).toString('hex');
      const mcp = spawn(mcpBin, ['serve', '--tool-profile', 'query-only', '--host', '127.0.0.1',
        '--port', String(mcpPort), '--token-env', tokenVar, '--origin-url', daemonUrl], {
        cwd: root,
        env: {
          PATH: process.env.PATH, LANG: 'en_US.UTF-8',
          ...(process.env.TMPDIR ? { TMPDIR: process.env.TMPDIR } : {}),
          HOME: home, USERPROFILE: home, WENLAN_SPACE: 'atlas-review',
          WENLAN_NO_AUTOSTART: '1', WENLAN_DATA_DIR: root,
          WENLAN_TEST_FASTEMBED_CACHE: cache,
          WENLAN_MCP_CACHE_DIR: mcpCache, [tokenVar]: token,
        },
        stdio: ['ignore', 'pipe', 'pipe'],
      });
      let mcpOutput = '';
      let mcpStartupError;
      for (const pipe of [mcp.stdout, mcp.stderr]) pipe.on('data', data => {
        mcpOutput = (mcpOutput + data).slice(-32_768);
      });
      const mcpExited = new Promise(resolve => {
        mcp.once('error', error => { mcpStartupError = error; resolve({ error: error.message }); });
        mcp.once('exit', (code, signal) => resolve({ code, signal }));
      });
      const stopMcp = async () => {
        if (mcp.pid && mcp.exitCode === null && mcp.signalCode === null) mcp.kill('SIGTERM');
        let stopped = await Promise.race([mcpExited, delay(5000, null, { ref: false })]);
        if (!stopped) {
          mcp.kill('SIGKILL');
          stopped = await mcpExited;
          assert.fail('owned MCP required forced shutdown');
        }
        assert(!mcpOutput.includes(token), 'MCP logs must never contain the bearer token');
        await writeFile(join(root, 'mcp.log'), mcpOutput, { mode: 0o600 });
        await writeFile(join(root, 'mcp-receipt.json'),
          JSON.stringify({ binarySha256: mcpHash, pid: mcp.pid, stopped }), { mode: 0o600 });
        return stopped;
      };
      try {
        let mcpReady = false;
        const mcpDeadline = Date.now() + 45_000;
        while (Date.now() < mcpDeadline) {
          if (mcpStartupError) throw mcpStartupError;
          assert.equal(mcp.exitCode, null, 'MCP exited during startup');
          assert.equal(mcp.signalCode, null, 'MCP terminated during startup');
          try {
            const health = await fetch(`http://127.0.0.1:${mcpPort}/health`,
              { redirect: 'error', signal: AbortSignal.timeout(3000) });
            const healthy = health.ok;
            await health.arrayBuffer();
            if (healthy) { mcpReady = true; break; }
          } catch {}
          await delay(200);
        }
        assert(mcpReady, `MCP did not become ready; retained logs at ${root}`);
        let session;
        let rpcId = 0;
        const mcpResult = async (response, id) => {
          assert.equal(response.status, 200, await response.clone().text());
          const text = await response.text();
          const messages = response.headers.get('content-type')?.startsWith('text/event-stream')
            ? text.split('\n').filter(line => line.startsWith('data:')).map(line => line.slice(5).trim())
              .filter(Boolean).map(data => JSON.parse(data))
            : [JSON.parse(text)];
          const message = messages.find(message => message.id === id);
          assert(message, 'matching JSON-RPC response');
          assert.equal(message.error, undefined);
          if (response.headers.has('mcp-session-id')) session = response.headers.get('mcp-session-id');
          return message.result;
        };
        const projected = value => {
          assert.equal(value.isError, false);
          assert.deepEqual(JSON.parse(value.content[0].text), value.structuredContent);
          for (const hidden of ['UNAUTHORIZED_SENTINEL', mapping.mapping['mem_private-sentinel'],
            mapping.mapping['mem_atlas-unavailable']]) {
            assert(hidden && !JSON.stringify(value).includes(hidden), 'no private or unavailable memory leakage');
          }
          return value.structuredContent;
        };
        const mcpCall = (name, args) => {
          const callId = ++rpcId;
          return fetch(mcpUrl, {
            method: 'POST', redirect: 'error', signal: AbortSignal.timeout(10_000),
            headers: { 'content-type': 'application/json', accept: 'application/json, text/event-stream',
              'mcp-protocol-version': '2025-06-18', authorization: `Bearer ${token}`,
              ...(session ? { 'mcp-session-id': session } : {}) },
            body: JSON.stringify({ jsonrpc: '2.0', id: callId, method: 'tools/call',
              params: { name, arguments: args } }),
          }).then(async response => mcpResult(response, callId));
        };
        const initId = ++rpcId;
        const initResponse = await fetch(mcpUrl, {
          method: 'POST', redirect: 'error', signal: AbortSignal.timeout(10_000),
          headers: { 'content-type': 'application/json', accept: 'application/json, text/event-stream',
            'mcp-protocol-version': '2025-06-18', authorization: `Bearer ${token}` },
          body: JSON.stringify({ jsonrpc: '2.0', id: initId, method: 'initialize',
            params: { protocolVersion: '2025-06-18', capabilities: {},
              clientInfo: { name: 'native-fixture-mcp-check', version: '1' } } }),
        });
        const initInfo = await mcpResult(initResponse, initId);
        assert(initInfo.serverInfo.name);
        assert(session, 'MCP session tracked');
        const notified = await fetch(mcpUrl, {
          method: 'POST', redirect: 'error', signal: AbortSignal.timeout(10_000),
          headers: { 'content-type': 'application/json', accept: 'application/json, text/event-stream',
            'mcp-protocol-version': '2025-06-18', authorization: `Bearer ${token}`,
            ...(session ? { 'mcp-session-id': session } : {}) },
          body: JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }),
        });
        assert([200, 202].includes(notified.status), `initialized: ${notified.status}`);
        await notified.arrayBuffer();
        const listId = ++rpcId;
        const listed = await mcpResult(await fetch(mcpUrl, {
          method: 'POST', redirect: 'error', signal: AbortSignal.timeout(10_000),
          headers: { 'content-type': 'application/json', accept: 'application/json, text/event-stream',
            'mcp-protocol-version': '2025-06-18', authorization: `Bearer ${token}`,
            ...(session ? { 'mcp-session-id': session } : {}) },
          body: JSON.stringify({ jsonrpc: '2.0', id: listId, method: 'tools/list' }),
        }), listId);
        assert.deepEqual(listed.tools.map(tool => tool.name).sort(),
          ['brief', 'get_page_sources', 'recall']);
        const memAuth = mapping.mapping['mem_atlas-auth'];
        const pageAuth = mapping.mapping['page_atlas-auth'];
        const pageUnavailable = mapping.mapping['page_atlas-unavailable'];
        assert.match(memAuth, /^[A-Za-z0-9_-]+$/);
        const brief = projected(await mcpCall('brief', {}));
        assert.equal(brief.space, 'atlas-review');
        assert.match(JSON.stringify(brief), /Use signed requests/);
        assert.match(JSON.stringify(brief), /Document offline fallback/);
        const topic = projected(await mcpCall('brief', { topic: 'Atlas authentication decision' }));
        assert.deepEqual(topic.brief, brief.brief);
        assert(topic.related_context.results.some(hit => hit.source_id === memAuth));
        const recalled = projected(await mcpCall('recall',
          { query: 'Atlas authentication decision', limit: 3, rerank: false }));
        assert(recalled.results.length <= 3 && recalled.results.some(hit => hit.source_id === memAuth));
        const authSources = projected(await mcpCall('get_page_sources', { page_id: pageAuth }));
        assert.equal(authSources.sources.length, 1);
        assert(JSON.stringify(authSources).includes(memAuth));
        assert(!JSON.stringify(authSources).includes('UNAUTHORIZED_SENTINEL'));
        const unavailable = projected(await mcpCall('get_page_sources', { page_id: pageUnavailable }));
        assert.deepEqual(unavailable, { page_id: pageUnavailable, sources: [] });
        const hiddenMem = mapping.mapping['mem_atlas-unavailable'];
        if (hiddenMem) assert(!JSON.stringify(unavailable).includes(hiddenMem),
          'unavailable page must not leak its hidden memory ID');
        const mcpStopped = await stopMcp();
        await assertListenerClosed(mcpPort);
        console.log(JSON.stringify({
          evidence: 'authenticated loopback MCP over fresh isolated daemon only',
          binarySha256: hash, mcpBinarySha256: mcpHash, root, mcpPort,
          tools: ['brief', 'get_page_sources', 'recall'],
          mcpStopped, reviewerInstallation: 'unchecked', chatgpt: 'unchecked', oauth: 'unchecked',
          publicRelay: 'unchecked', ui: 'unchecked', model: 'unchecked',
        }));
      } finally {
        await stopMcp();
        await assertListenerClosed(mcpPort);
      }
    }
  } finally {
    if (child.pid && child.exitCode === null && child.signalCode === null) child.kill('SIGTERM');
    let stopped = await Promise.race([exited, delay(5000, null, { ref: false })]);
    if (!stopped) {
      child.kill('SIGKILL');
      stopped = await exited;
      await writeFile(join(root, 'daemon.log'), output, { mode: 0o600 });
      assert.fail('owned daemon required forced shutdown');
    }
    await writeFile(join(root, 'daemon.log'), output, { mode: 0o600 });
    await writeFile(join(root, 'process-receipt.json'), JSON.stringify({ binarySha256: hash, pid: child.pid, stopped }), { mode: 0o600 });
    await assertListenerClosed(port);
  }
});
