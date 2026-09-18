// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import {
  validConnectorContract,
  verifyConnector,
  verifyConnectorContract,
  type ConnectorProbe,
} from '../src/connector-check.ts';

const candidate = {
  tunnelOrigin: 'https://fixture.trycloudflare.com',
  backendToken: 'a'.repeat(43), space: 'review',
};
const info = {
  contract_version: 1, server: 'wenlan-mcp', tool_profile: 'query-only',
  authentication: 'bearer', space: 'review',
};
const contract = { backendToken: candidate.backendToken, space: candidate.space };

function fixture(anonymous: number, authenticated: () => Response) {
  let calls = 0;
  const fetcher: typeof fetch = async (url, init) => {
    calls++;
    assert.equal(url, 'https://fixture.trycloudflare.com/connector-info');
    assert.equal(init?.redirect, 'manual');
    assert.ok(init?.signal);
    const token = new Headers(init?.headers).get('authorization');
    if (calls === 1) {
      assert.equal(token, null);
      return new Response(null, { status: anonymous });
    }
    assert.equal(token, `Bearer ${candidate.backendToken}`);
    return authenticated();
  };
  return { fetcher, get calls() { return calls; } };
}

test('requires a protected query-only backend pinned to the requested Space', async () => {
  const state = fixture(401, () => Response.json(info));
  assert.equal(await verifyConnector(candidate, state.fetcher), true);
  assert.equal(state.calls, 2);
});

test('does not send a token to a public, redirecting or missing handshake', async () => {
  for (const status of [200, 302, 403, 404, 500]) {
    const state = fixture(status, () => Response.json(info));
    assert.equal(await verifyConnector(candidate, state.fetcher), false);
    assert.equal(state.calls, 1);
  }
});

test('rejects unsupported contracts, broad tools and a different Space', async () => {
  for (const wrong of [null, {}, { ...info, contract_version: 2 },
    { ...info, server: 'other' }, { ...info, tool_profile: 'standard' },
    { ...info, authentication: 'none' }, { ...info, space: 'private' }]) {
    assert.equal(await verifyConnector(candidate, fixture(401, () => Response.json(wrong)).fetcher), false);
  }
});

test('rejects redirect, credential rejection, malformed and oversized responses', async () => {
  for (const response of [() => Response.redirect('https://example.invalid'),
    () => new Response(null, { status: 401 }), () => new Response('not JSON'),
    () => new Response('invalid', { headers: { 'content-type': 'application/json' } }),
    () => Response.json({ ...info, extra: 'x'.repeat(4096) })]) {
    assert.equal(await verifyConnector(candidate, fixture(401, response).fetcher), false);
  }
});

test('invalid target and credentials cannot cause a network request', async () => {
  const fetcher: typeof fetch = async () => { assert.fail('must not fetch'); };
  for (const value of [{ ...candidate, tunnelOrigin: 'https://example.invalid/?trycloudflare.com' },
    { ...candidate, backendToken: 'short' }, { ...candidate, space: '' },
    { ...candidate, space: ' review' }]) {
    assert.equal(await verifyConnector(value, fetcher), false);
  }
});

test('upstream failures are returned without private exception details', async () => {
  assert.equal(await verifyConnector(candidate, async () => { throw new Error('PRIVATE'); }), false);
});

test('verifies a connector through a direct probe without a target URL', async () => {
  let calls = 0;
  const probe: ConnectorProbe = async (headers, signal) => {
    calls++;
    assert.ok(signal);
    if (calls === 1) {
      assert.equal(headers.get('authorization'), null);
      return new Response(null, { status: 401 });
    }
    assert.equal(headers.get('authorization'), `Bearer ${candidate.backendToken}`);
    assert.equal(headers.get('accept'), 'application/json');
    return Response.json(info);
  };
  assert.equal(await verifyConnectorContract(contract, probe), true);
  assert.equal(calls, 2);
});

test('invalid connector contracts are rejected before probing', async () => {
  let probes = 0;
  const probe: ConnectorProbe = async () => {
    probes++;
    return Response.json(info);
  };
  for (const invalid of [
    { backendToken: 'short', space: 'review' },
    { backendToken: 'a'.repeat(32), space: ' ' },
    { backendToken: 'a'.repeat(32), space: ' review' },
    { backendToken: 'a'.repeat(32), space: 'x'.repeat(257) },
    { backendToken: 'a'.repeat(32), space: 'bad\u0000' },
  ]) {
    assert.equal(validConnectorContract(invalid), false);
    assert.equal(await verifyConnectorContract(invalid, probe), false);
  }
  assert.equal(probes, 0);
});

test('direct verification requires anonymous 401 before sending the token', async () => {
  let calls = 0;
  const probe: ConnectorProbe = async headers => {
    calls++;
    assert.equal(headers.get('authorization'), null);
    return Response.json(info);
  };
  assert.equal(await verifyConnectorContract(contract, probe), false);
  assert.equal(calls, 1);
});

test('direct verification requires the exact query-only contract scope and profile', async () => {
  for (const wrong of [
    { ...info, tool_profile: 'standard' },
    { ...info, authentication: 'none' },
    { ...info, server: 'other' },
  ]) {
    let calls = 0;
    const probe: ConnectorProbe = async headers => {
      calls++;
      if (calls === 1) {
        assert.equal(headers.get('authorization'), null);
        return new Response(null, { status: 401 });
      }
      assert.equal(headers.get('authorization'), `Bearer ${candidate.backendToken}`);
      return Response.json(wrong);
    };
    assert.equal(await verifyConnectorContract(contract, probe), false);
    assert.equal(calls, 2);
  }
});

test('snapshots credentials and Space before the anonymous probe awaits', async () => {
  const mutableCandidate = { backendToken: candidate.backendToken, space: candidate.space };
  const originalToken = mutableCandidate.backendToken;
  const originalSpace = mutableCandidate.space;
  let releaseAnonymous = () => {};
  let anonymousStarted = () => {};
  const anonymousPaused = new Promise<void>(resolve => { releaseAnonymous = resolve; });
  const anonymousProbeStarted = new Promise<void>(resolve => { anonymousStarted = resolve; });
  let calls = 0;
  const probe: ConnectorProbe = async headers => {
    calls++;
    if (calls === 1) {
      assert.equal(headers.get('authorization'), null);
      anonymousStarted();
      await anonymousPaused;
      return new Response(null, { status: 401 });
    }
    assert.equal(headers.get('authorization'), `Bearer ${originalToken}`);
    return Response.json({ ...info, space: originalSpace });
  };

  const verification = verifyConnectorContract(mutableCandidate, probe);
  await anonymousProbeStarted;
  mutableCandidate.backendToken = 'b'.repeat(originalToken.length);
  mutableCandidate.space = 'mutated';
  releaseAnonymous();

  assert.equal(await verification, true);
  assert.equal(calls, 2);
});
