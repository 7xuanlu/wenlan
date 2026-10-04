// SPDX-License-Identifier: Apache-2.0
// Shared synthetic reviewer preparation helper (not an MCP tool/backend).
// Seeds the reviewer fixture through EXISTING daemon HTTP endpoints only.
// No autostart, file/DB access, deletion, or presence minting. Fails closed,
// no rollback. Origin checks protect only the explicitly-owned scratch
// daemon setup, not arbitrary callers. NOT submit-ready: this helper alone
// does not prove installation, UI or target-client acceptance.

import { isAbsolute } from 'node:path';

const SPACE = 'atlas-review';
const OTHER = 'private-sentinel';
const AGENT = 'reviewer-real-backend';
const FORBIDDEN_PORT = 7878;
const TIMEOUT_MS = 10_000;
const ID_RE = /^[A-Za-z0-9][A-Za-z0-9_-]{0,127}$/;

function fail(message, extra) {
  const err = new Error(extra ? `${message}: ${extra}` : message);
  err.code = 'reviewer-seed-failed';
  throw err;
}

export function checkDaemonUrl(daemonUrl) {
  let url;
  try {
    url = new URL(daemonUrl);
  } catch {
    fail('daemonUrl must be an explicit http://127.0.0.1:<port> origin');
  }
  if (url.protocol !== 'http:') fail('daemonUrl must use http:');
  if (url.hostname !== '127.0.0.1') fail('daemonUrl host must be 127.0.0.1');
  if (url.username || url.password) fail('daemonUrl must not carry credentials');
  if (url.pathname !== '/' && url.pathname !== '') fail('daemonUrl must not carry a path');
  if (url.search || url.hash) fail('daemonUrl must not carry query or hash');
  if (daemonUrl !== url.origin && daemonUrl !== `${url.origin}/`) fail('daemonUrl must be a canonical explicit origin');
  const port = Number(url.port);
  if (!Number.isInteger(port) || port <= 0 || port > 65535) fail('daemonUrl must carry an explicit port');
  if (port === FORBIDDEN_PORT) fail(`daemonUrl must not use default port ${FORBIDDEN_PORT}`);
  return url.origin;
}

function checkId(value, label) {
  if (typeof value !== 'string' || !ID_RE.test(value)) {
    fail(`malformed generated ID for ${label}`);
  }
  return value;
}

function spaceNames(body) {
  const list = Array.isArray(body) ? body : body?.spaces;
  if (!Array.isArray(list)) fail('GET /api/spaces returned malformed body');
  return list.map((entry) => {
    const name = typeof entry === 'string' ? entry : entry?.name;
    if (typeof name !== 'string' || name.length === 0) {
      fail('GET /api/spaces returned malformed entry');
    }
    return name;
  });
}

