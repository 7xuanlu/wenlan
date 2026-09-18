// SPDX-License-Identifier: Apache-2.0
// Explicit public test lane. Never uses an installed service or personal library.
import assert from 'node:assert/strict';
import test from 'node:test';
import { execFile, spawn } from 'node:child_process';
import { mkdir, mkdtemp, realpath, rm, stat, writeFile } from 'node:fs/promises';
import { createServer, createConnection } from 'node:net';
import { tmpdir } from 'node:os';
import { isAbsolute, join } from 'node:path';
import { randomBytes, createHash } from 'node:crypto';
import { promisify } from 'node:util';
import { setTimeout as delay } from 'node:timers/promises';
import { CodexRpcError, startCodexClient } from './fixtures/codex-client.mjs';
import { approvePortalPairing } from './fixtures/portal-pairing.mjs';
import { prepareModelHome } from './fixtures/codex-model-home.mjs';
import { runCodexNegativeCases } from './fixtures/codex-negative-cases.mjs';

const origin = 'https://relay.wenlan.app';
const stagingOrigin = 'https://wenlan-relay-staging.wenlan-app.workers.dev';
const space = 'atlas-review';
const reverseReadyMarker = 'WENLAN_REVERSE_HELPER_READY';
const exec = promisify(execFile);

async function freePort() {
  const server = createServer();
  await new Promise((resolve, reject) => server.once('error', reject).listen(0, '127.0.0.1', resolve));
  const port = server.address().port;
  await new Promise(resolve => server.close(resolve));
  return port;
}

function start(name, binary, args, env) {
  const child = spawn(binary, args, { env, stdio: ['ignore', 'pipe', 'pipe'] });
  const task = { name, child, tunnel: undefined, connected: false, reverseReady: false, quicTimeout: false };
  let tail = '';
  for (const pipe of [child.stdout, child.stderr]) pipe.on('data', data => {
    tail = (tail + data).slice(-8192);
    task.tunnel ??= /https:\/\/[a-z0-9-]+\.trycloudflare\.com\b/.exec(tail)?.[0];
    task.connected ||= tail.includes('Registered tunnel connection');
    task.reverseReady ||= tail.includes(reverseReadyMarker);
    task.quicTimeout ||= tail.includes('Failed to dial a quic connection') || tail.includes('no recent network activity');
  });
  task.exited = new Promise(resolve => {
    child.once('error', () => { task.failed = true; resolve(); });
    child.once('exit', () => resolve());
  });
  return task;
}

async function stop(task) {
  if (!task) return;
  if (task.child.exitCode === null && task.child.signalCode === null) task.child.kill('SIGTERM');
  // cloudflared's documented default drains in-flight requests for up to 30s.
  const graceMs = task.name === 'tunnel' ? 35_000 : 5000;
  const exited = await Promise.race([task.exited.then(() => true), delay(graceMs, false, { ref: false })]);
  if (!exited) {
    task.child.kill('SIGKILL');
    const forced = await Promise.race([task.exited.then(() => true), delay(3000, false, { ref: false })]);
    assert(forced, `${task.name} exit unconfirmed`);
    throw new Error(`${task.name} required forced termination`);
  }
  assert(task.child.exitCode !== null || task.child.signalCode !== null, `${task.name} exit unconfirmed`);
}

async function readyMarker(task) {
  const end = Date.now() + 30_000;
  while (Date.now() < end) {
    assert(!task.failed && task.child.exitCode === null && task.child.signalCode === null,
      `${task.name} exited before readiness marker`);
    if (task.reverseReady) return;
    await delay(100);
  }
  throw new Error(`${task.name} readiness marker deadline`);
}

async function closed(port) {
  if (!port) return;
  const connected = await new Promise(resolve => {
    const socket = createConnection({ host: '127.0.0.1', port });
    socket.once('connect', () => { socket.destroy(); resolve(true); });
    socket.once('error', error => resolve(error.code !== 'ECONNREFUSED'));
    socket.setTimeout(1000, () => { socket.destroy(); resolve(true); });
  });
  assert.equal(connected, false, 'owned listener closure must be confirmed');
}

async function request(url, init = {}) {
  const response = await fetch(url, { ...init, redirect: 'manual', signal: AbortSignal.timeout(15_000) });
  const reader = response.body?.getReader();
  const chunks = [];
  let length = 0;
  try {
    while (reader) {
      const next = await reader.read();
      if (next.done) break;
      length += next.value.length;
      assert(length <= 256 * 1024, 'response exceeds test boundary');
      chunks.push(Buffer.from(next.value));
    }
  } finally { await reader?.cancel().catch(() => {}); }
  return { status: response.status, headers: response.headers, text: Buffer.concat(chunks).toString('utf8') };
}

async function readSseHeartbeat(reader, timeoutMs = 4_000) {
  let timer;
  let text = '';
  try {
    return await Promise.race([
      (async () => {
        while (true) {
          const next = await reader.read();
          if (next.done) throw new Error('SSE stream ended before SDK heartbeat');
          text += new TextDecoder().decode(next.value);
          assert(text.length <= 4096, 'SSE heartbeat probe exceeded test boundary');
          if (text.split(/\r?\n/).some(line => line === ':')) return text;
        }
      })(),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('SDK heartbeat deadline')), timeoutMs); }),
    ]);
  } finally { clearTimeout(timer); }
}

