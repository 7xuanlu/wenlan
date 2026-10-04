// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { seedReviewerViaApi } from './fixtures/reviewer-api-seed.mjs';

const ORIGIN = 'http://127.0.0.1:18081';
const KNOWLEDGE = '/tmp/knowledge';

function fakeFetch({ spaces = [], briefReceipt, statusOverrides = {}, onStore, onPage, onMove, onSources, rawBodies = {} } = {}) {
  const calls = [];
  const memIds = ['m_auth', 'm_unavail', 'm_sent'];
  const pageIds = ['p_auth', 'p_unavail', 'p_sent'];
  let mem = 0;
  let page = 0;
  const ok = (json) => ({ ok: true, status: 200, json: async () => json });
  const defaultSources = (path) => {
    if (path === '/api/pages/p_auth/sources') return [{ memory: { source_id: 'm_auth' } }];
    if (path === '/api/pages/p_unavail/sources') return [{ memory: null }];
    return [];
  };
  const fetchImpl = async (url, init = {}) => {
    const { method = 'GET' } = init;
    const path = String(url).slice(ORIGIN.length);
    const body = init.body ? JSON.parse(init.body) : undefined;
    calls.push({ method, path, body, redirect: init.redirect, headers: init.headers });
    const key = `${method} ${path}`;
    if (statusOverrides[key] !== undefined) {
      return { ok: false, status: statusOverrides[key], text: async () => 'boom', json: async () => ({}) };
    }
    if (rawBodies[key] !== undefined) {
      const raw = rawBodies[key];
      return { ok: true, status: 200, json: async () => { throw new SyntaxError(raw); } };
    }
    if (key === 'GET /api/knowledge/path') return ok({ path: KNOWLEDGE });
    if (key === 'GET /api/spaces') return ok(spaces);
    if (key === 'POST /api/spaces') return ok({ name: body.name });
    if (key === 'PATCH /api/brief') {
      return ok(briefReceipt ?? {
        space: 'atlas-review',
        applied: [{ kind: 'summary' }, { kind: 'add', mutation_index: 0 }, { kind: 'add', mutation_index: 1 }],
        conflicts: [],
      });
    }
    if (key === 'POST /api/memory/store') {
      const res = { source_id: memIds[mem++], space: body.space, write_outcome: 'created' };
      return ok(onStore ? onStore(res, body) : res);
    }
    if (key === 'POST /api/pages') {
      const res = { id: pageIds[page++], space: body.space, write_outcome: 'created' };
      return ok(onPage ? onPage(res, body) : res);
    }
    if (path.startsWith('/api/documents/') && path.endsWith('/space')) {
      return ok(onMove ? onMove({ ok: true }, body) : { ok: true });
    }
    if (path.startsWith('/api/pages/') && path.endsWith('/sources')) {
      const res = onSources ? onSources(defaultSources(path), path) : defaultSources(path);
      return ok(res);
    }
    return { ok: false, status: 404, text: async () => 'unknown', json: async () => ({}) };
  };
  return { calls, fetchImpl };
}

const EXPECTED_MAPPING = {
  'mem_atlas-auth': 'm_auth',
  'mem_atlas-unavailable': 'm_unavail',
  'mem_private-sentinel': 'm_sent',
  'page_atlas-auth': 'p_auth',
  'page_atlas-unavailable': 'p_unavail',
  'page_private-sentinel': 'p_sent',
};

