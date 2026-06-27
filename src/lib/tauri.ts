// SPDX-License-Identifier: AGPL-3.0-only
import { invoke } from "@tauri-apps/api/core";

export async function setTrafficLightsVisible(visible: boolean): Promise<void> {
  return invoke("set_traffic_lights_visible", { visible });
}

export interface SearchResult {
  id: string;
  content: string;
  source: string;
  source_id: string;
  title: string;
  url: string | null;
  chunk_index: number;
  last_modified: number;
  score: number;
  chunk_type?: string;
  language?: string;
  semantic_unit?: string;
  memory_type?: string;
  domain?: string;
  space?: string | null;
  source_agent?: string;
  confidence?: number;
  confirmed?: boolean;
  entity_id?: string;
  entity_name?: string;
  quality?: string;
  is_archived?: boolean;
  is_recap?: boolean;
  structured_fields?: string | null;  // JSON string from Rust
  retrieval_cue?: string | null;
}

export interface IndexStatus {
  is_running: boolean;
  files_indexed: number;
  files_total: number;
  last_error: string | null;
  sources_connected: string[];
  reranker: RerankerStatus;
  reranker_light: RerankerStatus;
  reranker_mode: string;
}

export type RerankerStatus =
  | { state: "disabled" }
  | { state: "active"; model_id: string }
  | { state: "failed"; reason: string };

export interface SourceStatus {
  name: string;
  connected: boolean;
  requires_auth: boolean;
  last_sync: number | null;
  document_count: number;
  error: string | null;
}

type DomainCompat = { domain?: string | null; space?: string | null };

function withDomain<T extends DomainCompat>(item: T): T {
  if (item.domain !== undefined || item.space === undefined) return item;
  return { ...item, domain: item.space };
}

function withDomainArray<T extends DomainCompat>(items: T[]): T[] {
  return items.map(withDomain);
}

export async function search(
  query: string,
  limit?: number,
  sourceFilter?: string,
): Promise<SearchResult[]> {
  const results = await invoke<SearchResult[]>("search", {
    query,
    limit: limit ?? 10,
    sourceFilter: sourceFilter ?? null,
  });
  return withDomainArray(results);
}

export async function getIndexStatus(): Promise<IndexStatus> {
  return invoke("get_index_status");
}

export async function addWatchPath(path: string): Promise<void> {
  return invoke("add_watch_path", { path });
}

export async function removeWatchPath(path: string): Promise<void> {
  return invoke("remove_watch_path", { path });
}

export async function reindex(): Promise<void> {
  return invoke("reindex");
}

export async function listWatchPaths(): Promise<string[]> {
  return invoke("list_watch_paths");
}

export async function connectSource(sourceName: string): Promise<void> {
  return invoke("connect_source", { sourceName });
}

export async function disconnectSource(sourceName: string): Promise<void> {
  return invoke("disconnect_source", { sourceName });
}

export async function syncSource(sourceName: string): Promise<void> {
  return invoke("sync_source", { sourceName });
}

export async function listSources(): Promise<SourceStatus[]> {
  return invoke("list_sources");
}

// ===== Registered Sources =====

export type SourceTypeStr = "obsidian" | "directory";
export type SyncStatusStr = "Active" | "Paused" | { Error: string };

/**
 * Categorized sync error detail string. The Rust side sends this as
 * `Option<String>`, so any string is possible on the wire. The frontend
 * treats these as opaque keys and matches known values for targeted
 * messaging, falling through to a generic message otherwise.
 *
 * Known values (as of this writing): "google_drive_offline", "file_read_errors".
 */
export type SyncErrorDetail = string;

export interface RegisteredSource {
  id: string;
  source_type: SourceTypeStr;
  path: string;
  status: SyncStatusStr;
  last_sync: number | null;
  file_count: number;
  memory_count: number;
  last_sync_errors?: number;
  last_sync_error_detail?: SyncErrorDetail | null;
}

export interface SyncStats {
  files_found: number;
  ingested: number;
  skipped: number;
  errors: number;
  error_detail?: SyncErrorDetail | null;
}

export async function listRegisteredSources(): Promise<RegisteredSource[]> {
  return invoke("list_registered_sources");
}

export async function addSource(
  sourceType: SourceTypeStr,
  path: string,
): Promise<RegisteredSource> {
  return invoke("add_source", { sourceType, path });
}

export async function removeSource(id: string): Promise<void> {
  return invoke("remove_source", { id });
}

export async function syncRegisteredSource(id: string): Promise<SyncStats> {
  return invoke("sync_registered_source", { id });
}

export interface IndexedFileInfo {
  source: string;
  source_id: string;
  title: string;
  summary?: string;
  chunk_count: number;
  last_modified: number;
  processing?: boolean;
  memory_type?: string | null;
  domain?: string | null;
  space?: string | null;
  source_agent?: string | null;
  confidence?: number | null;
  confirmed?: boolean | null;
  pinned?: boolean;
}

export async function listIndexedFiles(): Promise<IndexedFileInfo[]> {
  const files = await invoke<IndexedFileInfo[]>("list_indexed_files");
  return withDomainArray(files);
}

export async function deleteFileChunks(source: string, sourceId: string): Promise<void> {
  return invoke("delete_file_chunks", { source, sourceId });
}

export async function deleteByTimeRange(start: number, end: number): Promise<void> {
  return invoke("delete_by_time_range", { start, end });
}

export async function deleteBulk(items: { source: string; sourceId: string }[]): Promise<void> {
  return invoke("delete_bulk", {
    items: items.map((i) => ({ source: i.source, source_id: i.sourceId })),
  });
}

export async function openFile(url: string): Promise<void> {
  const path = url.startsWith("file://") ? url.slice(7) : url;
  return invoke("open_file", { path });
}

export interface ChunkDetail {
  id: string;
  content: string;
  chunk_index: number;
  chunk_type: string | null;
  language: string | null;
}

export async function getChunks(source: string, sourceId: string): Promise<ChunkDetail[]> {
  return invoke("get_chunks", { source, sourceId });
}

export async function updateChunk(id: string, content: string): Promise<void> {
  return invoke("update_chunk", { id, content });
}

export interface QuickCaptureRequest {
  title?: string;
  content: string;
  tags?: string[];
  domain?: string;
}

export async function quickCapture(req: QuickCaptureRequest): Promise<number> {
  return invoke("quick_capture", { req });
}

