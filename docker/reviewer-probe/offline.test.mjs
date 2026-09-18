import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { randomBytes, randomUUID } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import test from 'node:test';
import { promisify } from 'node:util';

const exec = promisify(execFile);
const image = process.env.WENLAN_OFFLINE_PROBE_IMAGE;
assert.match(image ?? '', /^sha256:[a-f0-9]{64}$/, 'explicit offline probe image ID required');
const docker = (args, options = {}) => {
  const { input, ...rest } = options;
  const pending = exec('docker', args, { timeout: 30_000, maxBuffer: 48_000, ...rest });
  if (input !== undefined) pending.child.stdin.end(input);
  return pending;
};
const inspect = async (name, format) => {
  const result = await docker(['inspect', '--format', format, name]);
  return JSON.parse(result.stdout);
};

const redact = (text, token) => text.split(token).join('[test token]');

function parseExecHttp(raw) {
  const marker = raw.lastIndexOf('\n__STATUS:');
  assert.notEqual(marker, -1, `missing curl status marker in: ${raw.slice(-500)}`);
  const status = Number(raw.slice(marker + '\n__STATUS:'.length).trim());
  const head = raw.slice(0, marker).replace(/\r\n/g, '\n');
  const split = head.indexOf('\n\n');
  assert.notEqual(split, -1, 'missing header/body split');
  const headerText = head.slice(0, split);
  const body = head.slice(split + 2);
  const lines = headerText.split('\n');
  assert.match(lines[0], /^HTTP\/\d(\.\d)? \d{3} /);
  const headers = {};
  for (const line of lines.slice(1)) {
    const colon = line.indexOf(':');
    if (colon === -1) continue;
    headers[line.slice(0, colon).trim().toLowerCase()] = line.slice(colon + 1).trim();
  }
  return { status, headers, body };
}

function decodeMessages(body, contentType) {
  const text = body.trim();
  assert.ok(text.length > 0, 'empty MCP response body');
  if ((contentType ?? '').startsWith('text/event-stream')) {
    return text.split('\n').filter(line => line.startsWith('data:'))
      .map(line => line.slice(5).trim()).filter(Boolean)
      .map(data => JSON.parse(data));
  }
  return [JSON.parse(text)];
}

function projected(value, token) {
  assert.equal(value.isError, false);
  assert.deepEqual(JSON.parse(value.content[0].text), value.structuredContent);
  for (const secret of ['UNAUTHORIZED_SENTINEL', 'mem_private-sentinel', 'synthetic: true']) {
    assert(!JSON.stringify(value).includes(secret), `unexpected output: ${secret}`);
  }
  assert(!JSON.stringify(value).includes(token), 'bearer must never appear in tool output');
  return value.structuredContent;
}

