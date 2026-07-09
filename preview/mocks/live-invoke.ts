// SPDX-License-Identifier: AGPL-3.0-only
// Live browser backend: maps Tauri invoke() commands to the daemon's HTTP API
// (proxied at /daemon → http://127.0.0.1:7878), mirroring app/src/search.rs.
// Commands with no daemon route get app-local defaults; unknown ones warn.

type Args = Record<string, unknown> | undefined;

class HttpError extends Error {
  status: number;
  constructor(status: number, msg: string) {
    super(msg);
    this.status = status;
  }
}

async function http(method: string, path: string, body?: unknown): Promise<any> {
  const res = await fetch(`/daemon${path}`, {
    method,
    headers: body !== undefined ? { "content-type": "application/json" } : undefined,
    body: body !== undefined ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) throw new HttpError(res.status, `${method} ${path} → ${res.status}`);
  const text = await res.text();
  return text ? JSON.parse(text) : null;
}
const get = (p: string) => http("GET", p);
const post = (p: string, b: unknown = {}) => http("POST", p, b);
const put = (p: string, b: unknown = {}) => http("PUT", p, b);

const enc = encodeURIComponent;
const qs = (obj: Record<string, unknown>) => {
  const parts = Object.entries(obj)
    .filter(([, v]) => v !== undefined && v !== null)
    .map(([k, v]) => `${k}=${enc(String(v))}`);
  return parts.length ? `?${parts.join("&")}` : "";
};

const HANDLERS: Record<string, (a: any) => Promise<unknown>> = {
  // --- pages (mirrors search.rs exactly) ---
  get_page: async (a) => {
    try {
      const wire = await get(`/api/pages/${enc(a.id)}`);
      return wire?.page ?? null;
    } catch (e) {
      if (e instanceof HttpError && e.status === 404) return null;
      throw e;
    }
  },
  list_pages: (a) =>
    get(
      `/api/pages${qs({ status: a?.status, domain: a?.domain, limit: a?.limit, offset: a?.offset })}`,
    ).then((r) => r.pages ?? r),
  search_pages: (a) =>
    post("/api/pages/search", { query: a.query, limit: a.limit ?? null, page_type: null }).then(
      (r) => r.pages ?? r,
    ),
  list_recent_pages: (a) =>
    get(`/api/pages/recent${qs({ limit: a?.limit, since_ms: a?.sinceMs })}`).then(
      (r) => r.pages ?? r,
    ),
  list_recent_changes: (a) =>
    get(`/api/pages/recent-changes${qs({ limit: a?.limit })}`).then((r) => r.changes ?? r),
  get_page_sources: (a) => get(`/api/pages/${enc(a.pageId)}/sources`).then((r) => r?.sources ?? r),
  get_page_links: (a) => get(`/api/pages/${enc(a.pageId)}/links`),
  get_page_revisions: (a) => get(`/api/pages/${enc(a.pageId)}/revisions`),
  redistill_page: (a) => post(`/api/distill/${enc(a.pageId)}`, {}),
  update_page: (a) => post(`/api/memory/${enc(a.id)}/update-page`, { content: a.content }),
  delete_page: (a) => post(`/api/pages/${enc(a.id)}/archive`),
  archive_page: (a) => post(`/api/pages/${enc(a.id)}/archive`),
  list_orphan_links: (a) => get(`/api/pages/orphan-links${qs({ min_count: a?.minCount })}`),

  // --- memories ---
  list_recent_memories: (a) =>
    get(`/api/memory/recent${qs({ limit: a?.limit, since_ms: a?.sinceMs })}`).then(
      (r) => r.memories ?? r,
    ),
  get_memory_detail: (a) =>
    get(`/api/memory/${enc(a.sourceId)}/detail`).then((r) => r?.memory ?? null),
  list_memories_by_ids: (a) =>
    get(`/api/memory/by-ids?ids=${(a.ids as string[]).map(enc).join(",")}`).then(
      (r) => r.memories ?? r,
    ),
  list_memories_cmd: (a) =>
    post("/api/memory/list", {
      memory_type: a?.memoryType ?? null,
      space: a?.domain ?? null,
      limit: a?.limit ?? 200,
      confirmed: a?.confirmed ?? null,
    }).then((r) =>
      (r.memories ?? []).map((info: any) => ({
        supersedes: null,
        entity_id: null,
        quality: null,
        is_recap: String(info.source_id ?? "").startsWith("recap_"),
        enrichment_status: "raw",
        supersede_mode: "hide",
        structured_fields: null,
        retrieval_cue: null,
        source_text: null,
        access_count: 0,
        version: 1,
        changelog: null,
        pending_revision: false,
        merged_from: null,
        ...info,
        confirmed: info.confirmed ?? false,
      })),
    ),
  list_pinned_memories: () => get("/api/memory/pinned").then((r) => r.memories ?? r),
  list_unconfirmed_memories: (a) =>
    get(`/api/memory/unconfirmed${qs({ limit: a?.limit ?? 50 })}`).then((r) => r.memories ?? r),
  get_memory_stats_cmd: () => get("/api/memory/stats"),
  get_memory_revisions: (a) => get(`/api/memory/${enc(a.sourceId)}/revisions`),
  get_version_chain_cmd: (a) => get(`/api/memory/${enc(a.sourceId)}/versions`),
  search: (a) =>
    post("/api/search", { query: a.query, limit: a?.limit ?? 20 }).then((r) => r.results ?? []),
  search_memory: (a) =>
    post("/api/memory/search", { query: a.query, limit: a?.limit ?? 20 }).then(
      (r) => r.results ?? r.memories ?? [],
    ),

  // --- entities / knowledge ---
  list_entities_cmd: (a) =>
    post("/api/memory/entities/list", { limit: a?.limit ?? 100 }).then((r) => r.entities ?? r),
  search_entities_cmd: (a) =>
    post("/api/memory/entities/search", { query: a.query }).then((r) => r.entities ?? r),
  get_entity_detail_cmd: (a) => get(`/api/memory/entities/${enc(a.entityId)}`),
  get_entity_suggestions_cmd: () => get("/api/memory/entity-suggestions"),
  list_recent_relations: (a) =>
    get(`/api/knowledge/recent-relations${qs({ limit: a?.limit ?? 50 })}`).then(
      (r) => r.relations ?? r,
    ),
  count_knowledge_files: () => get("/api/knowledge/count").then((r) => r.total ?? r.count ?? 0),

  // --- home / profile / spaces / misc ---
  get_home_stats: () => get("/api/home-stats"),
  get_briefing: () => get("/api/briefing"),
  get_profile: () => get("/api/profile"),
  get_profile_narrative: () => get("/api/profile/narrative"),
  list_spaces: () => get("/api/spaces"),
  get_capture_stats: () => get("/api/capture-stats").then((r) => r.stats ?? r),
  // TagData shape: the UI reads both r.tags and r.document_tags — no unwrap.
  list_all_tags: () => get("/api/tags"),
  list_agents: () => get("/api/agents"),
  list_agent_activity: (a) => get(`/api/activities${qs({ limit: a?.limit ?? 50 })}`),
  list_activities: (a) => get(`/api/activities${qs({ limit: a?.limit ?? 50 })}`),
  list_recent_retrievals: (a) => get(`/api/retrievals/recent${qs({ limit: a?.limit ?? 20 })}`),
  list_pending_revisions: (a) =>
    get(`/api/memory/pending-revisions${qs({ limit: a?.limit })}`).then((r) => r.revisions ?? r),
  get_pending_contradictions: () =>
    get("/api/memory/contradictions").then((r) => r.contradictions ?? r),
  get_nurture_cards_cmd: () => get("/api/memory/nurture").then((r) => r.cards ?? r),
  list_refinements: () => get("/api/refinery/queue"),
  list_decisions_cmd: () => get("/api/decisions?limit=200").then((r) => r.decisions ?? r),
  list_decision_domains_cmd: () => get("/api/decisions/domains").then((r) => r.domains ?? r),
  get_working_memory: () => get("/api/memory/working").then((r) => r ?? null),
  pin_memory: (a) => post(`/api/memory/${enc(a.sourceId)}/pin`),
  unpin_memory: (a) => post(`/api/memory/${enc(a.sourceId)}/unpin`),
  confirm_memory: (a) => post(`/api/memory/confirm/${enc(a.sourceId)}`, { confirmed: true }),
  update_memory_cmd: (a) => put(`/api/memory/${enc(a.sourceId)}/update`, { content: a.content }),
  get_pipeline_status: () => get("/api/debug/pipeline"),
  list_onboarding_milestones: () =>
    get("/api/onboarding/milestones").then((r) => r.milestones ?? r),
};

// App-local commands (no daemon route) → static defaults that route the UI
// to the main screen and render panels empty rather than crashing.
const DEFAULTS: Record<string, unknown> = {
  should_show_wizard: false,
  get_setup_completed: true,
  get_setup_status: { completed: true },
  set_traffic_lights_visible: null,
  set_setup_completed: null,
  get_clipboard_enabled: false,
  get_screen_capture_enabled: false,
  check_screen_permission: true,
  get_private_browsing_detection: false,
  is_run_at_login_enabled: false,
  list_watch_paths: [],
  list_sources: [],
  list_registered_sources: [],
  list_indexed_files: [],
  list_pending_imports: [],
  get_index_status: {
    indexing: false,
    total_chunks: 0,
    watched_paths: 0,
    last_indexed: null,
  },
  get_api_key: null,
  get_model_choice: [null, null],
  get_external_llm: [null, null],
  get_on_device_model: null,
  get_system_info: null,
  get_remote_access_status: null,
  get_wenlan_mcp_entry: null,
  detect_mcp_clients_cmd: [],
  get_avatar_data_url: null,
  get_knowledge_path: null,
  get_session_snapshots: [],
  get_skip_apps: [],
  get_skip_title_patterns: [],
  suggest_tags: [],
  quit_wenlan_full: null,
  quit_origin_full: null,
};

export async function liveInvoke(cmd: string, args?: Args): Promise<unknown> {
  const handler = HANDLERS[cmd];
  if (handler) {
    try {
      return await handler(args);
    } catch (e) {
      console.warn(`[preview:live] ${cmd} failed:`, e);
      throw e;
    }
  }
  if (cmd in DEFAULTS) return DEFAULTS[cmd];
  console.warn(`[preview:live] unmapped invoke: ${cmd}`, args);
  return null;
}