// Skip flag to prevent re-ingesting content we just wrote to the clipboard
let _skipNextClipboardChange = false;

export function shouldSkipClipboardChange(): boolean {
  if (_skipNextClipboardChange) {
    _skipNextClipboardChange = false;
    return true;
  }
  return false;
}

export async function clipboardWrite(text: string): Promise<void> {
  const { writeText } = await import("tauri-plugin-clipboard-x-api");
  _skipNextClipboardChange = true;
  await writeText(text);
}

export async function ingestClipboard(content: string): Promise<number> {
  return invoke("ingest_clipboard", { content });
}

export async function getClipboardEnabled(): Promise<boolean> {
  return invoke("get_clipboard_enabled");
}

export async function setClipboardEnabled(enabled: boolean): Promise<void> {
  return invoke("set_clipboard_enabled", { enabled });
}

export async function getApiKey(): Promise<string | null> {
  return invoke("get_api_key");
}

export async function setApiKey(key: string): Promise<void> {
  return invoke("set_api_key", { key });
}

export async function getModelChoice(): Promise<[string | null, string | null]> {
  return invoke("get_model_choice");
}

// The daemon config endpoint is patch-based: null preserves the current value.
export async function setModelChoice(
  routineModel: string | null,
  synthesisModel: string | null
): Promise<void> {
  return invoke("set_model_choice", { routineModel, synthesisModel });
}

export interface SystemInfo {
  total_ram_gb: number;
  available_ram_gb: number;
  has_metal: boolean;
  has_cuda: boolean;
  os: string;
  arch: string;
  recommended_builtin: string;
}

export async function getSystemInfo(): Promise<SystemInfo> {
  return invoke("get_system_info");
}

export async function getExternalLlm(): Promise<[string | null, string | null]> {
  return invoke("get_external_llm");
}

// The daemon config endpoint is patch-based: null preserves the current value.
export async function setExternalLlm(
  endpoint: string | null,
  model: string | null
): Promise<void> {
  return invoke("set_external_llm", { endpoint, model });
}

export async function testExternalLlm(
  endpoint: string,
  model: string
): Promise<string> {
  return invoke("test_external_llm", { endpoint, model });
}

export interface OnDeviceModelEntry {
  id: string;
  display_name: string;
  param_count: string;
  ram_required_gb: number;
  file_size_gb: number;
  cached: boolean;
}

export interface OnDeviceModelResponse {
  /// ID of the model currently loaded in the daemon (if any).
  loaded: string | null;
  /// ID the user has selected in config (may differ from loaded).
  selected: string | null;
  /// All available models with per-model cache state.
  models: OnDeviceModelEntry[];
}

export async function getOnDeviceModel(): Promise<OnDeviceModelResponse> {
  return invoke("get_on_device_model");
}

/// Triggers download + hot-load. Long-running (minutes for 2.7GB).
export async function downloadOnDeviceModel(modelId: string): Promise<void> {
  return invoke("download_on_device_model", { modelId });
}

export interface ActivitySummary {
  id: string;
  started_at: number;
  ended_at: number;
  is_live: boolean;
  app_names: string[];
}

export async function listActivities(): Promise<ActivitySummary[]> {
  return invoke("list_activities");
}

export async function rebuildActivities(): Promise<number> {
  return invoke("rebuild_activities");
}

export interface WorkingMemoryEntry {
  timestamp: number;
  source: string;
  app_name: string;
  window_title: string;
  text_snippet: string;
  source_id: string;
}

export async function getWorkingMemory(): Promise<WorkingMemoryEntry[]> {
  return invoke("get_working_memory");
}

export type CaptureStats = Record<string, number>;

export async function getCaptureStats(): Promise<CaptureStats> {
  return invoke("get_capture_stats");
}

// ── Tags ────────────────────────────────────────────────────────────────

export interface TagData {
  tags: string[];
  document_tags: Record<string, string[]>;
  categories: string[];
  document_categories: Record<string, string>;
}

// ── Spaces ──────────────────────────────────────────────────────────────

export interface Space {
  id: string;
  name: string;
  description: string | null;
  suggested: boolean;
  starred: boolean;
  sort_order: number;
  memory_count: number;
  entity_count: number;
  created_at: number;
  updated_at: number;
}

// Legacy types — kept for MemoryView.tsx backwards compat
export interface SpaceRule {
  kind: "app" | "path" | "keyword" | "url_pattern";
  pattern: string;
}

export interface LegacySpace {
  id: string;
  name: string;
  icon: string;
  color: string;
  rules: SpaceRule[];
  pinned: boolean;
  auto_detected: boolean;
  created_at: number;
}

export interface ActivityStream {
  id: string;
  space_id: string;
  name: string;
  started_at: number;
  ended_at: number | null;
  app_sequence: string[];
}

export interface SpaceData {
  spaces: LegacySpace[];
  activity_streams: ActivityStream[];
  document_spaces: Record<string, string>;
  document_tags: Record<string, string[]>;
  tags: string[];
}

export async function listAllTags(): Promise<TagData> {
  return invoke("list_all_tags");
}

export async function setDocumentTags(
  source: string,
  sourceId: string,
  tags: string[],
): Promise<string[]> {
  return invoke("set_document_tags", { source, sourceId, tags });
}

export async function deleteTag(name: string): Promise<void> {
  return invoke("delete_tag", { name });
}

export async function suggestTags(
  source: string,
  sourceId: string,
  lastModified: number,
): Promise<string[]> {
  return invoke("suggest_tags", { source, sourceId, lastModified });
}

export async function listSpaces(): Promise<Space[]> {
  return invoke("list_spaces");
}

export async function getSpace(name: string): Promise<Space | null> {
  return invoke("get_space", { name });
}

export async function createSpace(
  name: string,
  description?: string,
): Promise<Space> {
  return invoke("create_space", { name, description: description ?? null });
}

export async function updateSpace(
  name: string,
  newName: string,
  description?: string,
): Promise<Space> {
  return invoke("update_space", { name, newName, description: description ?? null });
}

export async function deleteSpace(name: string, memoryAction?: string): Promise<void> {
  return invoke("delete_space", { name, memoryAction: memoryAction ?? null });
}

export async function confirmSpace(name: string): Promise<void> {
  return invoke("confirm_space", { name });
}

export async function reorderSpace(name: string, newOrder: number): Promise<void> {
  return invoke("reorder_space", { name, newOrder });
}