test('offline loopback container serves real MCP with no network or mounts', {
  timeout: 360_000,
}, async () => {
  const owner = randomUUID();
  const name = `wenlan-offline-probe-${owner}`;
  const token = randomBytes(48).toString('base64url');
  const info = await inspect(image, '{{json .}}');
  assert.equal(info.Architecture, 'amd64');
  assert.equal(info.Config.User, '10001');
  assert.ok(!info.Config.Env.some(value => value.startsWith('REVIEWER_BEARER_TOKEN=')));
  let started = false;
  const startedAt = Date.now();
  let readyMs = null;
  try {
    await docker(['run', '--detach', '--pull=never', '--name', name,
      '--label', `wenlan.probe.owner=${owner}`, '--platform', 'linux/amd64',
      '--cpus', '2', '--memory', '4g', '--network', 'none',
      '--env', 'REVIEWER_BEARER_TOKEN', image], {
      env: { ...process.env, REVIEWER_BEARER_TOKEN: token },
    });
    started = true;
    assert.deepEqual(await inspect(name, '{{json .Mounts}}'), []);
    const networks = await inspect(name, '{{json .NetworkSettings.Networks}}');
    assert.deepEqual(Object.keys(networks), ['none']);
    const ports = await inspect(name, '{{json .NetworkSettings.Ports}}');
    assert.ok(!ports || Object.keys(ports).length === 0, `no ports may be published: ${JSON.stringify(ports)}`);
    assert.equal((await inspect(name, '{{json .HostConfig.NetworkMode}}')), 'none');
    await docker(['exec', '--workdir', '/opt', name, 'sha256sum', '-c', 'model-sha256.txt']);

    const curlGet = async (path) => {
      const result = await docker(['exec', name, 'curl', '-sS',
        '--max-time', '5', '-o', '/dev/null', '-w', '%{http_code}',
        `http://127.0.0.1:8080${path}`], { timeout: 15_000, maxBuffer: 8_000 });
      return Number(result.stdout.trim());
    };
    let ready = false;
    const deadline = Date.now() + 120_000;
    while (Date.now() < deadline) {
      const state = await inspect(name, '{{json .State}}');
      assert.equal(state.Running, true, `container exited with ${state.ExitCode}`);
      try {
        if ((await curlGet('/health')) === 200) { ready = true; break; }
      } catch {}
      await delay(1000);
    }
    assert.ok(ready, 'real MCP must become ready within 120s');
    readyMs = Date.now() - startedAt;
    console.log(`offline probe local readiness: ${readyMs}ms (local container only, not hosted cold-start)`);

    let id = 0;
    let session;
    const post = async (payload, authed) => {
      const args = ['exec', '-i', name, 'curl', '-sS', '-i', '--max-time', '10',
        '--config', '-',
        '-X', 'POST', 'http://127.0.0.1:8080/mcp',
        '-H', 'content-type: application/json',
        '-H', 'accept: application/json, text/event-stream',
        '-H', 'mcp-protocol-version: 2025-06-18',
        '-w', '\n__STATUS:%{http_code}',
        '--data-raw', JSON.stringify(payload)];
      if (authed) {
        if (session) args.push('-H', `mcp-session-id: ${session}`);
      }
      const result = await docker(args, {
        input: authed ? `header = "authorization: Bearer ${token}"\n` : '',
        timeout: 15_000, maxBuffer: 48_000,
      });
      return parseExecHttp(result.stdout);
    };
    const resultOf = (response, callId) => {
      assert.equal(response.status, 200, response.body.slice(0, 2000));
      const messages = decodeMessages(response.body, response.headers['content-type']);
      const message = messages.find(message => message.id === callId);
      assert(message, 'matching JSON-RPC response');
      assert.equal(message.error, undefined);
      return message.result;
    };

    const anonymous = await post({ jsonrpc: '2.0', id: ++id, method: 'tools/list' }, false);
    assert.equal(anonymous.status, 401);

    const initId = ++id;
    const initialized = await post({ jsonrpc: '2.0', id: initId, method: 'initialize',
      params: { protocolVersion: '2025-06-18', capabilities: {},
        clientInfo: { name: 'offline-probe', version: '1' } } }, true);
    session = initialized.headers['mcp-session-id'];
    assert.ok(session, 'server must issue mcp-session-id');
    const initInfo = resultOf(initialized, initId);
    assert(initInfo.serverInfo.name);
    assert.equal(initInfo.protocolVersion, '2025-06-18');
    const notified = await post(
      { jsonrpc: '2.0', method: 'notifications/initialized' }, true);
    assert.equal(notified.status, 202);

    const listId = ++id;
    const listed = resultOf(await post(
      { jsonrpc: '2.0', id: listId, method: 'tools/list' }, true), listId);
    assert.deepEqual(listed.tools.map(tool => tool.name).sort(),
      ['brief', 'get_page_sources', 'recall']);

    const call = async toolName => {
      const callId = ++id;
      return { callId, response: await post({ jsonrpc: '2.0', id: callId,
        method: 'tools/call', params: { name: toolName, arguments: toolName === 'brief' ? {}
          : toolName === 'recall'
            ? { query: 'Atlas authentication decision', limit: 3, rerank: false }
            : { page_id: 'page_atlas-auth' } } }, true) };
    };
    const briefCall = await call('brief');
    const brief = projected(resultOf(briefCall.response, briefCall.callId), token);
    assert.equal(brief.space, 'atlas-review');
    assert.equal(brief.brief.active[0].text, 'Use signed requests');
    assert.equal(brief.brief.backlog[0].text, 'Document offline fallback');

    const recallCall = await call('recall');
    const recalled = projected(resultOf(recallCall.response, recallCall.callId), token);
    assert(recalled.results.length <= 3
      && recalled.results.some(hit => hit.source_id === 'mem_atlas-auth'));

    const sourcesCall = await call('get_page_sources');
    const sources = projected(resultOf(sourcesCall.response, sourcesCall.callId), token);
    assert.equal(sources.sources.length, 1);
    assert(JSON.stringify(sources).includes('mem_atlas-auth'));

    // Cross-space sentinel: denied without leaking sentinel material. The
    // direct HTTP MCP surface may report unauthorized space access in its own
    // form, so only the denial + no-leak shape is asserted here (no relay 403
    // assumption without backend source evidence).
    const deniedId = ++id;
    const denied = resultOf(await post({ jsonrpc: '2.0', id: deniedId, method: 'tools/call',
      params: { name: 'get_page_sources', arguments: { page_id: 'page_private-sentinel' } } },
    true), deniedId);
    assert.equal(denied.isError, true);
    assert(!JSON.stringify(denied).includes('UNAUTHORIZED_SENTINEL'));
    assert(!JSON.stringify(denied).includes('mem_private-sentinel'));

    assert.equal((await inspect(name, '{{json .State}}')).Running, true,
      'runtime must still be alive before graceful stop');
    await docker(['stop', '--time', '10', name], { timeout: 20_000 });
    const stopped = await inspect(name, '{{json .State}}');
    assert.equal(stopped.Running, false);
    assert.equal(stopped.ExitCode, 143);

    // Lifecycle phase 2: local re-creation with the same name. This proves
    // stop/remove/recreate behavior only; it is not hosted sleep/wake proof.
    const staleSession = session;
    assert.ok(staleSession, 'first phase must have issued a session to stale');
    const firstLabels = await inspect(name, '{{json .Config.Labels}}');
    assert.equal(firstLabels?.['wenlan.probe.owner'], owner);
    await docker(['rm', name], { timeout: 20_000 });
    await docker(['run', '--detach', '--pull=never', '--name', name,
      '--label', `wenlan.probe.owner=${owner}`, '--platform', 'linux/amd64',
      '--cpus', '2', '--memory', '4g', '--network', 'none',
      '--env', 'REVIEWER_BEARER_TOKEN', image], {
      env: { ...process.env, REVIEWER_BEARER_TOKEN: token },
    });
    assert.deepEqual(await inspect(name, '{{json .Mounts}}'), []);
    assert.deepEqual(Object.keys(await inspect(name, '{{json .NetworkSettings.Networks}}')), ['none']);
    assert.equal((await inspect(name, '{{json .HostConfig.NetworkMode}}')), 'none');
    await docker(['exec', '--workdir', '/opt', name, 'sha256sum', '-c', 'model-sha256.txt']);
    let recreated = false;
    const recreateDeadline = Date.now() + 120_000;
    while (Date.now() < recreateDeadline) {
      const state = await inspect(name, '{{json .State}}');
      assert.equal(state.Running, true, `recreated container exited with ${state.ExitCode}`);
      try {
        if ((await curlGet('/health')) === 200) { recreated = true; break; }
      } catch {}
      await delay(1000);
    }
    assert.ok(recreated, 'recreated MCP must become ready within 120s');

    // The fresh process holds an empty in-memory session store, so the
    // previous session id is unknown there; the Streamable HTTP service
    // reports unknown sessions as 404. Require the actual 404, no fallback.
    const staleId = ++id;
    const stale = await post({ jsonrpc: '2.0', id: staleId, method: 'tools/call',
      params: { name: 'brief', arguments: {} } }, true);
    assert.equal(stale.status, 404);

    session = undefined;
    const reInitId = ++id;
    const reInitialized = await post({ jsonrpc: '2.0', id: reInitId, method: 'initialize',
      params: { protocolVersion: '2025-06-18', capabilities: {},
        clientInfo: { name: 'offline-probe', version: '1' } } }, true);
    session = reInitialized.headers['mcp-session-id'];
    assert.ok(session, 'recreated server must issue mcp-session-id');
    assert.notEqual(session, staleSession);
    const reInitInfo = resultOf(reInitialized, reInitId);
    assert.equal(reInitInfo.protocolVersion, '2025-06-18');
    const reNotified = await post(
      { jsonrpc: '2.0', method: 'notifications/initialized' }, true);
    assert.equal(reNotified.status, 202);
    const reBriefCall = await call('brief');
    const reBrief = projected(resultOf(reBriefCall.response, reBriefCall.callId), token);
    assert.equal(reBrief.space, 'atlas-review');
    assert.equal(reBrief.brief.active[0].text, 'Use signed requests');
    assert.equal(reBrief.brief.backlog[0].text, 'Document offline fallback');

    assert.equal((await inspect(name, '{{json .State}}')).Running, true,
      'recreated runtime must still be alive before graceful stop');
    await docker(['stop', '--time', '10', name], { timeout: 20_000 });
    const reStopped = await inspect(name, '{{json .State}}');
    assert.equal(reStopped.Running, false);
    assert.equal(reStopped.ExitCode, 143);
  } catch (error) {
    if (started) {
      const logs = await docker(['logs', '--tail', '25', name]).catch(() => null);
      if (logs) console.error(redact((logs.stdout + logs.stderr).slice(-6000), token));
    }
    // execFile errors include argv, which can contain the synthetic bearer.
    throw new Error(redact(error.stack ?? String(error), token));
  } finally {
    // Inspect the ownership label even if docker run had an uncertain result.
    const labels = await inspect(name, '{{json .Config.Labels}}').catch(() => null);
    if (labels?.['wenlan.probe.owner'] === owner) await docker(['rm', '--force', name]);
  }
});
