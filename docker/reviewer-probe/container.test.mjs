import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { setTimeout as delay } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import test from 'node:test';

const exec = promisify(execFile);
const image = process.env.WENLAN_CONTAINER_PROBE_IMAGE;
assert.match(image ?? '', /^sha256:[a-f0-9]{64}$/, 'explicit built image ID required');
assert.ok(process.env.WENLAN_RELAY_TOOLCHAIN, 'existing relay toolchain required');
const docker = (args, options = {}) => exec('docker', args, {
  timeout: 30_000, maxBuffer: 24_000, ...options,
});
const inspect = async (name, format) => {
  const result = await docker(['inspect', '--format', format, name]);
  return JSON.parse(result.stdout);
};

test('real synthetic container serves scoped OAuth queries and fails closed on reused data', {
  timeout: 480_000,
}, async () => {
  const owner = randomUUID();
  const name = `wenlan-reviewer-probe-${owner}`;
  const info = await inspect(image, '{{json .}}');
  assert.equal(info.Architecture, 'amd64');
  assert.equal(info.Config.User, '10001');
  assert.ok(!info.Config.Env.some(value => value.startsWith('REVIEWER_BEARER_TOKEN=')));
  let started = false;
  try {
    await docker(['run', '--detach', '--pull=never', '--name', name,
      '--label', `wenlan.probe.owner=${owner}`, '--platform', 'linux/amd64',
      '--cpus', '2', '--memory', '4g', '--publish', '127.0.0.1::8080',
      '--env', 'REVIEWER_BEARER_TOKEN', image], {
      env: { ...process.env, REVIEWER_BEARER_TOKEN: 'b'.repeat(64) },
    });
    started = true;
    assert.deepEqual(await inspect(name, '{{json .Mounts}}'), []);
    const ports = await inspect(name, '{{json .NetworkSettings.Ports}}');
    assert.deepEqual(Object.keys(ports), ['8080/tcp']);
    assert.equal(ports['8080/tcp'].length, 1);
    assert.equal(ports['8080/tcp'][0].HostIp, '127.0.0.1');
    const base = `http://127.0.0.1:${ports['8080/tcp'][0].HostPort}`;
    let ready = false;
    const deadline = Date.now() + 240_000;
    while (Date.now() < deadline) {
      const state = await inspect(name, '{{json .State}}');
      assert.equal(state.Running, true, `container exited with ${state.ExitCode}`);
      try {
        const response = await fetch(`${base}/health`, { signal: AbortSignal.timeout(1000) });
        ready = response.ok;
        await response.body?.cancel();
        if (ready) break;
      } catch {}
      await delay(1000);
    }
    assert.ok(ready, 'real MCP must become ready within four minutes');
    const anonymous = await fetch(`${base}/mcp`, {
      method: 'POST', headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'tools/list' }),
      signal: AbortSignal.timeout(3000),
    });
    assert.equal(anonymous.status, 401);
    await anonymous.body?.cancel();
    const checked = await exec(process.execPath, [fileURLToPath(new URL(
      '../../relay/tests/real-backend-check.mjs', import.meta.url,
    ))], { env: { ...process.env, WENLAN_TEST_MCP_URL: base,
      WENLAN_NO_AUTOSTART: '1' }, timeout: 75_000, maxBuffer: 24_000 });
    assert.match(checked.stdout, /pass 1/, 'existing real-backend suite must pass');
    await docker(['stop', '--time', '10', name]);
    const stopped = await inspect(name, '{{json .State}}');
    assert.equal(stopped.Running, false);
    assert.equal(stopped.ExitCode, 143);
    await assert.rejects(fetch(`${base}/health`, { signal: AbortSignal.timeout(1000) }));
    await docker(['start', name]);
    const reused = await docker(['wait', name], { timeout: 15_000 });
    assert.equal(reused.stdout.trim(), '1', 'existing root must be refused');
  } catch (error) {
    if (started) {
      const logs = await docker(['logs', '--tail', '25', name]).catch(() => null);
      if (logs) console.error((logs.stdout + logs.stderr).replaceAll('b'.repeat(64), '[test token]').slice(-6000));
    }
    throw error;
  } finally {
    // Inspect the ownership label even if docker run had an uncertain result.
    const labels = await inspect(name, '{{json .Config.Labels}}').catch(() => null);
    if (labels?.['wenlan.probe.owner'] === owner) await docker(['rm', '--force', name]);
  }
});