export async function toggleSpaceStarred(name: string): Promise<boolean> {
  return invoke("toggle_space_starred", { name });
}

export async function setDocumentSpace(
  source: string,
  sourceId: string,
  spaceId: string,
): Promise<void> {
  return invoke("set_document_space", { source, sourceId, spaceId });
}

export async function addLegacySpace(
  name: string,
  icon: string,
  color: string,
): Promise<void> {
  return invoke("add_space", { name, icon, color });
}

export async function removeLegacySpace(spaceId: string): Promise<void> {
  return invoke("remove_space", { spaceId });
}

export async function renameLegacySpace(
  spaceId: string,
  newName: string,
): Promise<void> {
  return invoke("rename_space", { spaceId, newName });
}

export async function pinLegacySpace(spaceId: string): Promise<void> {
  return invoke("pin_space", { spaceId });
}

// ── Session Snapshots ───────────────────────────────────────────────────

export interface SessionSnapshot {
  id: string;
  activity_id: string;
  started_at: number;
  ended_at: number;
  primary_apps: string[];
  summary: string;
  tags: string[];
  capture_count: number;
}

export async function getSessionSnapshots(limit?: number): Promise<SessionSnapshot[]> {
  return invoke("get_session_snapshots", { limit: limit ?? 10 });
}

export interface SnapshotCapture {
  source_id: string;
  app_name: string;
  window_title: string;
  timestamp: number;
  source: string;
}

export async function getSnapshotCaptures(snapshotId: string): Promise<SnapshotCapture[]> {
  return invoke("get_snapshot_captures", { snapshotId });
}

export interface SnapshotCaptureWithContent {
  source_id: string;
  app_name: string;
  window_title: string;
  timestamp: number;
  source: string;
  content: string;
  summary: string | null;
}

export async function getSnapshotCapturesWithContent(snapshotId: string): Promise<SnapshotCaptureWithContent[]> {
  return invoke("get_snapshot_captures_with_content", { snapshotId });
}

export async function deleteSnapshot(snapshotId: string): Promise<void> {
  return invoke("delete_snapshot", { snapshotId });
}

// ── Capture Quality ─────────────────────────────────────────────────────

export async function getSkipApps(): Promise<string[]> {
  return invoke("get_skip_apps");
}

export async function setSkipApps(apps: string[]): Promise<void> {
  return invoke("set_skip_apps", { apps });
}

export async function getSkipTitlePatterns(): Promise<string[]> {
  return invoke("get_skip_title_patterns");
}

export async function setSkipTitlePatterns(patterns: string[]): Promise<void> {
  return invoke("set_skip_title_patterns", { patterns });
}

export async function getPrivateBrowsingDetection(): Promise<boolean> {
  return invoke("get_private_browsing_detection");
}

export async function setPrivateBrowsingDetection(enabled: boolean): Promise<void> {
  return invoke("set_private_browsing_detection", { enabled });
}

// ── Memory Facets ───────────────────────────────────────────────────

export type MemoryType = 'identity' | 'preference' | 'decision' | 'lesson' | 'gotcha' | 'fact';

export interface MemoryFacet {
  type: MemoryType;
  label: string;
  color: string;
  description: string;
}

export const MEMORY_FACETS: MemoryFacet[] = [
  { type: 'identity', label: 'Identity', color: 'indigo', description: 'Who you are, always injected into context' },
  { type: 'preference', label: 'Preference', color: 'orange', description: 'How you like things done, always injected' },
  { type: 'decision', label: 'Decision', color: 'amber', description: 'Choices made with rationale and tradeoffs' },
  { type: 'lesson', label: 'Lesson', color: 'emerald', description: 'Positive learnings to reuse later' },
  { type: 'gotcha', label: 'Gotcha', color: 'red', description: 'Traps, failures, and warnings to avoid' },
  { type: 'fact', label: 'Fact', color: 'zinc', description: 'Durable knowledge, standard hybrid search' },
];

// Canonical memory-type colors aligned with Wenlan's --mem-accent-* palette:
//   identity  -> indigo  (--mem-accent-indigo: depth, foundational)
//   preference -> orange (--mem-accent-warm: personal, warm)
//   decision  -> amber   (--mem-accent-amber: weight, decisive)
//   lesson    -> emerald (--mem-accent-sage: reusable learning)
//   gotcha    -> red     (warning, avoid)
//   fact      -> zinc    (neutral, informational)
//   goal      -> emerald (legacy rows only)
export const FACET_COLORS: Record<string, string> = {
  identity: "bg-indigo-500/20 text-indigo-700 dark:text-indigo-400 border-indigo-500/30",
  preference: "bg-orange-500/20 text-orange-700 dark:text-orange-400 border-orange-500/30",
  decision: "bg-amber-500/20 text-amber-700 dark:text-amber-400 border-amber-500/30",
  lesson: "bg-emerald-500/20 text-emerald-700 dark:text-emerald-400 border-emerald-500/30",
  gotcha: "bg-red-500/20 text-red-700 dark:text-red-400 border-red-500/30",
  fact: "bg-zinc-500/20 text-zinc-700 dark:text-zinc-400 border-zinc-500/30",
  goal: "bg-emerald-500/20 text-emerald-700 dark:text-emerald-400 border-emerald-500/30",
};

export type StabilityTier = "protected" | "standard" | "ephemeral";

export const STABILITY_TIERS: Record<string, StabilityTier> = {
  identity: "protected",
  preference: "protected",
  fact: "standard",
  decision: "standard",
  lesson: "standard",
  gotcha: "standard",
  goal: "protected",
};

export interface PendingRevision {
  source_id: string;
  content: string;
  source_agent: string | null;
}

export interface PendingRevisionItem {
  target_source_id: string;
  revision_source_id: string;
  revision_content: string;
  source_agent: string | null;
  last_modified: number;
}

export interface RevisionAcceptResponse {
  target_source_id: string;
  revision_source_id: string;
  wrote: boolean;
}

export interface RevisionDismissResponse {
  target_source_id: string;
  wrote: boolean;
}

export interface ContradictionDismissResponse {
  source_id: string;
  wrote: boolean;
}

export interface MemoryVersionItem {
  source_id: string;
  title: string;
  content: string;
  memory_type: string | null;
  confirmed: boolean;
  supersedes: string | null;
  last_modified: number;
}