async function call(fetchImpl, origin, method, path, body, opts) {
  let res;
  const headers = { 'content-type': 'application/json' };
  if (opts?.space !== undefined) headers['X-Wenlan-Space'] = opts.space;
  try {
    res = await fetchImpl(`${origin}${path}`, {
      method,
      redirect: 'error',
      signal: AbortSignal.timeout(TIMEOUT_MS),
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
  } catch (err) {
    fail(`${method} ${path} request failed`, err?.message);
  }
  if (!res.ok) {
    let detail = '';
    try {
      detail = (await res.text()).slice(0, 300);
    } catch {}
    fail(`${method} ${path} returned ${res.status}`, detail);
  }
  try {
    return await res.json();
  } catch {
    fail(`${method} ${path} returned non-JSON`);
  }
}

// Replaces the direct-DB assumption in
// crates/wenlan-mcp/examples/support/reviewer_seed.rs with HTTP calls.
export async function seedReviewerViaApi({ daemonUrl, expectedKnowledgePath, fetchImpl = globalThis.fetch }) {
  const origin = checkDaemonUrl(daemonUrl);
  if (typeof expectedKnowledgePath !== 'string' || !isAbsolute(expectedKnowledgePath)) {
    fail('expectedKnowledgePath must be an absolute path');
  }

  const known = await call(fetchImpl, origin, 'GET', '/api/knowledge/path');
  if (known?.path !== expectedKnowledgePath) {
    fail('knowledge path mismatch', `expected ${expectedKnowledgePath} got ${known?.path}`);
  }

  const names = spaceNames(await call(fetchImpl, origin, 'GET', '/api/spaces'));
  if (names.includes(SPACE) || names.includes(OTHER)) {
    fail('refusing to seed over existing atlas-review or private-sentinel spaces');
  }

  for (const name of [SPACE, OTHER]) {
    const created = await call(fetchImpl, origin, 'POST', '/api/spaces', { name });
    if ((created?.name ?? created?.space) !== name) {
      fail(`POST /api/spaces did not confirm space ${name}`);
    }
  }

  // CLI supports brief update JSON (PATCH /api/brief); prior audit was wrong.
  // Core counts the summary as an applied mutation (BriefAppliedMutation kind
  // "summary"), so a full receipt is summary + 2 adds with zero conflicts.
  const receipt = await call(fetchImpl, origin, 'PATCH', '/api/brief', {
    space: SPACE,
    caller_id: AGENT,
    operation_id: 'reviewer-seed',
    summary: { text: 'Atlas uses signed requests.', expected_version: 0 },
    mutations: [
      { kind: 'add', text: 'Use signed requests', state: 'active' },
      { kind: 'add', text: 'Document offline fallback', state: 'backlog' },
    ],
  });
  const kinds = Array.isArray(receipt?.applied) ? receipt.applied.map((a) => a?.kind) : null;
  if (kinds === null || kinds.length !== 3 || kinds[0] !== 'summary' || kinds[1] !== 'add' || kinds[2] !== 'add') {
    fail('brief update did not fully apply', `applied kinds: ${JSON.stringify(kinds)}`);
  }
  if (!Array.isArray(receipt.conflicts) || receipt.conflicts.length !== 0) {
    fail('brief update reported conflicts', JSON.stringify(receipt.conflicts)?.slice(0, 300));
  }

  const generatedIds = new Set();
  const warnings = [];
  function uniqueId(value, label) {
    const id = checkId(value, label);
    if (generatedIds.has(id)) fail(`duplicate generated ID for ${label}`);
    generatedIds.add(id);
    return id;
  }

  async function storeMemory({ content, title, space }) {
    const res = await call(fetchImpl, origin, 'POST', '/api/memory/store', {
      content,
      title,
      memory_type: 'decision',
      space,
      source_agent: AGENT,
    });
    if (res?.gated || res?.queued) fail(`memory store for ${title} was staged, failing closed`);
    if (res?.space !== space) fail(`memory store for ${title} landed in wrong space`, String(res?.space));
    if (res?.write_outcome !== 'created') fail(`memory store for ${title} outcome was not created`, String(res?.write_outcome));
    // Near-duplicate is advisory: the normal API stores the distinct record.
    // Still require a created outcome and unique ID, and retain the warning.
    if (res.near_duplicate) warnings.push({ title, kind: 'near_duplicate' });
    return uniqueId(res?.source_id, title);
  }

  const memAuth = await storeMemory({
    content: 'Atlas authentication decision: use signed requests to authenticate relay traffic.',
    title: 'Atlas authentication decision',
    space: SPACE,
  });
  const memUnavailable = await storeMemory({
    content: 'Historical project evidence, moved outside the shared Space.',
    title: 'Historical evidence',
    space: SPACE,
  });
  const memSentinel = await storeMemory({
    content: 'Atlas authentication decision signed requests UNAUTHORIZED_SENTINEL',
    title: 'Atlas authentication decision',
    space: OTHER,
  });

  async function createPage({ title, content, space, source }) {
    const res = await call(fetchImpl, origin, 'POST', '/api/pages', {
      title,
      content,
      space,
      source_memory_ids: [source],
      creation_kind: 'authored',
      workspace: space,
    });
    if (res?.attached_to) fail(`page ${title} attached to existing content, failing closed`);
    if (res?.gated || res?.queued) fail(`page ${title} was staged, failing closed`);
    if (res?.space !== space) fail(`page ${title} landed in wrong space`, String(res?.space));
    if (res?.write_outcome !== 'created') fail(`page ${title} write outcome was not created`, String(res?.write_outcome));
    return uniqueId(res?.id, title);
  }

  const pageAuth = await createPage({
    title: 'Atlas authentication',
    content: 'Atlas uses signed requests to authenticate relay traffic.',
    space: SPACE,
    source: memAuth,
  });
  const pageUnavailable = await createPage({
    title: 'Historical evidence index',
    content: 'Historical evidence is no longer available in this shared library.',
    space: SPACE,
    source: memUnavailable,
  });
  const pageSentinel = await createPage({
    title: 'Private authentication',
    content: 'UNAUTHORIZED_SENTINEL private evidence.',
    space: OTHER,
    source: memSentinel,
  });

  // Move the unavailable-evidence memory out of the shared Space.
  // Existing endpoint: POST /api/documents/{source_id}/space.
  const moved = await call(fetchImpl, origin, 'POST', `/api/documents/${encodeURIComponent(memUnavailable)}/space`, {
    space_name: OTHER,
  });
  if (moved?.ok !== true) fail('memory move did not confirm ok:true');

  // Source readback: re-read both atlas-review pages through the scoped
  // source endpoint. The endpoint returns raw link entries; unavailable
  // links are present with memory:null, so count available memories
  // (entries with non-null memory), not all links.
  async function readSources(pageId, label) {
    const body = await call(fetchImpl, origin, 'GET', `/api/pages/${encodeURIComponent(pageId)}/sources`, undefined, {
      space: SPACE,
    });
    const list = Array.isArray(body) ? body : body?.sources;
    if (!Array.isArray(list)) fail(`source readback for ${label} returned malformed body`);
    for (const entry of list) {
      if (entry === null || typeof entry !== 'object' || Array.isArray(entry)) {
        fail(`source readback for ${label} returned malformed entry`);
      }
      if (!Object.hasOwn(entry, 'memory') || (entry.memory !== null && (
        typeof entry.memory !== 'object' || Array.isArray(entry.memory)
        || typeof entry.memory.source_id !== 'string' || !entry.memory.source_id.trim()
      ))) {
        fail(`source readback for ${label} returned malformed memory`);
      }
    }
    return list;
  }

  const authSources = await readSources(pageAuth, 'Atlas authentication');
  const authAvailable = authSources.filter((entry) => entry.memory !== null && entry.memory !== undefined);
  if (authAvailable.length !== 1) fail('auth page source readback must have exactly one available memory');
  if (authAvailable[0].memory?.source_id !== memAuth) {
    fail('auth page source readback has wrong source');
  }

  const unavailableSources = await readSources(pageUnavailable, 'Historical evidence index');
  const unavailableAvailable = unavailableSources.filter((entry) => entry.memory !== null && entry.memory !== undefined);
  if (unavailableSources.length !== 1) fail('unavailable page source readback must retain one source link');
  if (unavailableAvailable.length !== 0) fail('unavailable page source readback must have zero available memories');

  return {
    sourceChecks: {
      authAvailable: 1,
      unavailableLinks: 1,
      unavailableAvailable: 0,
    },
    mapping: {
      'mem_atlas-auth': memAuth,
      'mem_atlas-unavailable': memUnavailable,
      'mem_private-sentinel': memSentinel,
      'page_atlas-auth': pageAuth,
      'page_atlas-unavailable': pageUnavailable,
      'page_private-sentinel': pageSentinel,
    },
    // No confirmation flags are changed. This is not a claim that review is
    // required before querying: exercise the real client to establish that.
    pageReviewNotModified: [pageAuth, pageUnavailable, pageSentinel],
    warnings,
  };
}