function openSse(url, headers, signal) {
  return fetch(url, { method: 'GET', headers, redirect: 'manual',
    signal: AbortSignal.any([signal, AbortSignal.timeout(15_000)]) });
}

const json = response => JSON.parse(response.text);
const post = (path, value, headers = {}) => request(`${origin}${path}`, { method: 'POST',
  headers: { 'content-type': 'application/json', ...headers }, body: JSON.stringify(value) });

async function ready(task, url, headers = {}, expected = 200) {
  const end = Date.now() + 30_000;
  let observed = 'no response';
  while (Date.now() < end) {
    assert(!task.failed && task.child.exitCode === null && task.child.signalCode === null, `${task.name} exited before readiness`);
    try {
      const status = (await request(url, { headers })).status;
      observed = `HTTP ${status}`;
      if (status === expected) return;
    } catch (error) {
      const code = error.cause?.code ?? error.name;
      observed = typeof code === 'string' && /^[A-Za-z0-9_]+$/.test(code) ? code : 'request failed';
    }
    await delay(250);
  }
  throw new Error(`${task.name} readiness deadline: ${observed}; connected=${task.connected}; quicTimeout=${task.quicTimeout}`);
}

function result(response, id) {
  assert.equal(response.status, 200, 'MCP request must succeed');
  const messages = response.headers.get('content-type')?.startsWith('text/event-stream')
    ? response.text.split('\n').filter(line => line.startsWith('data:')).map(line => line.slice(5).trim()).filter(Boolean).map(JSON.parse)
    : [json(response)];
  const message = messages.find(item => item.id === id);
  assert(message && !message.error, 'matching successful JSON-RPC response required');
  return message.result;
}

function projected(value) {
  assert.equal(value.isError, false);
  assert.deepEqual(JSON.parse(value.content[0].text), value.structuredContent);
  assert(!JSON.stringify(value).includes('UNAUTHORIZED_SENTINEL'));
  return value.structuredContent;
}

const annotationKeys = ['readOnlyHint', 'destructiveHint', 'idempotentHint', 'openWorldHint'];
const authPolicy = schemes => schemes === undefined ? 'server default' : Array.isArray(schemes)
  ? schemes.map(scheme => ({
    type: typeof scheme?.type === 'string' ? scheme.type : null,
    ...(Array.isArray(scheme?.scopes) ? { scopes: scheme.scopes.filter(scope => typeof scope === 'string') } : {}),
  }))
  : { present: true, kind: typeof schemes };