export interface MemoryRevisionEntry {
  source_id: string;
  depth: number;
  title: string;
  content_preview: string;
  last_modified: number;
  source_agent?: string | null;
  supersede_mode?: string | null;
  delta_summary?: string | null;
}

export interface ListMemoryRevisionsResponse {
  current_source_id: string;
  chain_depth: number;
  entries: MemoryRevisionEntry[];
}

export interface PageChangelogEntry {
  version: number;
  at: number;
  edited_by: string;
  delta_summary?: string | null;
  incoming_source_ids?: string[] | null;
}

export interface ListPageRevisionsResponse {
  page_id: string;
  current_version: number;
  user_edited: boolean;
  stale_reason?: string | null;
  entries: PageChangelogEntry[];
}

// ── Memory Page ─────────────────────────────────────────────────────

export interface Entity {
  id: string;
  name: string;
  entity_type: string;
  domain: string | null;
  space?: string | null;
  source_agent: string | null;
  confidence: number | null;
  confirmed: boolean;
  created_at: number;
  updated_at: number;
}

export interface Observation {
  id: string;
  entity_id: string;
  content: string;
  source_agent: string | null;
  confidence: number | null;
  confirmed: boolean;
  created_at: number;
}

export interface RelationWithEntity {
  id: string;
  relation_type: string;
  direction: string;
  entity_id: string;
  entity_name: string;
  entity_type: string;
  source_agent: string | null;
  created_at: number;
}

export interface EntityDetail {
  entity: Entity;
  observations: Observation[];
  relations: RelationWithEntity[];
}

export interface EntitySearchResult {
  entity: Entity;
  distance: number;
}

export interface MemoryItem {
  source_id: string;
  title: string;
  content: string;
  summary: string | null;
  source_text?: string | null;
  memory_type: string | null;
  domain: string | null;
  space?: string | null;
  source_agent: string | null;
  confidence: number | null;
  confirmed: boolean;
  stability?: "new" | "learned" | "confirmed";
  pinned: boolean;
  supersedes: string | null;
  last_modified: number;
  chunk_count: number;
  entity_id?: string | null;
  quality?: string | null;
  is_recap?: boolean;
  enrichment_status?: string;
  supersede_mode?: string;
  structured_fields?: string | null;  // JSON string from Rust
  retrieval_cue?: string | null;
  access_count?: number;
  version?: number;
  changelog?: Array<{
    version: number;
    at: number;
    delta: string;
    source_agent?: string | null;
    incoming_source_id?: string | null;
  }>;
}

export interface DomainInfo {
  name: string;
  count: number;
}

export interface TypeBreakdown {
  memory_type: string;
  count: number;
}

export interface MemoryStats {
  total: number;
  new_today: number;
  confirmed: number;
  domains: DomainInfo[];
  by_type?: TypeBreakdown[];
  entity_linked?: number;
  enrichment_pending?: number;
}

export interface EntitySuggestion {
  id: string;
  entity_name: string | null;
  source_ids: string[];
  confidence: number;
  created_at: string;
}

// ── Pages ───────────────────────────────────────────────────────────

export interface Page {
  id: string;
  title: string;
  summary: string | null;
  content: string;
  entity_id: string | null;
  domain: string | null;
  space?: string | null;
  source_memory_ids: string[];
  version: number;
  status: string;
  created_at: string;
  last_compiled: string;
  last_modified: string;
  // Staleness tracking (added in migration 40)
  sources_updated_count?: number;
  stale_reason?: string | null;
  user_edited?: boolean;
}

/** @deprecated Use {@link Page} instead. Kept for gradual migration. */
export type Concept = Page;

export interface PageSource {
  page_id: string;
  memory_source_id: string;
  linked_at: number;
  link_reason?: string | null;
}

/** @deprecated Use {@link PageSource} instead. Kept for gradual migration. */
export type ConceptSource = PageSource;

export interface PageSourceWithMemory {
  source: PageSource;
  memory: MemoryItem | null;
}

/** @deprecated Use {@link PageSourceWithMemory} instead. Kept for gradual migration. */
export type ConceptSourceWithMemory = PageSourceWithMemory;

export interface PageLinkOutbound {
  label: string;
  target_page_id: string | null;
}

export interface PageLinkInbound {
  source_page_id: string;
  label: string;
}

export interface PageLinksResponse {
  outbound: PageLinkOutbound[];
  inbound: PageLinkInbound[];
}

export interface OrphanLink {
  label: string;
  count: number;
}

export interface OrphanLinksResponse {
  min_count: number;
  orphan_labels: OrphanLink[];
}

export async function getPageSources(
  pageId: string,
): Promise<PageSourceWithMemory[]> {
  const sources = await invoke<PageSourceWithMemory[]>("get_page_sources", { pageId });
  return sources.map((source) => ({
    ...source,
    memory: source.memory ? withDomain(source.memory) : null,
  }));
}

/** @deprecated Use {@link getPageSources} instead. */
export async function getConceptSources(
  conceptId: string,
): Promise<PageSourceWithMemory[]> {
  return getPageSources(conceptId);
}

export async function getPageLinks(pageId: string): Promise<PageLinksResponse> {
  return invoke("get_page_links", { pageId });
}

export async function listOrphanLinks(minCount?: number): Promise<OrphanLinksResponse> {
  return invoke("list_orphan_links", { minCount: minCount ?? null });
}

// ── Profiles & Agent Connections ─────────────────────────────────────

export interface Profile {
  id: string;
  name: string;
  display_name: string | null;
  email: string | null;
  bio: string | null;
  avatar_path: string | null;
  created_at: number;
  updated_at: number;
}

export interface AgentConnection {
  id: string;
  /** Canonical technical ID (lowercase, hyphen-case). Matches `x-agent-name`. */
  name: string;
  /** Human-readable name shown in UI. Falls back to KNOWN_CLIENT_DISPLAY_NAMES → name. */
  display_name: string | null;
  agent_type: string;
  description: string | null;
  enabled: boolean;
  trust_level: string;
  last_seen_at: number | null;
  memory_count: number;
  created_at: number;
  updated_at: number;
}

// Knowledge Graph

export async function createEntity(
  name: string,
  entityType: string,
  domain?: string,
): Promise<string> {
  return invoke("create_entity_cmd", {
    name,
    entityType,
    domain: domain ?? null,
  });
}