describe('reviewer-api-seed', () => {
  it('retains advisory near-duplicate warnings but rejects reused generated IDs', async () => {
    const advisory = fakeFetch({ onStore: res => ({ ...res, near_duplicate: { source_id: 'other', similarity: 0.9 } }) });
    const result = await seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: advisory.fetchImpl });
    assert.equal(result.warnings.length, 3);
    const repeated = fakeFetch({ onStore: res => ({ ...res, source_id: 'same_id' }) });
    await assert.rejects(seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: repeated.fetchImpl }), /duplicate generated ID/);
  });
  it('seeds through existing endpoints in order and returns the ID mapping', async () => {
    const { calls, fetchImpl } = fakeFetch();
    const { mapping, pageReviewNotModified, sourceChecks } = await seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl });
    assert.deepEqual(mapping, EXPECTED_MAPPING);
    assert.deepEqual(pageReviewNotModified, ['p_auth', 'p_unavail', 'p_sent']);
    assert.deepEqual(sourceChecks, { authAvailable: 1, unavailableLinks: 1, unavailableAvailable: 0 });
    const seq = calls.map((c) => `${c.method} ${c.path}`);
    assert.deepEqual(seq.slice(0, 5), [
      'GET /api/knowledge/path',
      'GET /api/spaces',
      'POST /api/spaces',
      'POST /api/spaces',
      'PATCH /api/brief',
    ]);
    assert.equal(calls.filter((c) => c.method === 'POST' && c.path === '/api/memory/store').length, 3);
    assert.equal(calls.filter((c) => c.method === 'POST' && c.path === '/api/pages').length, 3);
    assert.match(`${calls.at(-3).method} ${calls.at(-3).path}`, /^POST \/api\/documents\/.+\/space$/);
    assert.deepEqual(calls.slice(-2).map((c) => `${c.method} ${c.path}`), [
      'GET /api/pages/p_auth/sources',
      'GET /api/pages/p_unavail/sources',
    ]);
    for (const c of calls.slice(-2)) {
      assert.equal(c.headers?.['X-Wenlan-Space'], 'atlas-review');
    }
    for (const c of calls.slice(0, -2)) {
      assert.equal(c.headers?.['X-Wenlan-Space'], undefined);
    }
    const brief = calls.find((c) => c.path === '/api/brief').body;
    assert.equal(brief.space, 'atlas-review');
    assert.deepEqual(brief.mutations.map((m) => m.state), ['active', 'backlog']);
    const pages = calls.filter((c) => c.path === '/api/pages').map((c) => c.body);
    assert.ok(pages.every((p) => p.creation_kind === 'authored' && p.workspace === p.space));
    assert.deepEqual(pages.map((p) => p.source_memory_ids), [['m_auth'], ['m_unavail'], ['m_sent']]);
    assert.equal(calls.at(-3).body.space_name, 'private-sentinel');
    assert.ok(calls.every((c) => c.redirect === 'error'));
  });

  it('refuses unsafe daemon URLs and relative knowledge paths', async () => {
    const { fetchImpl } = fakeFetch();
    for (const daemonUrl of [
      'http://127.0.0.1:7878',
      'http://localhost:18081',
      'https://127.0.0.1:18081',
      'http://user:pass@127.0.0.1:18081',
      'http://127.0.0.1:18081/extra',
      'http://127.0.0.1:18081?x=1',
      'http://127.0.0.1',
    ]) {
      await assert.rejects(() => seedReviewerViaApi({ daemonUrl, expectedKnowledgePath: KNOWLEDGE, fetchImpl }), /daemonUrl/);
    }
    await assert.rejects(() => seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: 'relative/path', fetchImpl }), /absolute/);
  });

  it('refuses mismatched knowledge path and existing fixture spaces', async () => {
    const { fetchImpl } = fakeFetch();
    await assert.rejects(() => seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: '/other', fetchImpl }), /mismatch/);
    for (const name of ['atlas-review', 'private-sentinel']) {
      const existing = fakeFetch({ spaces: [{ name }] });
      await assert.rejects(() => seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: existing.fetchImpl }), /existing/);
    }
  });

  it('fails closed on malformed spaces with zero writes', async () => {
    for (const spaces of [{ bogus: 1 }, 'oops', [{ no_name: true }]]) {
      const { calls, fetchImpl } = fakeFetch({ spaces });
      await assert.rejects(() => seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl }), /malformed/);
      assert.deepEqual(calls.map((c) => `${c.method} ${c.path}`), ['GET /api/knowledge/path', 'GET /api/spaces']);
    }
  });

  it('rejects partial brief receipts and conflicted updates', async () => {
    const partial = fakeFetch({
      briefReceipt: { space: 'atlas-review', applied: [{ kind: 'add' }, { kind: 'add' }], conflicts: [] },
    });
    await assert.rejects(() => seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: partial.fetchImpl }), /did not fully apply/);
    const conflicted = fakeFetch({
      briefReceipt: {
        space: 'atlas-review',
        applied: [{ kind: 'summary' }, { kind: 'add' }, { kind: 'add' }],
        conflicts: [{ reason: 'brief_version_mismatch' }],
      },
    });
    await assert.rejects(() => seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: conflicted.fetchImpl }), /conflicts/);
  });

  it('fails closed on non-JSON, malformed IDs, and non-2xx', async () => {
    const raw = fakeFetch({ rawBodies: { 'GET /api/spaces': 'not json' } });
    await assert.rejects(() => seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: raw.fetchImpl }), /non-JSON/);
    const badId = fakeFetch({ onStore: () => ({ source_id: 'not an id!!', space: 'atlas-review', write_outcome: 'created' }) });
    await assert.rejects(() => seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: badId.fetchImpl }), /malformed/);
    const fail = fakeFetch({ statusOverrides: { 'POST /api/spaces': 500 } });
    await assert.rejects(() => seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: fail.fetchImpl }), /returned 500/);
  });

  it('rejects wrong-space, non-created, and staged writes plus unconfirmed moves', async () => {
    const wrongSpace = fakeFetch({ onStore: (res) => ({ ...res, space: 'elsewhere' }) });
    await assert.rejects(() => seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: wrongSpace.fetchImpl }), /wrong space/);
    const resolved = fakeFetch({ onStore: (res) => ({ ...res, write_outcome: 'resolved_existing' }) });
    await assert.rejects(() => seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: resolved.fetchImpl }), /not created/);
    const gated = fakeFetch({ onStore: (res) => ({ ...res, gated: true }) });
    await assert.rejects(() => seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: gated.fetchImpl }), /staged/);
    const queuedPage = fakeFetch({ onPage: (res) => ({ ...res, queued: true }) });
    await assert.rejects(() => seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: queuedPage.fetchImpl }), /staged/);
    const attached = fakeFetch({ onPage: (res) => ({ ...res, attached_to: 'p0', write_outcome: 'attached_existing' }) });
    await assert.rejects(() => seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: attached.fetchImpl }), /attached/);
    const pageSpace = fakeFetch({ onPage: (res) => ({ ...res, space: 'elsewhere' }) });
    await assert.rejects(() => seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: pageSpace.fetchImpl }), /wrong space/);
    const moveBad = fakeFetch({ onMove: () => ({ ok: false }) });
    await assert.rejects(() => seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: moveBad.fetchImpl }), /ok:true/);
  });

  it('rejects missing or malformed memory fields rather than treating them as unavailable', async () => {
    for (const entry of [{}, { memory: false }, { memory: [] }, { memory: {} }, { memory: { source_id: '' } }]) {
      const malformed = fakeFetch({ onSources: (res, path) => path.endsWith('/p_unavail/sources') ? [entry] : res });
      await assert.rejects(seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: malformed.fetchImpl }), /malformed memory/);
    }
  });

  it('rejects lying move acknowledgements and bad source readbacks', async () => {
    const lying = fakeFetch({ onSources: (res, path) => (path.endsWith('/p_unavail/sources') ? [{ memory: { source_id: 'm_unavail' } }] : res) });
    await assert.rejects(seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: lying.fetchImpl }), /zero available/);
    const wrongAuth = fakeFetch({ onSources: (res, path) => (path.endsWith('/p_auth/sources') ? [{ memory: { source_id: 'm_sent' } }] : res) });
    await assert.rejects(seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: wrongAuth.fetchImpl }), /wrong source/);
    const emptyAuth = fakeFetch({ onSources: (res, path) => (path.endsWith('/p_auth/sources') ? [] : res) });
    await assert.rejects(seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: emptyAuth.fetchImpl }), /exactly one available/);
    const emptyUnavail = fakeFetch({ onSources: (res, path) => (path.endsWith('/p_unavail/sources') ? [] : res) });
    await assert.rejects(seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: emptyUnavail.fetchImpl }), /retain one source link/);
    const malformed = fakeFetch({ onSources: () => ({ bogus: 1 }) });
    const malformedCalls = malformed.calls;
    await assert.rejects(seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: malformed.fetchImpl }), /malformed/);
    assert.ok(malformedCalls.some((c) => c.path.endsWith('/sources')));
    const failed = fakeFetch({ statusOverrides: { 'GET /api/pages/p_auth/sources': 500 } });
    await assert.rejects(seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: failed.fetchImpl }), /returned 500/);
    const rawReadback = fakeFetch({ rawBodies: { 'GET /api/pages/p_auth/sources': 'not json' } });
    await assert.rejects(seedReviewerViaApi({ daemonUrl: ORIGIN, expectedKnowledgePath: KNOWLEDGE, fetchImpl: rawReadback.fetchImpl }), /non-JSON/);
  });
});