test('deployed relay reaches isolated native knowledge through the selected public transport', { timeout: 300_000 }, async t => {
  assert.equal(process.env.WENLAN_TEST_PUBLIC_RELAY, '1', 'explicit public test approval gate required');
  const checkEnvironmentIsolation = process.env.WENLAN_TEST_ENVIRONMENT_ISOLATION === '1';
  assert.notEqual(process.platform, 'win32', 'POSIX lifecycle driver is not Windows evidence');
  const transport = process.env.WENLAN_PUBLIC_TRANSPORT ?? 'tunnel';
  assert(['tunnel', 'reverse'].includes(transport), 'WENLAN_PUBLIC_TRANSPORT must be tunnel or reverse');
  const bin = process.env.WENLAN_NATIVE_BIN_DIR;
  const cache = process.env.WENLAN_TEST_FASTEMBED_CACHE;
  const cloudflared = process.env.WENLAN_CLOUDFLARED_BIN;
  const reverseHelperBin = process.env.WENLAN_REVERSE_HELPER_BIN;
  const modelHome = process.env.WENLAN_CODEX_MODEL_HOME;
  const modelCases = process.env.WENLAN_CODEX_MODEL_CASES ?? 'positive';
  assert(['positive', 'negative'].includes(modelCases), 'model cases must be positive or negative');
  if (modelCases === 'negative') assert(modelHome, 'negative model cases require the isolated model lane');
  if (modelHome) {
    assert.equal(process.env.WENLAN_TEST_MODEL_EXECUTION, '1', 'explicit model test opt-in required');
    assert(process.env.WENLAN_CODEX_BIN, 'native Codex binary required');
    assert.equal(transport, 'reverse');
    assert.equal(process.env.WENLAN_PORTAL_PAIRING_ID, undefined, 'model and attended portal windows are separate runs');
  }
  for (const path of [bin, cache]) assert(path && isAbsolute(path), 'explicit absolute runtime/cache paths required');
  if (transport === 'tunnel') {
    assert(cloudflared && isAbsolute(cloudflared), 'explicit cloudflared path required for tunnel transport');
  } else {
    assert(reverseHelperBin && isAbsolute(reverseHelperBin), 'explicit reverse helper path required');
    assert((await stat(reverseHelperBin)).isFile(), 'reverse helper must be a regular file');
  }
  assert((await stat(cache)).isDirectory());
  const scratch = await realpath(await mkdtemp(join(tmpdir(), 'wenlan-public-check-')));
  const root = join(scratch, 'library');
  const backendToken = randomBytes(32).toString('base64url');
  const env = { PATH: process.env.PATH, LANG: 'en_US.UTF-8',
    ...(process.env.TMPDIR ? { TMPDIR: process.env.TMPDIR } : {}),
    HOME: join(root, 'home'), USERPROFILE: join(root, 'home'), WENLAN_DATA_DIR: root,
    WENLAN_NO_AUTOSTART: '1', WENLAN_SPACE: space, WENLAN_RERANKER_MODE: 'off',
    WENLAN_TEST_FASTEMBED_CACHE: cache, WENLAN_MCP_CACHE_DIR: join(root, 'mcp-cache') };
  let daemon, mcp, tunnel, reverseHelper, oauthClient, nativeClient, nativeCall, daemonPort, mcpPort, device,
    recoveryPath, cleanupModelConfig, revoked = false;
  async function nativeDenied(name, args) {
    let response;
    try { response = await nativeCall(name, args); }
    catch (error) { assert(error instanceof CodexRpcError, 'explicit native RPC failure required'); return; }
    assert.equal(response.isError, true);
    assert(!JSON.stringify(response).includes('UNAUTHORIZED_SENTINEL'));
  }
  const deviceHeaders = () => ({ authorization: `Bearer ${device.managementToken}`, 'x-wenlan-device-id': device.id });
  try {
    const codexConfig = `mcp_oauth_credentials_store = "file"\n[mcp_servers.wenlan_public_synthetic]\nurl = "${origin}/mcp"\n`;
    if (modelHome) {
      cleanupModelConfig = await prepareModelHome(modelHome,
        `mcp_oauth_credentials_store = "file"\nmodel_reasoning_effort = "low"\nweb_search = "disabled"\n` +
        `[features]\nview_image = false\nshell_tool = false\nunified_exec = false\n` +
        `js_repl = false\napps = false\nplugins = false\nbrowser_use = false\ncomputer_use = false\n` +
        `[mcp_servers.wenlan_public_synthetic]\nurl = "${origin}/mcp"\n`);
      const preflight = startCodexClient(process.env.WENLAN_CODEX_BIN,
        { ...env, CODEX_HOME: modelHome, BROWSER: '/usr/bin/false' }, scratch);
      try {
        await preflight.rpc('initialize', { clientInfo: { name: 'wenlan-model-preflight', version: '1' } });
        preflight.notify('initialized');
        const account = await preflight.rpc('account/read', { refreshToken: false });
        assert(account.account, 'isolated model login required before public enrollment');
      } finally { await preflight.close(); }
    }
    await exec(join(bin, 'examples/seed_reviewer_library'), [root], { env, timeout: 45_000, maxBuffer: 16_384 });
    daemonPort = await freePort();
    mcpPort = await freePort();
    daemon = start('daemon', join(bin, 'wenlan-server'), [], { ...env, WENLAN_PORT: String(daemonPort), WENLAN_BIND_ADDR: `127.0.0.1:${daemonPort}` });
    const daemonUrl = `http://127.0.0.1:${daemonPort}`;
    await ready(daemon, `${daemonUrl}/api/health`);
    assert.deepEqual(json(await request(`${daemonUrl}/api/knowledge/path`)), { path: join(root, 'pages') });
    mcp = start('MCP', join(bin, 'wenlan-mcp'), ['--origin-url', daemonUrl, '--agent-name', 'public-synthetic-check',
      'serve', '--tool-profile', 'query-only', '--host', '127.0.0.1', '--port', String(mcpPort), '--token-env', 'WENLAN_TEST_TOKEN'],
    { ...env, WENLAN_TEST_TOKEN: backendToken });
    const local = `http://127.0.0.1:${mcpPort}`;
    await ready(mcp, `${local}/health`);
    assert.equal((await request(`${local}/connector-info`)).status, 401);
    assert.equal((await request(`${local}/mcp`)).status, 401);
    let enrollment;
    if (transport === 'tunnel') {
      tunnel = start('tunnel', cloudflared, ['tunnel', '--no-autoupdate', '--url', local], env);
      const tunnelDeadline = Date.now() + 30_000;
      while ((!tunnel.tunnel || !tunnel.connected) && Date.now() < tunnelDeadline) {
        assert(!tunnel.failed && tunnel.child.exitCode === null, 'tunnel exited before URL');
        await delay(100);
      }
      assert(tunnel.tunnel && tunnel.connected, 'tunnel URL and connection deadline');
      // Public DNS was observed appearing after connection. Enrollment exercises
      // the actual Worker -> tunnel path and verifies both anonymous 401 and the
      // protected contract before storage. Never retry an uncertain mutation.
      await delay(15_000);
      const contract = await request(`${local}/connector-info`, { headers: { authorization: `Bearer ${backendToken}` } });
      assert.equal(contract.status, 200);
      assert.equal(json(contract).space, space);
      enrollment = await post('/devices', { tunnelOrigin: tunnel.tunnel, backendToken, space });
    } else {
      // Reverse enrollment only prepares a pending device; the native helper
      // activates it after probing the real local MCP listener.
      const contract = await request(`${local}/connector-info`, { headers: { authorization: `Bearer ${backendToken}` } });
      assert.equal(contract.status, 200);
      assert.equal(json(contract).space, space);
      enrollment = await post('/devices/reverse', { backendToken, space });
    }
    const retryAfter = enrollment.headers.get('retry-after');
    assert.equal(enrollment.status, 201, enrollment.status === 429 && /^\d+$/.test(retryAfter ?? '')
      ? `public enrollment rate limited; retry-after=${retryAfter}s` : 'public enrollment');
    device = json(enrollment);
    recoveryPath = join(scratch, 'device-recovery.json');
    await writeFile(recoveryPath, JSON.stringify({ origin, device, backendToken }), { mode: 0o600, flag: 'wx' });
    assert.equal((await stat(recoveryPath)).mode & 0o777, 0o600, 'recovery credentials must be mode 0600');
    if (transport === 'reverse') {
      reverseHelper = start('reverse-helper', reverseHelperBin, [
        '--ignored', '--exact', 'remote_relay::tests::public_reverse_socket_helper', '--nocapture',
      ], {
        ...env,
        WENLAN_TEST_PUBLIC_RELAY: '1',
        WENLAN_REVERSE_HELPER_CREDENTIALS_PATH: recoveryPath,
        WENLAN_REVERSE_HELPER_MCP_PORT: String(mcpPort),
      });
      await readyMarker(reverseHelper);
    }
    await approvePortalPairing({ transport, origin, space, deviceHeaders, request, post,
      diagnostic: message => {
        t.diagnostic(message);
        process.stderr.write('WENLAN_PORTAL_READY\n');
      } });
    if (process.env.WENLAN_CODEX_BIN) {
      const codexBin = process.env.WENLAN_CODEX_BIN;
      assert(isAbsolute(codexBin));
      const codexHome = modelHome ?? join(scratch, 'codex');
      if (!modelHome) {
        await mkdir(codexHome, { mode: 0o700 });
        await writeFile(join(codexHome, 'config.toml'), codexConfig,
          { mode: 0o600, flag: 'wx' });
      }
      const codexEnv = { ...env, CODEX_HOME: codexHome, BROWSER: '/usr/bin/false' };
      // The app-server returns the URL without the CLI's automatic browser launch.
      oauthClient = startCodexClient(codexBin, codexEnv, codexHome);
      await oauthClient.rpc('initialize', { clientInfo: { name: 'wenlan-oauth-check', version: '1' } });
      oauthClient.notify('initialized');
      const login = await oauthClient.rpc('mcpServer/oauth/login', {
        name: 'wenlan_public_synthetic', scopes: ['wenlan:query'], timeoutSecs: 60,
      });
      const authUrl = new URL(login.authorizationUrl);
      assert.equal(authUrl.origin, origin);
      assert.equal(authUrl.pathname, '/authorize');
      assert.equal(authUrl.searchParams.get('code_challenge_method'), 'S256');
      assert.equal(authUrl.searchParams.get('resource'), `${origin}/mcp`);
      const callbackUrl = new URL(authUrl.searchParams.get('redirect_uri'));
      assert.equal(callbackUrl.protocol, 'http:');
      assert.equal(callbackUrl.hostname, '127.0.0.1');
      assert.equal(callbackUrl.pathname, '/callback');
      assert(callbackUrl.port && !callbackUrl.username && !callbackUrl.password && !callbackUrl.search && !callbackUrl.hash);
      const browser = await request(authUrl);
      assert.equal(browser.status, 303, 'native Codex authorization');
      const browserCookie = browser.headers.get('set-cookie').split(';')[0];
      const nativePairId = browserCookie.split('=')[1].split('.')[0];
      const nativeIntent = await request(`${origin}/pairings/${nativePairId}`, { headers: deviceHeaders() });
      assert.equal(nativeIntent.status, 200);
      assert.equal(json(nativeIntent).clientId, authUrl.searchParams.get('client_id'));
      assert.equal((await post(`/pairings/${nativePairId}/approve`, { approved: true,
        clientId: authUrl.searchParams.get('client_id'), resource: `${origin}/mcp`, space }, deviceHeaders())).status, 200);
      const nativeCompleted = await post('/pairing/complete', {}, { cookie: browserCookie, origin, 'sec-fetch-site': 'same-origin' });
      assert.equal(nativeCompleted.status, 200);
      const nativeRedirect = new URL(json(nativeCompleted).redirectTo);
      assert.equal(nativeRedirect.origin, callbackUrl.origin);
      assert.equal(nativeRedirect.pathname, callbackUrl.pathname);
      assert.equal(nativeRedirect.searchParams.get('state'), authUrl.searchParams.get('state'));
      assert.equal(nativeRedirect.searchParams.get('iss'), origin);
      assert.equal((await request(nativeRedirect)).status, 200, 'native Codex callback');
      // Callback HTTP success alone does not prove a completed token exchange.
      await oauthClient.waitForOAuth('wenlan_public_synthetic');
      await oauthClient.close();
      oauthClient = undefined;
      const listing = await exec(codexBin, ['mcp', 'list', '--json'], { env: codexEnv, timeout: 15_000, maxBuffer: 16_384 });
      const servers = JSON.parse(listing.stdout);
      assert.equal(servers.length, 1);
      assert.equal(servers[0].name, 'wenlan_public_synthetic');
      assert.equal(servers[0].auth_status, 'o_auth');
      assert.equal(servers[0].transport.url, `${origin}/mcp`);
      t.diagnostic('Actual isolated Codex OAuth login and fresh-process credential discovery passed');
      nativeClient = startCodexClient(codexBin, codexEnv, codexHome);
      await nativeClient.rpc('initialize', { clientInfo: { name: 'wenlan-native-check', version: '1' } });
      nativeClient.notify('initialized');
      const modelCwd = join(scratch, 'model-work');
      await mkdir(modelCwd, { mode: 0o700 });
      if (modelHome) {
        const account = await nativeClient.rpc('account/read', { refreshToken: false });
        assert(account.account, 'isolated model login required');
      }
      const nativeThread = await nativeClient.rpc('thread/start', { cwd: modelCwd, ephemeral: true,
        model: 'gpt-5.6-luna', allowProviderModelFallback: false,
        sandbox: 'read-only', approvalPolicy: 'on-request' });
      assert(nativeThread.thread.id);
      if (modelHome) {
        assert.equal(nativeThread.model, 'gpt-5.6-luna');
        assert.equal(nativeThread.reasoningEffort, 'low');
      }
      const threadId = nativeThread.thread.id;
      const inventory = await nativeClient.rpc('mcpServerStatus/list', { threadId, detail: 'toolsAndAuthOnly' });
      assert.equal(inventory.data.length, 1);
      assert.equal(inventory.data[0].name, 'wenlan_public_synthetic');
      assert.deepEqual(Object.values(inventory.data[0].tools).map(tool => tool.name).sort(),
        ['brief', 'get_page_sources', 'recall']);
      nativeCall = (tool, args) => nativeClient.rpc('mcpServer/tool/call', {
        threadId, server: 'wenlan_public_synthetic', tool, arguments: args,
      });
      const nativeBrief = projected(await nativeCall('brief', {}));
      assert.equal(nativeBrief.space, space);
      assert.equal(nativeBrief.brief.active[0].text, 'Use signed requests');
      const nativeTopic = projected(await nativeCall('brief', { topic: 'Atlas authentication decision' }));
      assert(nativeTopic.related_context.results.some(hit => hit.source_id === 'mem_atlas-auth'));
      const nativeRecall = projected(await nativeCall('recall', { query: 'Atlas authentication decision', limit: 3, rerank: false }));
      assert(nativeRecall.results.length <= 3 && nativeRecall.results.some(hit => hit.source_id === 'mem_atlas-auth'));
      const nativeSources = projected(await nativeCall('get_page_sources', { page_id: 'page_atlas-auth' }));
      assert.equal(nativeSources.sources.length, 1);
      assert(JSON.stringify(nativeSources).includes('mem_atlas-auth'));
      assert.deepEqual(projected(await nativeCall('get_page_sources', { page_id: 'page_atlas-unavailable' })),
        { page_id: 'page_atlas-unavailable', sources: [] });
      for (const [name, args] of [['get_page_sources', { page_id: 'page_private-sentinel' }],
        ['brief', { space: 'private-sentinel' }], ['capture', { content: 'must not be stored' }]]) {
        await nativeDenied(name, args);
        assert.deepEqual(projected(await nativeCall('brief', {})), nativeBrief);
      }
      t.diagnostic('Actual Codex tool discovery, five positive fixtures and three negative cases passed');
      if (modelHome && modelCases === 'positive') {
        const started = await nativeClient.rpc('turn/start', { threadId, input: [{
          type: 'text', text_elements: [],
          text: 'Use only the connected Wenlan synthetic library for five fresh checks. ' +
            'First read the project Brief without a topic. Then read its Brief with related context ' +
            'about the Atlas authentication decision. Search for that decision with at most three results ' +
            'and reranking disabled. Show supporting sources for page_atlas-auth, then page_atlas-unavailable. ' +
            'Label all five results and cite only source IDs actually returned. If sources are empty, ' +
            'report no available evidence without guessing why. Do not use the filesystem, shell, web, ' +
            'other plugins, or invent content. These are read/query-only synthetic checks.',
        }] });
        const proof = await nativeClient.waitForTurn(threadId, started.turn.id, 90_000);
        assert.equal(proof.turn.status, 'completed');
        assert.equal(proof.turn.error, null);
        const calls = proof.items.filter(item => item.type === 'mcpToolCall');
        assert.equal(calls.length, 5, 'five actual model-selected calls required');
        assert(calls.every(item => item.server === 'wenlan_public_synthetic' &&
          item.status === 'completed' && !item.error && item.result));
        const plain = calls.find(item => item.tool === 'brief' && !item.arguments.topic);
        const topicCall = calls.find(item => item.tool === 'brief' && item.arguments.topic);
        const recallCall = calls.find(item => item.tool === 'recall');
        const pageCall = calls.find(item => item.tool === 'get_page_sources' && item.arguments.page_id === 'page_atlas-auth');
        const emptyCall = calls.find(item => item.tool === 'get_page_sources' && item.arguments.page_id === 'page_atlas-unavailable');
        assert(plain && topicCall && recallCall && pageCall && emptyCall, 'all five scenarios must be exercised');
        assert.equal(recallCall.arguments.limit, 3);
        assert.equal(recallCall.arguments.rerank, false);
        const output = call => {
          const value = call.result.structuredContent;
          assert.deepEqual(JSON.parse(call.result.content[0].text), value);
          assert(!JSON.stringify(call.result).includes('UNAUTHORIZED_SENTINEL'));
          return value;
        };
        assert.equal(output(plain).space, space);
        assert(output(topicCall).related_context.results.some(hit => hit.source_id === 'mem_atlas-auth'));
        assert(output(recallCall).results.some(hit => hit.source_id === 'mem_atlas-auth'));
        assert.equal(output(pageCall).sources.length, 1);
        assert.deepEqual(output(emptyCall), { page_id: 'page_atlas-unavailable', sources: [] });
        const final = proof.items.filter(item => item.type === 'agentMessage').map(item => item.text).join('\n');
        assert(final.includes('mem_atlas-auth') && /signed requests/i.test(final));
        assert(!final.includes('UNAUTHORIZED_SENTINEL') && !final.includes('mem_atlas-unavailable'));
        assert(!proof.items.some(item => ['commandExecution', 'fileChange', 'webSearch'].includes(item.type)));
        t.diagnostic(`Codex model-mediated five-positive proof: ${JSON.stringify({
          model: nativeThread.model, turnId: started.turn.id,
          tools: calls.map(item => ({ name: item.tool, arguments: item.arguments })), final,
        })}`);
      }
      if (modelHome && modelCases === 'negative') {
        const negativeThread = await nativeClient.rpc('thread/start', {
          cwd: modelCwd, ephemeral: true, model: 'gpt-5.6-luna',
          allowProviderModelFallback: false, sandbox: 'read-only', approvalPolicy: 'on-request',
        });
        assert.equal(negativeThread.model, 'gpt-5.6-luna');
        assert.equal(negativeThread.reasoningEffort, 'low');
        const negativeInventory = await nativeClient.rpc('mcpServerStatus/list', {
          threadId: negativeThread.thread.id, detail: 'toolsAndAuthOnly',
        });
        assert.equal(negativeInventory.data.length, 1);
        assert.deepEqual(Object.values(negativeInventory.data[0].tools).map(tool => tool.name).sort(),
          ['brief', 'get_page_sources', 'recall']);
        try {
          const proof = await runCodexNegativeCases(nativeClient, negativeThread.thread.id);
          t.diagnostic(`Codex model-mediated three-negative proof: ${JSON.stringify({
            model: negativeThread.model, threadId: negativeThread.thread.id,
            cases: proof.cases.map(({ id, turnId, finalAnswer }) => ({ id, turnId, finalAnswer })),
          })}`);
        } catch (error) {
          if (error.negativeCaseEvidence) {
            t.diagnostic(`Codex negative failure evidence: ${JSON.stringify(error.negativeCaseEvidence)}`);
          }
          throw error;
        }
      }
    }
    const registration = await post('/oauth/register', { client_name: 'Wenlan real synthetic integration',
      redirect_uris: ['https://client.example/callback'], grant_types: ['authorization_code', 'refresh_token'],
      response_types: ['code'], token_endpoint_auth_method: 'none' });
    assert.equal(registration.status, 201, 'public DCR');
    const client = json(registration);
    const verifier = randomBytes(32).toString('base64url');
    const query = new URLSearchParams({ response_type: 'code', client_id: client.client_id,
      redirect_uri: 'https://client.example/callback', resource: `${origin}/mcp`, scope: 'wenlan:query',
      code_challenge_method: 'S256', code_challenge: createHash('sha256').update(verifier).digest('base64url') });
    const authorize = await request(`${origin}/authorize?${query}`);
    assert.equal(authorize.status, 303, 'public authorization');
    const cookie = authorize.headers.get('set-cookie').split(';')[0];
    const pairId = cookie.split('=')[1].split('.')[0];
    const intent = await request(`${origin}/pairings/${pairId}`, { headers: deviceHeaders() });
    assert.equal(intent.status, 200);
    assert.equal(json(intent).clientId, client.client_id);
    assert.equal((await post(`/pairings/${pairId}/approve`, { approved: true, clientId: client.client_id,
      resource: `${origin}/mcp`, space }, deviceHeaders())).status, 200);
    const completed = await post('/pairing/complete', {}, { cookie, origin, 'sec-fetch-site': 'same-origin' });
    assert.equal(completed.status, 200);
    const callback = new URL(json(completed).redirectTo);
    assert.equal(callback.origin, 'https://client.example');
    assert.equal(callback.searchParams.get('iss'), origin);
    const tokenRequest = fields => request(`${origin}/oauth/token`, { method: 'POST',
      headers: { 'content-type': 'application/x-www-form-urlencoded' }, body: new URLSearchParams({
        ...fields, client_id: client.client_id, resource: `${origin}/mcp`,
      }).toString() });
    const exchanged = await tokenRequest({ grant_type: 'authorization_code', code: callback.searchParams.get('code'),
      code_verifier: verifier, redirect_uri: 'https://client.example/callback' });
    assert.equal(exchanged.status, 200);
    let tokens = json(exchanged), session, id = 0;
    if (checkEnvironmentIsolation) {
      const discovery = await request(`${stagingOrigin}/.well-known/oauth-authorization-server`);
      assert.equal(discovery.status, 200);
      assert.equal(json(discovery).issuer, stagingOrigin);
      const resource = await request(`${stagingOrigin}/.well-known/oauth-protected-resource/mcp`);
      assert.equal(resource.status, 200);
      assert.equal(json(resource).resource, `${stagingOrigin}/mcp`);
      const denied = await request(`${stagingOrigin}/mcp`, {
        method: 'POST',
        headers: { authorization: `Bearer ${tokens.access_token}`, 'content-type': 'application/json',
          accept: 'application/json, text/event-stream' },
        body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'initialize', params: {
          protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'isolation-check', version: '1' },
        } }),
      });
      assert.equal(denied.status, 401, 'staging must reject an issued production OAuth access token');
      const deviceDenied = await request(`${stagingOrigin}/devices/reverse/status`, { headers: deviceHeaders() });
      assert.equal(deviceDenied.status, 401, 'staging must reject a production device credential');
      const refreshDenied = await request(`${stagingOrigin}/oauth/token`, {
        method: 'POST', headers: { 'content-type': 'application/x-www-form-urlencoded' },
        body: new URLSearchParams({ grant_type: 'refresh_token', refresh_token: tokens.refresh_token,
          client_id: client.client_id, resource: `${stagingOrigin}/mcp` }).toString(),
      });
      assert([400, 401].includes(refreshDenied.status), 'staging must reject a production refresh grant');
      assert(['invalid_client', 'invalid_grant'].includes(json(refreshDenied).error));
      t.diagnostic('Staging has its own OAuth issuer/resource and rejects production access, refresh and device credentials');
    }
    const headers = () => ({ authorization: `Bearer ${tokens.access_token}`, accept: 'application/json, text/event-stream',
      'mcp-protocol-version': '2025-06-18', ...(session ? { 'mcp-session-id': session } : {}) });
    const initialize = async () => {
      const response = await post('/mcp', { jsonrpc: '2.0', id: ++id, method: 'initialize', params: {
        protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'public-native-check', version: '1' },
      } }, headers());
      assert(result(response, id).serverInfo.name);
      session = response.headers.get('mcp-session-id');
      assert(session);
      assert.equal((await post('/mcp', { jsonrpc: '2.0', method: 'notifications/initialized' }, headers())).status, 202);
    };
    const call = (name, args) => post('/mcp', { jsonrpc: '2.0', id: ++id, method: 'tools/call', params: { name, arguments: args } }, headers());
    await initialize();
    const listing = result(await post('/mcp', { jsonrpc: '2.0', id: ++id, method: 'tools/list' }, headers()), id);
    const tools = listing.tools;
    assert.deepEqual(tools.map(tool => tool.name).sort(), ['brief', 'get_page_sources', 'recall']);
    t.diagnostic(`Public tools/list structural metadata: ${JSON.stringify({
      tools: tools.map(tool => ({
        name: tool.name,
        annotations: Object.fromEntries(annotationKeys.map(key => [key,
          typeof tool.annotations?.[key] === 'boolean' ? tool.annotations[key] : null])),
        outputSchemaPresent: Object.prototype.hasOwnProperty.call(tool, 'outputSchema'),
        authPolicy: {
          securitySchemes: authPolicy(tool.securitySchemes),
          metaSecuritySchemes: authPolicy(tool._meta?.securitySchemes),
        },
      })),
    })}`);
    const brief = projected(result(await call('brief', {}), id));
    assert.equal(brief.space, space);
    assert.equal(brief.brief.active[0].text, 'Use signed requests');
    const topic = projected(result(await call('brief', { topic: 'Atlas authentication decision' }), id));
    assert(topic.related_context.results.some(hit => hit.source_id === 'mem_atlas-auth'));
    const recalled = projected(result(await call('recall', { query: 'Atlas authentication decision', limit: 3, rerank: false }), id));
    assert(recalled.results.length <= 3 && recalled.results.some(hit => hit.source_id === 'mem_atlas-auth'));
    const sources = projected(result(await call('get_page_sources', { page_id: 'page_atlas-auth' }), id));
    assert.equal(sources.sources.length, 1);
    assert(JSON.stringify(sources).includes('mem_atlas-auth'));
    assert.deepEqual(projected(result(await call('get_page_sources', { page_id: 'page_atlas-unavailable' }), id)),
      { page_id: 'page_atlas-unavailable', sources: [] });
    if (transport === 'reverse') {
      const streamAbort = new AbortController();
      let streamReader;
      try {
        const stream = await openSse(`${origin}/mcp`, { ...headers(), accept: 'text/event-stream' }, streamAbort.signal);
        assert.equal(stream.status, 200, 'public reverse GET stream must succeed');
        assert.equal(stream.headers.get('content-type')?.split(';')[0].trim(), 'text/event-stream');
        streamReader = stream.body?.getReader();
        assert(streamReader, 'public reverse GET stream must have a body');
        const heartbeat = await readSseHeartbeat(streamReader);
        assert.match(heartbeat, /(?:^|\r?\n):(?:\r?\n|$)/, 'actual SDK SSE heartbeat required');
        streamAbort.abort();
        await streamReader.cancel().catch(() => {});
      } finally {
        streamAbort.abort();
        await streamReader?.cancel().catch(() => {});
      }
      const afterAbort = projected(result(await call('brief', {}), id));
      assert.deepEqual(afterAbort, brief, 'next authorized tool call must remain responsive after GET abort');
      t.diagnostic('Actual reverse Streamable HTTP GET heartbeat, abort, and next-tool responsiveness passed; exact remote release is not externally observable');
    }
    const deniedPage = result(await call('get_page_sources', { page_id: 'page_private-sentinel' }), id);
    assert.equal(deniedPage.isError, true);
    assert(!JSON.stringify(deniedPage).includes('UNAUTHORIZED_SENTINEL'));
    assert.equal((await call('brief', { space: 'private-sentinel' })).status, 403);
    assert.equal((await call('capture', { content: 'must not be stored' })).status, 403);
    if (process.env.WENLAN_TEST_OFFLINE_RECOVERY === '1') {
      assert.equal(transport, 'reverse', 'offline check requires the owned reverse helper');
      await stop(reverseHelper);
      await delay(2000);
      const unavailable = result(await call('brief', {}), id);
      assert.equal(unavailable.isError, true);
      assert.deepEqual(unavailable.content, [{ type: 'text', text:
        'Wenlan could not reach your local device. Make sure Wenlan is running and the device is online, then try again. This does not mean your authorization was revoked.' }]);
      assert.equal((await call('brief', { space: 'private-sentinel' })).status, 403);
      reverseHelper = start('reverse-helper', reverseHelperBin, [
        '--ignored', '--exact', 'remote_relay::tests::public_reverse_socket_helper', '--nocapture',
      ], { ...env, WENLAN_TEST_PUBLIC_RELAY: '1',
        WENLAN_REVERSE_HELPER_CREDENTIALS_PATH: recoveryPath,
        WENLAN_REVERSE_HELPER_MCP_PORT: String(mcpPort) });
      await readyMarker(reverseHelper);
      let recovered = await call('brief', {});
      if (recovered.status === 404) {
        session = undefined;
        await initialize();
        recovered = await call('brief', {});
      }
      assert.deepEqual(projected(result(recovered, id)), brief);
      t.diagnostic('Live offline tools/call isError, scope denial, and same-device/token recovery passed; MCP session reinitialized on 404');
    }
    const refreshed = await tokenRequest({ grant_type: 'refresh_token', refresh_token: tokens.refresh_token });
    assert.equal(refreshed.status, 200);
    const oldRefresh = tokens.refresh_token;
    tokens = json(refreshed);
    assert(tokens.refresh_token && tokens.refresh_token !== oldRefresh);
    assert.deepEqual(projected(result(await call('brief', {}), id)), brief);
    assert.equal((await request(`${origin}/mcp`, { method: 'DELETE', headers: headers() })).status, 202);
    assert.equal((await call('brief', {})).status, 404);
    session = undefined;
    await initialize();
    assert.equal((await post('/devices/revoke', {}, deviceHeaders())).status, 200);
    revoked = true;
    assert.equal((await call('brief', {})).status, 403);
    const deniedRefresh = await tokenRequest({ grant_type: 'refresh_token', refresh_token: tokens.refresh_token });
    assert.equal(deniedRefresh.status, 400);
    assert.equal(json(deniedRefresh).error, 'invalid_grant');
    if (nativeCall) {
      await nativeDenied('brief', {});
      t.diagnostic('Actual Codex client denied a tool call after device revocation');
    }
  } finally {
    const failures = [];
    if (device && !revoked) {
      try {
        assert.equal((await post('/devices/revoke', {}, deviceHeaders())).status, 200, 'test device revocation');
        revoked = true;
      } catch { failures.push(new Error('Public test revocation unconfirmed')); }
    }
    try { await nativeClient?.close(); } catch (error) { failures.push(error); }
    try { await oauthClient?.close(); } catch (error) { failures.push(error); }
    for (const task of [reverseHelper, tunnel, mcp, daemon]) {
      try { await stop(task); } catch (error) { failures.push(error); }
    }
    for (const port of [mcpPort, daemonPort]) {
      try { await closed(port); } catch (error) { failures.push(error); }
    }
    if (cleanupModelConfig) {
      try { await cleanupModelConfig(); } catch (error) { failures.push(error); }
    }
    if (!failures.length) await rm(scratch, { recursive: true, force: true });
    else throw new AggregateError(failures, `Synthetic recovery files retained at ${scratch}`);
  }
});