export async function listEntities(
  entityType?: string,
  domain?: string,
): Promise<Entity[]> {
  const entities = await invoke<Entity[]>("list_entities_cmd", {
    entityType: entityType ?? null,
    domain: domain ?? null,
  });
  return withDomainArray(entities);
}

export async function searchEntities(
  query: string,
  limit?: number,
): Promise<EntitySearchResult[]> {
  const results = await invoke<EntitySearchResult[]>("search_entities_cmd", {
    query,
    limit: limit ?? null,
  });
  return results.map((result) => ({
    ...result,
    entity: withDomain(result.entity),
  }));
}

export async function getEntityDetail(entityId: string): Promise<EntityDetail> {
  const detail = await invoke<EntityDetail>("get_entity_detail_cmd", { entityId });
  return { ...detail, entity: withDomain(detail.entity) };
}

export async function updateObservation(observationId: string, content: string): Promise<void> {
  return invoke("update_observation_cmd", { observationId, content });
}

export async function deleteObservation(observationId: string): Promise<void> {
  return invoke("delete_observation_cmd", { observationId });
}

export async function addObservation(
  entityId: string,
  content: string,
  sourceAgent?: string,
  confidence?: number,
): Promise<string> {
  return invoke("add_observation_cmd", {
    entityId,
    content,
    sourceAgent: sourceAgent ?? null,
    confidence: confidence ?? null,
  });
}

export async function deleteEntity(entityId: string): Promise<void> {
  return invoke("delete_entity_cmd", { entityId });
}

export async function confirmEntity(entityId: string, confirmed: boolean): Promise<void> {
  return invoke("confirm_entity_cmd", { entityId, confirmed });
}

export async function confirmObservation(observationId: string, confirmed: boolean): Promise<void> {
  return invoke("confirm_observation_cmd", { observationId, confirmed });
}

// Store Memory

export interface StoreMemoryRequest {
  content: string;
  memory_type?: string;
  domain?: string;
  source_agent?: string;
  title?: string;
  tags?: string[];
  confidence?: number;
  supersedes?: string;
  structured_fields?: Record<string, string>;
  retrieval_cue?: string;
}

export interface StoreMemoryResponse {
  source_id: string;
  warnings?: string[];
  /**
   * Background-enrichment state. `"pending"` when the daemon will classify
   * and extract structured fields asynchronously; `"not_needed"` when no
   * LLM is available. Components that display the stored memory should
   * invalidate their query on this value to pick up enriched fields when
   * they land (target window is ~2s post-store).
   */
  enrichment?: "pending" | "not_needed" | string;
  /**
   * Prose nudge the caller agent can relay verbatim. Empty when enrichment
   * is not_needed.
   */
  hint?: string;
  triggered_revisions?: string[];
  auto_superseded?: string[];
}

export async function storeMemory(req: StoreMemoryRequest): Promise<StoreMemoryResponse> {
  return invoke("store_memory", { req });
}

// Memories

export async function listMemoriesRich(
  domain?: string,
  memoryType?: string,
  confirmed?: boolean,
  limit?: number,
): Promise<MemoryItem[]> {
  const memories = await invoke<MemoryItem[]>("list_memories_cmd", {
    domain: domain ?? null,
    memoryType: memoryType ?? null,
    confirmed: confirmed ?? null,
    limit: limit ?? null,
  });
  return withDomainArray(memories);
}

export async function getMemoryDetail(sourceId: string): Promise<MemoryItem | null> {
  const memory = await invoke<MemoryItem | null>("get_memory_detail", { sourceId });
  return memory ? withDomain(memory) : null;
}

export interface EnrichmentStepStatus {
  step: string;
  status: string;
  error?: string | null;
  attempts: number;
}

export interface EnrichmentStatusResponse {
  source_id: string;
  summary: string;
  steps: EnrichmentStepStatus[];
}

export async function getEnrichmentStatus(
  sourceId: string,
): Promise<EnrichmentStatusResponse> {
  return invoke("get_enrichment_status", { sourceId });
}

/** Batch-fetch multiple memories by source_id in one round trip. Missing ids are silently omitted. */
export async function listMemoriesByIds(ids: string[]): Promise<MemoryItem[]> {
  if (ids.length === 0) return [];
  const memories = await invoke<MemoryItem[]>("list_memories_by_ids", { ids });
  return withDomainArray(memories);
}

export async function getMemoryStats(): Promise<MemoryStats> {
  return invoke("get_memory_stats_cmd");
}

export interface TopMemory {
  source_id: string;
  content: string;
  memory_type: string | null;
  domain: string | null;
  times_retrieved: number;
}

export interface HomeStats {
  total: number;
  new_today: number;
  confirmed: number;
  total_ingested: number;
  active_insights: number;
  distilled_today: number;
  distilled_all: number;
  sources_archived: number;
  times_served_today: number;
  words_saved_today: number;
  times_served_week: number;
  words_saved_week: number;
  times_served_all: number;
  words_saved_all: number;
  corrections_active: number;
  top_memories: TopMemory[];
}

export async function getHomeStats(): Promise<HomeStats> {
  return invoke("get_home_stats");
}

export async function updateMemory(
  sourceId: string,
  content?: string,
  domain?: string,
  confirmed?: boolean,
  memoryType?: MemoryType,
): Promise<void> {
  return invoke("update_memory_cmd", {
    sourceId,
    content: content ?? null,
    domain: domain ?? null,
    confirmed: confirmed ?? null,
    memoryType: memoryType ?? null,
  });
}

export async function reclassifyMemory(
  sourceId: string,
  memoryType: MemoryType,
): Promise<string> {
  return invoke("reclassify_memory_cmd", { sourceId, memoryType });
}

export async function getVersionChain(
  sourceId: string,
): Promise<MemoryVersionItem[]> {
  return invoke("get_version_chain_cmd", { sourceId });
}

export async function getMemoryRevisions(
  sourceId: string,
): Promise<ListMemoryRevisionsResponse> {
  return invoke("get_memory_revisions", { sourceId });
}

export async function getPageRevisions(
  pageId: string,
): Promise<ListPageRevisionsResponse> {
  return invoke("get_page_revisions", { pageId });
}

export async function listPendingRevisions(limit?: number): Promise<PendingRevisionItem[]> {
  return invoke("list_pending_revisions", { limit: limit ?? null });
}

export async function acceptPendingRevision(sourceId: string): Promise<RevisionAcceptResponse> {
  return invoke("accept_pending_revision", { sourceId });
}

