// SPDX-License-Identifier: Apache-2.0
// Backend-only fixture: actual local Codex plugin route over an isolated daemon.
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, isAbsolute, join } from 'node:path';
import { promisify } from 'node:util';
import { startCodexClient, CodexRpcError } from './codex-client.mjs';
import { validateModelHome } from './codex-model-home.mjs';

const execFileAsync = promisify(execFile);

function shq(value) {
  return `'${String(value).replace(/'/g, `'\\''`)}'`;
}

function sha256File(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function findFiles(dir, out = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) findFiles(full, out);
    else if (entry.isFile()) out.push(full);
  }
  return out;
}

export async function checkCodexPlugin({ codex, mcpBin, root, daemonUrl, mapping, repoRoot, modelHome }) {
  for (const p of [codex, mcpBin, root, repoRoot]) assert(p && isAbsolute(p), 'absolute path required');
  assert(daemonUrl && typeof daemonUrl === 'string');
  const url = new URL(daemonUrl);
  assert.equal(url.protocol, 'http:');
  assert.equal(url.hostname, '127.0.0.1', 'numeric loopback daemon required');
  assert(Number.isInteger(Number(url.port)) && Number(url.port) > 0);
  assert.equal(url.pathname, '/');
  assert(!url.username && !url.password && !url.search && !url.hash);
  assert.notEqual(Number(url.port), 7878);
  for (const key of ['page_atlas-auth', 'mem_atlas-auth', 'page_private-sentinel',
    'page_atlas-unavailable', 'mem_atlas-unavailable']) assert(mapping?.[key], `missing fixture ${key}`);
  if (modelHome !== undefined) await validateModelHome(modelHome);
  const modelEnabled = modelHome !== undefined;

  const probe = join(root, 'codex-plugin-probe');
  mkdirSync(probe, { mode: 0o700 });
  const home = join(probe, 'home');
  const codexHome = modelHome ?? join(probe, 'codex-home');
  const workspace = join(probe, 'workspace');
  const cacheDir = join(probe, 'mcp-cache');
  for (const dir of [home, workspace, cacheDir]) mkdirSync(dir, { mode: 0o700 });
  if (!modelEnabled) mkdirSync(codexHome, { mode: 0o700 });

  const marketplace = join(probe, 'marketplace');
  mkdirSync(marketplace);
  cpSync(join(repoRoot, 'plugin-codex'), join(marketplace, 'plugin-codex'), { recursive: true });
  mkdirSync(join(marketplace, '.agents', 'plugins'), { recursive: true });
  cpSync(join(repoRoot, '.agents', 'plugins', 'marketplace.json'),
    join(marketplace, '.agents', 'plugins', 'marketplace.json'));

  const runnerOriginal = join(marketplace, 'plugin-codex', 'bin', 'wenlan-mcp-runner.sh');
  const manifestOriginal = join(marketplace, 'plugin-codex', '.codex-plugin', 'plugin.json');
  const mcpJsonOriginal = join(marketplace, 'plugin-codex', '.mcp.json');
  const siblingLocal = join(marketplace, 'plugin-codex', 'bin', 'wenlan-mcp.local');
  assert(!existsSync(siblingLocal), 'sibling override must be absent before fixture shim');

  const pidFile = join(probe, 'mcp.pid');
  const shim = `#!/bin/sh\n` +
    `export WENLAN_SPACE="atlas-review"\n` +
    `export WENLAN_NO_AUTOSTART="1"\n` +
    `export HOME=${shq(home)}\n` +
    `export WENLAN_DATA_DIR=${shq(root)}\n` +
    `export WENLAN_MCP_CACHE_DIR=${shq(cacheDir)}\n` +
    `printf '%s' "$$" > ${shq(pidFile)}\n` +
    `exec ${shq(mcpBin)} --origin-url ${shq(daemonUrl)} "$@"\n`;
  writeFileSync(siblingLocal, shim, { flag: 'wx', mode: 0o700 });
  chmodSync(siblingLocal, 0o700);

  const config = `cli_auth_credentials_store = "file"\nmodel = "gpt-5.6-luna"\n` +
    `model_reasoning_effort = "low"\nweb_search = "disabled"\n` +
    `[features]\nview_image = false\nshell_tool = false\nunified_exec = false\n` +
    `js_repl = false\napps = false\nplugins = true\nbrowser_use = false\ncomputer_use = false\n`;
  writeFileSync(join(codexHome, 'config.toml'), config, { flag: 'wx', mode: 0o600 });

  const cleanBase = { HOME: home, USERPROFILE: home, CODEX_HOME: codexHome,
    PATH: process.env.PATH, LANG: 'en_US.UTF-8' };
  const execOpts = { cwd: workspace, timeout: 60_000, maxBuffer: 1_048_576, env: cleanBase };
  const marketplaceAdd = await execFileAsync(codex,
    ['plugin', 'marketplace', 'add', marketplace, '--json'], execOpts);
  writeFileSync(join(probe, 'marketplace-add.json'),
    JSON.stringify({ stdout: marketplaceAdd.stdout, stderr: marketplaceAdd.stderr }, null, 2),
    { flag: 'wx', mode: 0o600 });
  const pluginAdd = await execFileAsync(codex,
    ['plugin', 'add', 'wenlan@7xuanlu-wenlan', '--json'], execOpts);
  writeFileSync(join(probe, 'plugin-add.json'),
    JSON.stringify({ stdout: pluginAdd.stdout, stderr: pluginAdd.stderr }, null, 2),
    { flag: 'wx', mode: 0o600 });
  const installation = JSON.parse(pluginAdd.stdout);
  assert.equal(installation.pluginId, 'wenlan@7xuanlu-wenlan');
  assert.equal(installation.version, JSON.parse(readFileSync(manifestOriginal, 'utf8')).version);
  assert(!/^\[mcp_servers[.\]]/m.test(readFileSync(join(codexHome, 'config.toml'), 'utf8')),
    'manual MCP configuration is not plugin evidence');

  const cacheRoot = join(codexHome, 'plugins', 'cache');
  assert(existsSync(cacheRoot), 'plugin cache must exist below owned CODEX_HOME');
  const cached = existsSync(cacheRoot) ? findFiles(cacheRoot) : [];
  const installedRunner = cached.find(p => p.endsWith('bin/wenlan-mcp-runner.sh'));
  const installedManifest = cached.find(p => p.endsWith('.codex-plugin/plugin.json'));
  assert(installedRunner && installedManifest, 'installed runner and manifest required');
  assert.equal(sha256File(installedRunner), sha256File(runnerOriginal));
  assert.equal(sha256File(installedManifest), sha256File(manifestOriginal));
  assert.equal(sha256File(join(dirname(installedRunner), 'wenlan-mcp.local')), sha256File(siblingLocal),
    'installed isolation shim must match before any MCP process starts');
  assert.equal(sha256File(join(dirname(dirname(installedRunner)), '.mcp.json')), sha256File(mcpJsonOriginal));
  assert.equal(dirname(dirname(installedRunner)), installation.installedPath);

  const clientEnv = { ...cleanBase, BROWSER: '/usr/bin/false',
    WENLAN_NO_AUTOSTART: '1', WENLAN_MCP_CACHE_DIR: cacheDir };
  const client = startCodexClient(codex, clientEnv, workspace);
  const calls = [];
  let listing;
  let mcpExited = false;
  let modelVerified = false;
  try {
    await client.rpc('initialize', { clientInfo: { name: 'wenlan-plugin-check', version: '1' } });
    client.notify('initialized');
    if (modelEnabled) assert((await client.rpc('account/read', { refreshToken: false })).account);
    const session = await client.rpc('thread/start', { cwd: workspace, ephemeral: true,
      ...(modelEnabled ? { model: 'gpt-5.6-luna', allowProviderModelFallback: false } : {}),
      sandbox: 'read-only', approvalPolicy: 'on-request' });
    if (modelEnabled) {
      assert.equal(session.model, 'gpt-5.6-luna');
      assert.equal(session.reasoningEffort, 'low');
    }
    const threadId = session.thread.id;
    listing = await client.rpc('mcpServerStatus/list', { threadId, detail: 'toolsAndAuthOnly' });
    writeFileSync(join(probe, 'mcp-listing.json'), JSON.stringify(listing, null, 2), { flag: 'wx', mode: 0o600 });
    assert.equal(listing.data.length, 1, 'exactly one plugin-generated server expected');
    const server = listing.data[0].name ?? listing.data[0].server ?? listing.data[0].id;
    assert(server && typeof server === 'string', 'generated server name required');
    const names = Object.values(listing.data[0].tools).map(t => t.name);
    for (const tool of ['brief', 'recall', 'get_page_sources', 'capture']) {
      assert(names.includes(tool), `Standard tool ${tool} required`);
    }
    async function call(tool, args) {
      const output = await client.rpc('mcpServer/tool/call', { threadId, server, tool, arguments: args });
      calls.push({ tool, args, output });
      assert(!JSON.stringify(output).includes('UNAUTHORIZED_SENTINEL'));
      return output;
    }
    const brief = await call('brief', {});
    assert.notEqual(brief.isError, true);
    assert(JSON.stringify(brief).includes('Use signed requests'));
    const topic = await call('brief', { topic: 'Atlas authentication decision' });
    assert.notEqual(topic.isError, true);
    assert(JSON.stringify(topic).includes(mapping['mem_atlas-auth']));
    const recall = await call('recall', { query: 'Atlas authentication decision', limit: 3, rerank: false });
    assert.notEqual(recall.isError, true);
    assert(JSON.stringify(recall).includes(mapping['mem_atlas-auth']));
    const sources = await call('get_page_sources', { page_id: mapping['page_atlas-auth'] });
    assert.notEqual(sources.isError, true);
    assert(JSON.stringify(sources).includes(mapping['mem_atlas-auth']));
    const unavailable = await call('get_page_sources', { page_id: mapping['page_atlas-unavailable'] });
    assert.notEqual(unavailable.isError, true);
    const text = unavailable.content.map(item => item.text ?? '').join('');
    const unavailableLinks = JSON.parse(text.slice(text.indexOf('\n') + 1));
    assert.match(text, /^0 sources\n/);
    assert.deepEqual(unavailableLinks, [], 'strict pinned Standard must omit unavailable links');
    assert(!JSON.stringify(unavailable).includes(mapping['mem_atlas-unavailable']));
    assert.deepEqual(await call('brief', { space: 'private-sentinel' }), brief);
    try {
      assert.equal((await call('get_page_sources', { page_id: mapping['page_private-sentinel'] })).isError, true);
    } catch (error) {
      if (!(error instanceof CodexRpcError)) throw error;
      calls.push({ tool: 'get_page_sources', args: { page_id: mapping['page_private-sentinel'] }, deniedByRpc: true });
    }
    if (modelEnabled) {
      const { runCodexPluginModel } = await import('./codex-plugin-model-check.mjs');
      await runCodexPluginModel(client, threadId, { root: probe, mapping, server });
      modelVerified = true;
    }
  } finally {
    writeFileSync(join(probe, 'tool-observations.json'), JSON.stringify(calls, null, 2), { flag: 'wx', mode: 0o600 });
    await client.close();
    writeFileSync(join(probe, 'codex-process.json'), JSON.stringify({
      pid: client.child.pid, exitCode: client.child.exitCode, signalCode: client.child.signalCode,
    }, null, 2), { flag: 'wx', mode: 0o600 });
    if (existsSync(pidFile)) {
      const pid = Number(readFileSync(pidFile, 'utf8').trim());
      assert(Number.isInteger(pid) && pid > 0);
      const deadline = Date.now() + 5000;
      while (Date.now() < deadline) {
        try { process.kill(pid, 0); } catch (error) {
          if (error.code === 'ESRCH') { mcpExited = true; break; }
          throw error;
        }
        await new Promise(resolve => setTimeout(resolve, 100));
      }
      writeFileSync(join(probe, 'mcp-process.json'), JSON.stringify({ pid, exited: mcpExited }),
        { flag: 'wx', mode: 0o600 });
      assert(mcpExited, 'owned MCP process must exit');
    }
  }
  const childEnd = { exitCode: client.child.exitCode, signalCode: client.child.signalCode };
  assert.equal(childEnd.exitCode, 0);
  assert.equal(childEnd.signalCode, null);
  assert(existsSync(pidFile), 'fixture shim must have actually run');

  const receipt = {
    binarySha256: sha256File(mcpBin),
    pluginSource: {
      runner: sha256File(runnerOriginal),
      manifest: sha256File(manifestOriginal),
      mcpJson: sha256File(mcpJsonOriginal),
    },
    listing, calls, childEnd, mcpExited,
    model: modelVerified ? { name: 'gpt-5.6-luna', effort: 'low', positiveCases: 5,
      localGeneralNegative: 1, evidence: ['model-positive.json', 'model-general-negative.json'] } : null,
    limits: [...(modelVerified ? ['public QueryOnly three-negative matrix not exercised'] : ['no model turn']),
      'no native UI', 'no public relay', 'no OAuth', 'no submission proof'],
  };
  writeFileSync(join(probe, 'codex-plugin-receipt.json'), JSON.stringify(receipt, null, 2), { flag: 'wx', mode: 0o600 });
  return { probe, listing, calls, childEnd };
}