export async function dismissPendingRevision(sourceId: string): Promise<RevisionDismissResponse> {
  return invoke("dismiss_pending_revision", { sourceId });
}

export async function dismissContradiction(sourceId: string): Promise<ContradictionDismissResponse> {
  return invoke("dismiss_contradiction", { sourceId });
}

export async function confirmMemory(sourceId: string, confirmed: boolean = true): Promise<void> {
  return invoke("confirm_memory", { sourceId, confirmed });
}

export async function getPendingRevision(sourceId: string): Promise<PendingRevision | null> {
  return invoke("get_pending_revision", { sourceId });
}

// ===== Refinery Queue =====

export type ProposalAction =
  | "entity_merge"
  | "relation_conflict"
  | "detect_contradiction"
  | "suggest_entity"
  | "dedup_merge";

export type RefinementPayload =
  | {
      action: "entity_merge";
      existing_id: string;
      new_id: string;
      similarity: number;
    }
  | {
      action: "relation_conflict";
      existing_id: string;
      new_id: string;
      from: string;
      to: string;
      old_type: string;
      new_type: string;
    }
  | { action: "detect_contradiction" }
  | { action: "suggest_entity"; name_hint?: string | null }
  | { action: "dedup_merge" };

export interface RefinementProposalSummary {
  id: string;
  action: ProposalAction;
  source_ids: string[];
  payload?: RefinementPayload | null;
  confidence: number;
  created_at: string;
}

export interface ListRefinementsResponse {
  proposals: RefinementProposalSummary[];
}

export interface AcceptRefinementResponse {
  id: string;
  action_applied: string;
}

export interface RejectRefinementResponse {
  id: string;
}

export async function listRefinements(limit?: number): Promise<ListRefinementsResponse> {
  return invoke("list_refinements", { limit: limit ?? null });
}

export async function acceptRefinement(id: string): Promise<AcceptRefinementResponse> {
  return invoke("accept_refinement", { id });
}

export async function rejectRefinement(id: string): Promise<RejectRefinementResponse> {
  return invoke("reject_refinement", { id });
}

// ===== Entity Suggestions =====

export async function getEntitySuggestions(): Promise<EntitySuggestion[]> {
  return invoke("get_entity_suggestions_cmd");
}

export async function approveEntitySuggestion(id: string): Promise<{ entity_id: string; entity_name: string; memories_linked: number }> {
  return invoke("approve_entity_suggestion_cmd", { id });
}

export async function dismissEntitySuggestion(id: string): Promise<void> {
  return invoke("dismiss_entity_suggestion_cmd", { id });
}

// ===== Pages =====

export async function getPage(id: string): Promise<Page | null> {
  const page = await invoke<Page | null>("get_page", { id });
  return page ? withDomain(page) : null;
}

/** @deprecated Use {@link getPage} instead. */
export async function getConcept(id: string): Promise<Page | null> {
  return getPage(id);
}

export async function updatePage(id: string, content: string): Promise<void> {
  return invoke("update_page", { id, content });
}

/** @deprecated Use {@link updatePage} instead. */
export async function updateConcept(id: string, content: string): Promise<void> {
  return updatePage(id, content);
}

export async function deletePage(id: string): Promise<void> {
  return invoke("delete_page", { id });
}

/** @deprecated Use {@link deletePage} instead. */
export async function deleteConcept(id: string): Promise<void> {
  return deletePage(id);
}

export async function archivePage(id: string): Promise<void> {
  return invoke("archive_page", { id });
}

/** @deprecated Use {@link archivePage} instead. */
export async function archiveConcept(id: string): Promise<void> {
  return archivePage(id);
}

export async function searchPages(
  query: string,
  limit?: number,
): Promise<Page[]> {
  const pages = await invoke<Page[]>("search_pages", { query, limit: limit ?? 5 });
  return withDomainArray(pages);
}

/** @deprecated Use {@link searchPages} instead. */
export async function searchConcepts(
  query: string,
  limit?: number,
): Promise<Page[]> {
  return searchPages(query, limit);
}

export async function listPages(
  status?: string,
  domain?: string,
  limit?: number,
  offset?: number,
): Promise<Page[]> {
  const pages = await invoke<Page[]>("list_pages", { status, domain, limit, offset });
  return withDomainArray(pages);
}

/** @deprecated Use {@link listPages} instead. */
export async function listConcepts(
  status?: string,
  domain?: string,
  limit?: number,
  offset?: number,
): Promise<Page[]> {
  return listPages(status, domain, limit, offset);
}

// ── Home delta feed ────────────────────────────────────────────────────

export interface RetrievalEvent {
  timestamp_ms: number;
  agent_name: string;
  query?: string | null;
  page_titles: string[];
  /** Stable page IDs corresponding 1:1 with page_titles. Empty on legacy events. */
  page_ids: string[];
  memory_snippets: string[];
}

export type PageChangeKind = "created" | "revised" | "merged";

/** @deprecated Use {@link PageChangeKind} instead. */
export type ConceptChangeKind = PageChangeKind;

export interface PageChange {
  page_id: string;
  title: string;
  change_kind: PageChangeKind;
  changed_at_ms: number;
}

/** @deprecated Use {@link PageChange} instead. */
export type ConceptChange = PageChange;

export async function listRecentRetrievals(limit?: number): Promise<RetrievalEvent[]> {
  return invoke<RetrievalEvent[]>("list_recent_retrievals", { limit: limit ?? 10 });
}

export async function listRecentChanges(limit?: number): Promise<PageChange[]> {
  return invoke<PageChange[]>("list_recent_changes", { limit: limit ?? 10 });
}

export type ActivityKind = "concept" | "memory";

// Discriminated-union — matches Rust's internally-tagged serde enum.
export type ActivityBadge =
  | { kind: "new" }
  | { kind: "revised" }
  | { kind: "refined" }
  | { kind: "growing"; added: number }
  | { kind: "needs_review" }
  | { kind: "none" };

export interface RecentActivityItem {
  kind: ActivityKind;
  id: string;
  title: string;
  snippet: string | null;
  timestamp_ms: number;
  badge: ActivityBadge;
}

export async function listRecentMemories(
  limit: number,
  sinceMs?: number,
): Promise<RecentActivityItem[]> {
  return invoke<RecentActivityItem[]>("list_recent_memories", {
    limit,
    sinceMs: sinceMs ?? null,
  });
}

export async function listUnconfirmedMemories(
  limit: number = 6,
): Promise<RecentActivityItem[]> {
  return invoke<RecentActivityItem[]>("list_unconfirmed_memories", { limit });
}

export async function listRecentPages(
  limit: number,
  sinceMs?: number,
): Promise<RecentActivityItem[]> {
  return invoke<RecentActivityItem[]>("list_recent_pages", {
    limit,
    sinceMs: sinceMs ?? null,
  });
}

/** @deprecated Use {@link listRecentPages} instead. */
export async function listRecentConcepts(
  limit: number,
  sinceMs?: number,
): Promise<RecentActivityItem[]> {
  return listRecentPages(limit, sinceMs);
}

export interface RecentRelation {
  id: string;
  from_entity_id: string;
  relation_type: string;
  to_entity_id: string;
  from_entity_name: string;
  to_entity_name: string;
  created_at_ms: number;
}

export async function listRecentRelations(
  limit?: number,
  sinceMs?: number,
): Promise<RecentRelation[]> {
  return invoke<RecentRelation[]>("list_recent_relations", {
    limit: limit ?? 10,
    sinceMs: sinceMs ?? null,
  });
}

export async function exportPagesToObsidian(
  vaultPath: string,
): Promise<{ exported: number; skipped: number; failed: number }> {
  return invoke("export_pages_to_obsidian", { vaultPath });
}

/** @deprecated Use {@link exportPagesToObsidian} instead. */
export async function exportConceptsToObsidian(
  vaultPath: string,
): Promise<{ exported: number; skipped: number; failed: number }> {
  return exportPagesToObsidian(vaultPath);
}

export async function exportPageToObsidian(
  pageId: string,
  vaultPath: string,
): Promise<string> {
  return invoke("export_page_to_obsidian", { pageId, vaultPath });
}

/** @deprecated Use {@link exportPageToObsidian} instead. */
export async function exportConceptToObsidian(
  conceptId: string,
  vaultPath: string,
): Promise<string> {
  return exportPageToObsidian(conceptId, vaultPath);
}

// ===== Knowledge Directory =====

export async function getKnowledgePath(): Promise<string> {
  return invoke("get_knowledge_path");
}

export async function countKnowledgeFiles(): Promise<number> {
  return invoke("count_knowledge_files");
}

// ===== Profile =====

export async function getProfile(): Promise<Profile | null> {
  return invoke<Profile | null>("get_profile");
}

export async function updateProfile(
  id: string,
  name?: string,
  displayName?: string,
  email?: string,
  bio?: string,
  avatarPath?: string,
): Promise<void> {
  return invoke("update_profile", {
    id,
    name: name ?? null,
    display_name: displayName ?? null,
    email: email ?? null,
    bio: bio ?? null,
    avatar_path: avatarPath ?? null,
  });
}

// ===== Agents =====

export async function listAgents(): Promise<AgentConnection[]> {
  return invoke<AgentConnection[]>("list_agents");
}

export async function getAgent(name: string): Promise<AgentConnection | null> {
  return invoke<AgentConnection | null>("get_agent", { name });
}

export async function updateAgent(
  name: string,
  updates: {
    agentType?: string;
    description?: string;
    enabled?: boolean;
    trustLevel?: string;
    /** Pass empty string to clear the display_name. */
    displayName?: string;
  },
): Promise<void> {
  return invoke("update_agent", {
    name,
    agent_type: updates.agentType ?? null,
    description: updates.description ?? null,
    enabled: updates.enabled ?? null,
    trust_level: updates.trustLevel ?? null,
    display_name: updates.displayName ?? null,
  });
}

export async function deleteAgent(name: string): Promise<void> {
  return invoke("delete_agent", { name });
}

// ===== Pin / Unpin =====

export async function pinMemory(sourceId: string): Promise<void> {
  return invoke("pin_memory", { sourceId });
}

export async function unpinMemory(sourceId: string): Promise<void> {
  return invoke("unpin_memory", { sourceId });
}

export async function listPinnedMemories(): Promise<MemoryItem[]> {
  const memories = await invoke<MemoryItem[]>("list_pinned_memories");
  return withDomainArray(memories);
}

// ===== Import =====

export interface ImportResult {
  imported: number;
  skipped: number;
  breakdown: Record<string, number>;
  entities_created: number;
  observations_added: number;
  relations_created: number;
  batch_id: string;
}

export async function importMemories(
  source: string,
  content: string,
  label?: string,
): Promise<ImportResult> {
  return invoke("import_memories_cmd", {
    source,
    content,
    label: label ?? null,
  });
}

// ===== Chat Export Import =====

export interface ImportChatExportResponse {
  import_id: string;
  vendor: string;
  conversations_total: number;
  conversations_new: number;
  conversations_skipped_existing: number;
  memories_stored: number;
}

export async function importChatExport(
  path: string,
): Promise<ImportChatExportResponse> {
  return invoke<ImportChatExportResponse>("import_chat_export", { path });
}

export async function saveTempFile(bytes: Uint8Array, filename: string): Promise<string> {
  return invoke<string>("save_temp_file", { bytes: Array.from(bytes), filename });
}

/** User-facing labels for import pipeline stages. Reusable across any UI
 *  that references import state (settings, home, notifications, etc.). */
export const IMPORT_STAGE_LABELS: Record<string, string> = {
  parsing: "Reading archive",
  stage_a: "Importing conversations",
  stage_b: "Classifying and extracting entities",
  done: "Complete",
  error: "Failed",
};

/** Get a user-facing label for an import stage. Falls back to the raw value. */
export function importStageLabel(stage: string): string {
  return IMPORT_STAGE_LABELS[stage] ?? stage;
}

export interface PendingImport {
  id: string;
  vendor: string;
  stage: string;
  source_path: string;
  processed_conversations: number;
  total_conversations: number | null;
}

export async function listPendingImports(): Promise<PendingImport[]> {
  return invoke<PendingImport[]>("list_pending_imports");
}

// ===== Avatar =====

export async function setAvatar(sourcePath: string): Promise<string> {
  return invoke("set_avatar", { sourcePath });
}

export async function getAvatarDataUrl(): Promise<string | null> {
  return invoke<string | null>("get_avatar_data_url");
}

export async function removeAvatar(): Promise<void> {
  return invoke("remove_avatar");
}

// ===== Setup Wizard =====

export interface McpClient {
  name: string;
  client_type: string;
  config_path: string;
  detected: boolean;
  already_configured: boolean;
}

export async function shouldShowWizard(): Promise<boolean> {
  return invoke("should_show_wizard");
}

export interface SetupStatus {
  setup_completed: boolean;
  mode: "basic-memory" | "local-model" | "anthropic-key" | string;
  anthropic_key_configured: boolean;
  local_model_selected: string | null;
  local_model_loaded: string | null;
  local_model_cached: boolean;
}

export async function getSetupStatus(): Promise<SetupStatus> {
  return invoke("get_setup_status");
}

export async function getSetupCompleted(): Promise<boolean> {
  return invoke("get_setup_completed");
}

export async function setSetupCompleted(completed: boolean): Promise<void> {
  return invoke("set_setup_completed", { completed });
}

export async function detectMcpClients(): Promise<McpClient[]> {
  return invoke("detect_mcp_clients_cmd");
}

export async function writeMcpConfig(clientType: string): Promise<void> {
  return invoke("write_mcp_config", { clientType });
}

/** Returns the `wenlan` MCP server entry (command + args) with real values —
 *  either a resolved local binary path (dev) or `npx -y wenlan-mcp` (prod). */
export async function getWenlanMcpEntry(): Promise<{
  command: string;
  args: string[];
}> {
  return invoke("get_wenlan_mcp_entry");
}

// ===== Onboarding Journey Milestones =====

export type MilestoneId =
  | "intelligence-ready"
  | "first-memory"
  | "first-recall"
  | "first-concept"
  | "graph-alive"
  | "second-agent";

export interface MilestoneRecord {
  id: MilestoneId;
  first_triggered_at: number;
  acknowledged_at: number | null;
  payload: Record<string, unknown> | null;
}

export async function listOnboardingMilestones(): Promise<MilestoneRecord[]> {
  return invoke("list_onboarding_milestones");
}

export async function acknowledgeOnboardingMilestone(id: MilestoneId): Promise<void> {
  return invoke("acknowledge_onboarding_milestone", { id });
}

export async function resetOnboardingMilestones(): Promise<void> {
  return invoke("reset_onboarding_milestones");
}

// ===== Briefing & Contradictions =====

export interface BriefingResponse {
  content: string;
  new_today: number;
  primary_agent: string | null;
  generated_at: number;
  is_stale: boolean;
}

export interface ContradictionItem {
  id: string;
  new_content: string;
  existing_content: string;
  new_source_id: string;
  existing_source_id: string;
}

export async function getBriefing(): Promise<BriefingResponse> {
  return invoke("get_briefing");
}

export async function getPendingContradictions(): Promise<ContradictionItem[]> {
  return invoke("get_pending_contradictions");
}

// ===== Profile Narrative =====

export interface NarrativeResponse {
  content: string;
  generated_at: number;
  is_stale: boolean;
  memory_count: number;
}

export async function getProfileNarrative(): Promise<NarrativeResponse> {
  return invoke("get_profile_narrative");
}

export async function regenerateNarrative(): Promise<NarrativeResponse> {
  return invoke("regenerate_narrative");
}

// ===== Agent Activity =====

export interface AgentActivityItem {
  id: number;
  timestamp: number;
  agent_name: string;
  action: string;
  memory_ids: string | null;
  query: string | null;
  detail: string | null;
  memory_titles: string[];
}

export async function listAgentActivity(
  limit?: number,
  agentName?: string,
  since?: number,
): Promise<AgentActivityItem[]> {
  return invoke("list_agent_activity", {
    limit: limit ?? 50,
    agentName: agentName ?? null,
    since: since ?? null,
  });
}

// ── Remote Access ──────────────────────────────────────────────────

export type RemoteAccessStatus =
  | { status: "off" }
  | { status: "starting" }
  | { status: "connected"; tunnel_url: string; token: string; relay_url: string | null }
  | { status: "error"; error: string };

export async function toggleRemoteAccess(
  enabled: boolean,
): Promise<RemoteAccessStatus> {
  return invoke<RemoteAccessStatus>("toggle_remote_access", { enabled });
}

export async function getRemoteAccessStatus(): Promise<RemoteAccessStatus> {
  return invoke<RemoteAccessStatus>("get_remote_access_status");
}

export async function rotateRemoteToken(): Promise<string> {
  return invoke<string>("rotate_remote_token");
}

/** Result of a one-shot probe against the Remote MCP tunnel's `/health`. */
export interface RemoteConnectionTest {
  ok: boolean;
  latency_ms: number | null;
  error: string | null;
}

/** One-shot health probe for the Remote MCP tunnel. Used by the
 *  "Test connection" button in `RemoteAccessPanel`. */
export async function testRemoteMcpConnection(): Promise<RemoteConnectionTest> {
  return invoke<RemoteConnectionTest>("test_remote_mcp_connection");
}

// ── Nurturing Garden ────────────────────────────────────────────────

export async function getNurtureCards(limit?: number, domain?: string): Promise<MemoryItem[]> {
  const memories = await invoke<MemoryItem[]>("get_nurture_cards_cmd", {
    limit: limit ?? 3,
    domain: domain ?? null,
  });
  return withDomainArray(memories);
}

export async function setStability(sourceId: string, stability: "new" | "learned" | "confirmed"): Promise<void> {
  return invoke("set_stability_cmd", { sourceId, stability });
}

export async function correctMemory(sourceId: string, correctionPrompt: string): Promise<string> {
  return invoke("correct_memory_cmd", { sourceId, correctionPrompt });
}

// ── Decision Log ───────────────────────────────────────────────────

export async function listDecisions(
  domain?: string,
  limit?: number,
): Promise<MemoryItem[]> {
  const memories = await invoke<MemoryItem[]>("list_decisions_cmd", {
    domain: domain ?? null,
    limit: limit ?? null,
  });
  return withDomainArray(memories);
}

export async function listDecisionDomains(): Promise<string[]> {
  return invoke("list_decision_domains_cmd", {});
}

// ── Lifecycle / Run-at-login ───────────────────────────────────────

export async function isRunAtLoginEnabled(): Promise<boolean> {
  return invoke("is_run_at_login_enabled");
}

export async function setRunAtLogin(enabled: boolean): Promise<void> {
  return invoke("set_run_at_login", { enabled });
}

export async function quitOriginFull(): Promise<void> {
  return invoke("quit_origin_full");
}
