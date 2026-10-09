// SPDX-License-Identifier: AGPL-3.0-only
import type {
  RepairApplyReceipt, RepairLintReport, RepairManifest, RepairOperationStatus,
  RepairPlanEntry, RepairPlanSummary, RepairVerificationReceipt,
} from "./repairTypes";
import {
  applyRequestForManifest, validateRepairApplyReceipt, validateRepairOperationStatus,
  validateRepairVerificationReceipt,
} from "./repairWorkflow";
import {
  getActivity, repairApply, repairCancel, repairLint, repairOperationStatus, repairPlan,
  repairPlanEntries, repairResumeRuntime, repairValidateManifest, repairVerify,
} from "./tauri";

const STORAGE_KEY = "wenlan.knowledge-check.progress.v1";
const LOCK_NAME = "wenlan.knowledge-check.repair.v1";
const PAGE_LIMIT = 100;
const MAX_ENTRIES = 10_000;
const DIGEST = /^[0-9a-f]{64}$/;
// Lint snapshot IDs use the backend LintDigest (u64), not repair SHA-256 digests.
const LINT_DIGEST = /^[0-9a-f]{16}$/;

export type KnowledgeCheckResult = "not_started" | "applying" | "applied_unverified" | "verified" | "failed";
export type KnowledgeCheckPhase = "idle" | "checking" | "ready" | "repairing" | "recovering" | "paused" | "complete" | "error";
export interface KnowledgeCheckState {
  readonly phase: KnowledgeCheckPhase;
  readonly report: RepairLintReport | null;
  readonly plan: RepairPlanSummary | null;
  readonly entries: readonly RepairPlanEntry[];
  readonly results: Readonly<Record<string, KnowledgeCheckResult>>;
  readonly error: string | null;
  readonly recoveryRequired: boolean;
  readonly completed: number;
  readonly total: number;
}

interface BatchItem { key: string; entry: RepairPlanEntry; }
interface SavedBatch {
  version: 1;
  scope: "global";
  planId: string;
  planDigest: string;
  items: BatchItem[];
  results: Record<string, KnowledgeCheckResult>;
  currentKey: string | null;
  stage: "prepared" | "sent" | "applied_unverified" | "verified" | "abandoned" | "complete";
  applyReceipt?: RepairApplyReceipt;
  verificationReceipt?: RepairVerificationReceipt;
}

type LockManager = { request<T>(name: string, options: { mode: "exclusive"; ifAvailable: true }, callback: (lock: unknown | null) => Promise<T>): Promise<T> };
interface WorkflowDeps {
  repairLint: typeof repairLint;
  repairPlan: typeof repairPlan;
  repairPlanEntries: typeof repairPlanEntries;
  repairValidateManifest: typeof repairValidateManifest;
  repairApply: typeof repairApply;
  repairOperationStatus: typeof repairOperationStatus;
  repairCancel: typeof repairCancel;
  repairVerify: typeof repairVerify;
  repairResumeRuntime: typeof repairResumeRuntime;
  getActivity: typeof getActivity;
  storage?: Storage;
  locks?: LockManager;
}

const makeInitial = (): KnowledgeCheckState => freezeState({ phase: "idle", report: null, plan: null, entries: [], results: {}, error: null, recoveryRequired: false, completed: 0, total: 0 });

function freezeState(state: KnowledgeCheckState): KnowledgeCheckState {
  const deepFreeze = (value: unknown): void => {
    if (!value || typeof value !== "object" || Object.isFrozen(value)) return;
    Object.values(value).forEach(deepFreeze);
    Object.freeze(value);
  };
  deepFreeze(state.report);
  deepFreeze(state.plan);
  deepFreeze(state.entries);
  deepFreeze(state.results);
  return Object.freeze(state);
}
function errorText(error: unknown): string { return error instanceof Error ? error.message : String(error); }
function isObject(value: unknown): value is Record<string, unknown> { return typeof value === "object" && value !== null; }
function stableScope(report: RepairLintReport): boolean { return exactGlobalScope(report.scope); }
function completeGeneral(report: RepairLintReport): boolean {
  if (!(report.profile === "general" && report.complete === true && stableScope(report) &&
    report.totals?.incomplete === 0 && Array.isArray(report.checks) &&
    typeof report.snapshots?.db?.analysis_digest === "string" && LINT_DIGEST.test(report.snapshots.db.analysis_digest) &&
    report.snapshots.db.post_run_digest === report.snapshots.db.analysis_digest &&
    typeof report.snapshots?.pages?.before_scan_digest === "string" && LINT_DIGEST.test(report.snapshots.pages.before_scan_digest) &&
    report.snapshots.pages.after_scan_digest === report.snapshots.pages.before_scan_digest &&
    report.totals.checks === report.checks.length && report.checks.every((c) => (c.outcome === "pass" || c.outcome === "finding") &&
      Array.isArray(c.evidence) && !c.evidence.some((e) => e.kind === "semantic_finding")))) return false;
  const passed = report.checks.filter((c) => c.outcome === "pass").length;
  const findings = report.checks.filter((c) => c.outcome === "finding").length;
  return report.totals.passed === passed && report.totals.findings === findings &&
    report.totals.actionable_findings + report.totals.advisory_findings === findings &&
    new Set(report.checks.map((c) => c.check_id)).size === report.checks.length;
}
function exactGlobalScope(value: unknown): boolean { return isObject(value) && value.kind === "global" && Object.keys(value).length === 1; }
function sameScope(left: unknown, right: unknown): boolean { return JSON.stringify(left) === JSON.stringify(right); }
function validApplyReceipt(receipt: RepairApplyReceipt, manifest: RepairManifest): boolean {
  return validateRepairApplyReceipt(receipt, manifest) && receipt.writer === manifest.writer &&
    JSON.stringify(receipt.actual_effects) === JSON.stringify(manifest.allowed_effects);
}

function validPlanEntry(value: unknown): value is RepairPlanEntry {
  if (!isObject(value) || typeof value.check_id !== "string" || !value.check_id ||
      typeof value.occurrence_digest !== "string" || !DIGEST.test(value.occurrence_digest) ||
      !Array.isArray(value.affected_records) || !value.affected_records.every((record) => isObject(record) && typeof record.kind === "string" && typeof record.durable_id === "string") ||
      !isObject(value.resolution)) return false;
  const resolution = value.resolution;
  if (resolution.disposition === "ready") {
    if (!isObject(resolution.manifest) || !isObject(resolution.manifest.target) || typeof resolution.manifest.target.kind !== "string" ||
        !isObject(resolution.manifest.mutation) || typeof resolution.manifest.mutation.kind !== "string") return false;
    const target = resolution.manifest.target;
    if ((target.kind === "page" || target.kind === "page_projection") && typeof target.page_id !== "string") return false;
    if (target.kind === "page_link" && (typeof target.source_page_id !== "string" || typeof target.label_key !== "string")) return false;
    if (target.kind === "memory" && typeof target.source_id !== "string") return false;
    if ((target.kind === "memory_entity_link" || target.kind === "memory_entity_extraction") && typeof target.memory_id !== "string") return false;
    return typeof resolution.manifest.mutation.kind === "string" &&
      (resolution.manifest.mutation.kind !== "bind_page_link" || typeof resolution.manifest.mutation.after_target_page_id === "string");
  }
  if (resolution.disposition === "review") {
    return isObject(resolution.review_item) && typeof resolution.review_item.review_id === "string" &&
      typeof resolution.review_item.issue === "string" && Array.isArray(resolution.review_item.choices) &&
      resolution.review_item.choices.every((choice) => typeof choice === "string") &&
      Array.isArray(resolution.review_item.suggested_research_queries) && resolution.review_item.suggested_research_queries.every((query) => typeof query === "string");
  }
  if (resolution.disposition === "blocked") {
    return isObject(resolution.blocked) && typeof resolution.blocked.detail === "string" && typeof resolution.blocked.next_action === "string";
  }
  if (resolution.disposition === "system_action") {
    return isObject(resolution.system_action) && typeof resolution.system_action.kind === "string" &&
      Array.isArray(resolution.system_action.evidence) && resolution.system_action.evidence.every((evidence) => typeof evidence === "string");
  }
  return false;
}

export function knowledgeEntryKey(entry: RepairPlanEntry): string {
  return `${String(entry?.check_id)}:${String(entry?.occurrence_digest)}`;
}

/** Exact allowlist for one-click deterministic, low-impact writers. */
export function isAutomaticRepair(entry: RepairPlanEntry): boolean {
  try { return automaticRepairShape(entry); } catch { return false; }
}
function automaticRepairShape(entry: RepairPlanEntry): boolean {
  if (!isObject(entry) || typeof entry.check_id !== "string" || typeof entry.occurrence_digest !== "string" || !isObject(entry.resolution)) return false;
  if (entry.resolution.disposition !== "ready") return false;
  const m = entry.resolution.manifest;
  if (!DIGEST.test(entry.occurrence_digest) || !DIGEST.test(m.manifest_digest) ||
      m.source.check_id !== entry.check_id || m.source.finding != null ||
      !exactGlobalScope(m.source.lint_scope) || !exactGlobalScope(m.source.report_scope) ||
      !isObject(m.target) || !isObject(m.allowed_effects) || !isObject(m.allowed_effects.owner) ||
      !sameScope(m.target.scope, m.allowed_effects.owner.scope) ||
      m.post_assertions.target_check_id !== entry.check_id || m.post_assertions.verification_policy.kind !== "general_only" ||
      m.post_assertions.require_complete_general !== true || m.post_assertions.reject_new_actionable !== true ||
      m.post_assertions.reject_new_incomplete !== true || m.post_assertions.deep_baseline.length !== 0) return false;
  const owner = m.target;
  const effects = m.allowed_effects;
  if (m.writer === "bind_page_link" && m.mutation.kind === "bind_page_link") {
    return entry.check_id === "pages.links.orphan_labels" && owner.kind === "page_link" && effects.owner.kind === "page_link" &&
      effects.owner.source_page_id === owner.source_page_id && effects.owner.label_key === owner.label_key &&
      sameScope(effects.owner.scope, owner.scope) &&
      m.mutation.after_target_page_id.length > 0 && effects.fields.length === 1 && effects.fields[0] === "target_page_id";
  }
  if (m.writer === "normalize_memory_source_agent" && m.mutation.kind === "normalize_memory_source_agent") {
    return entry.check_id === "identity.memory_state_integrity" && owner.kind === "memory" && effects.owner.kind === "memory" && effects.owner.source_id === owner.source_id &&
      sameScope(effects.owner.scope, owner.scope) &&
      m.mutation.before_source_agent.trim() === "" && effects.fields.length === 1 && effects.fields[0] === "source_agent";
  }
  if (m.writer === "clear_memory_supersedes" && m.mutation.kind === "clear_memory_supersedes") {
    return (entry.check_id === "identity.memory_state_integrity" || entry.check_id === "memories.supersession_integrity") && owner.kind === "memory" && effects.owner.kind === "memory" && effects.owner.source_id === owner.source_id &&
      sameScope(effects.owner.scope, owner.scope) &&
      m.mutation.before_supersedes === owner.source_id && effects.fields.length === 1 && effects.fields[0] === "supersedes";
  }
  if (m.writer === "delete_memory_entity_link" && m.mutation.kind === "delete_memory_entity_link") {
    return entry.check_id === "memory_entities.integrity" && owner.kind === "memory_entity_link" && effects.owner.kind === "memory_entity_link" &&
      effects.owner.memory_id === owner.memory_id && effects.owner.entity_id === owner.entity_id &&
      sameScope(effects.owner.scope, owner.scope) &&
      m.mutation.memory_id === owner.memory_id && m.mutation.entity_id === owner.entity_id &&
      effects.fields.length === 1 && effects.fields[0] === "memory_entity_link";
  }
  return false;
}

function browserStorage(): Storage {
  if (typeof window === "undefined" || !window.localStorage) throw new Error("knowledge check progress storage is unavailable");
  return window.localStorage;
}

export class KnowledgeCheckController {
  private readonly deps: WorkflowDeps;
  private snapshot = makeInitial();
  private listeners = new Set<() => void>();
  private busy = false;

  constructor(deps: Partial<WorkflowDeps> = {}) {
    this.deps = {
      repairLint, repairPlan, repairPlanEntries, repairValidateManifest, repairApply,
      repairOperationStatus, repairCancel, repairVerify, repairResumeRuntime, getActivity,
      storage: deps.storage, locks: deps.locks === undefined ? globalThis.navigator?.locks : deps.locks ?? undefined,
      ...deps,
    } as WorkflowDeps;
  }

  subscribe = (listener: () => void): (() => void) => { this.listeners.add(listener); return () => this.listeners.delete(listener); };
  getSnapshot = (): KnowledgeCheckState => this.snapshot;
  private storage(): Storage { return this.deps.storage ?? browserStorage(); }

  private set(patch: Partial<KnowledgeCheckState>): void {
    this.snapshot = freezeState({ ...this.snapshot, ...patch });
    for (const listener of this.listeners) listener();
  }
  private lock<T>(operation: () => Promise<T>): Promise<T> {
    if (!this.deps.locks) return Promise.reject(new Error("exclusive repair lock is unavailable in this browser"));
    return this.deps.locks.request(LOCK_NAME, { mode: "exclusive", ifAvailable: true }, (lock) => {
      if (lock === null) throw new Error("another window is already repairing knowledge");
      return operation();
    });
  }
  private loadSaved(): SavedBatch | null {
    const raw = this.storage().getItem(STORAGE_KEY);
    if (raw === null) return null;
    const value: unknown = JSON.parse(raw);
    if (!isObject(value) || value.version !== 1 || value.scope !== "global" || !Array.isArray(value.items) || value.items.length === 0 ||
        !isObject(value.results) || typeof value.planId !== "string" || value.planId.length === 0 || !DIGEST.test(String(value.planDigest)) ||
        !(value.currentKey === null || typeof value.currentKey === "string") ||
        !["prepared", "sent", "applied_unverified", "verified", "abandoned", "complete"].includes(String(value.stage))) {
      throw new Error("saved knowledge check progress is malformed");
    }
    const batch = value as unknown as SavedBatch;
    if (batch.items.some((item) => !item || !validPlanEntry(item.entry) || !isAutomaticRepair(item.entry) || item.key !== knowledgeEntryKey(item.entry)) ||
        Object.keys(batch.results).some((key) => !batch.items.some((item) => item.key === key)) ||
        batch.items.some((item) => !(item.key in batch.results)) ||
        Object.values(batch.results).some((result) => !["not_started", "applying", "applied_unverified", "verified", "failed"].includes(String(result))) ||
        new Set(batch.items.map((item) => item.key)).size !== batch.items.length ||
        (batch.currentKey !== null && !batch.items.some((item) => item.key === batch.currentKey)) ||
        (batch.stage === "complete" && (batch.currentKey !== null || Object.values(batch.results).some((result) => result !== "verified"))) ||
        (batch.stage === "abandoned" && (batch.currentKey !== null || Object.values(batch.results).some((result) => result === "applying" || result === "applied_unverified"))) ||
        (["sent", "applied_unverified", "verified"].includes(batch.stage) && batch.currentKey === null)) {
      throw new Error("saved knowledge check progress does not match its eligible entries");
    }
    return batch;
  }
  private persist(batch: SavedBatch): void { this.storage().setItem(STORAGE_KEY, JSON.stringify(batch)); }
  private unresolved(batch: SavedBatch): boolean {
    return batch.stage === "sent" || batch.stage === "applied_unverified" || batch.stage === "verified" ||
      (batch.stage === "prepared" && batch.currentKey !== null) ||
      Object.values(batch.results).some((result) => result === "applying" || result === "applied_unverified");
  }

  async initialize(): Promise<void> {
    if (this.busy) return;
    try {
      const batch = this.loadSaved();
      if (!batch) return;
      const recoveryRequired = this.unresolved(batch);
      const terminal = batch.stage === "complete" && Object.values(batch.results).every((result) => result === "verified");
      this.set({ phase: recoveryRequired ? "paused" : terminal ? "complete" : "paused", entries: batch.items.map((i) => i.entry), results: { ...batch.results }, total: batch.items.length, completed: Object.values(batch.results).filter((x) => x === "verified").length, recoveryRequired, error: recoveryRequired ? "A prior repair needs recovery" : terminal ? null : "Run a fresh check to continue" });
    } catch (error) {
      this.set({ phase: "error", error: errorText(error), recoveryRequired: true });
    }
  }

  async check(): Promise<void> {
    if (this.busy) return;
    try {
      const persisted = this.loadSaved();
      if (persisted && this.unresolved(persisted)) { this.set({ phase: "paused", error: "Recover the outstanding repair before running another check", recoveryRequired: true }); return; }
    } catch (error) { this.set({ phase: "error", error: errorText(error), recoveryRequired: true }); return; }
    this.busy = true;
    this.set({ phase: "checking", report: null, plan: null, entries: [], results: {}, error: null, recoveryRequired: false, completed: 0, total: 0 });
    let observedReport: RepairLintReport | null = null;
    try {
      const report = await this.deps.repairLint({ profile: "general" });
      observedReport = report;
      if (!completeGeneral(report)) { this.set({ report }); throw new Error("General knowledge check is incomplete or unstable; no repairs are available"); }
      const plan = await this.deps.repairPlan({ scope: { kind: "global" }, general_report: report });
      if (!isObject(plan) || typeof plan.plan_id !== "string" || plan.plan_id.length === 0 || !DIGEST.test(plan.plan_digest) || !Number.isSafeInteger(plan.entry_count) || plan.entry_count < 0 || plan.entry_count > MAX_ENTRIES || !exactGlobalScope(plan.scope) || plan.deterministic_complete !== true) throw new Error("repair plan summary is malformed, incomplete, or exceeds the safe limit");
      const entries: RepairPlanEntry[] = [];
      let offset = 0;
      while (offset < plan.entry_count) {
        const page = await this.deps.repairPlanEntries({ plan_id: plan.plan_id, plan_digest: plan.plan_digest, offset, limit: PAGE_LIMIT });
        const expectedNext = offset + page.entries.length < plan.entry_count ? offset + page.entries.length : null;
        if (page.plan_id !== plan.plan_id || page.plan_digest !== plan.plan_digest || !exactGlobalScope(page.scope) ||
            page.offset !== offset || page.total_entries !== plan.entry_count || !Array.isArray(page.entries) ||
            page.entries.length === 0 || page.entries.length > PAGE_LIMIT || (page.next_offset ?? null) !== expectedNext) throw new Error("repair plan entries are truncated or do not match the plan digest");
        entries.push(...page.entries);
        if (entries.length > plan.entry_count) throw new Error("repair plan returned more entries than its declared count");
        offset = page.next_offset ?? plan.entry_count;
      }
      if (entries.length !== plan.entry_count || entries.some((entry) => !validPlanEntry(entry)) || new Set(entries.map(knowledgeEntryKey)).size !== entries.length) throw new Error("repair plan entries are malformed, duplicated, or do not match the declared count");
      const results = Object.fromEntries(entries.map((e) => [knowledgeEntryKey(e), "not_started" as const]));
      this.set({ phase: "ready", report, plan, entries, results, total: entries.length, completed: 0, error: null, recoveryRequired: false });
    } catch (error) {
      this.set({ phase: "error", report: observedReport, error: errorText(error), recoveryRequired: false });
    } finally { this.busy = false; }
  }

  async repair(keys: string[]): Promise<void> {
    if (this.busy || this.snapshot.phase !== "ready" || this.snapshot.recoveryRequired || !this.snapshot.report ||
        !this.snapshot.plan || !DIGEST.test(this.snapshot.plan.plan_digest) || !completeGeneral(this.snapshot.report)) return;
    const selected = [...new Set(keys)];
    if (!selected.length || selected.length !== keys.length) return;
    const byKey = new Map(this.snapshot.entries.map((entry) => [knowledgeEntryKey(entry), entry]));
    const items = selected.map((key) => ({ key, entry: byKey.get(key)! }));
    if (items.some((item) => !item.entry || !isAutomaticRepair(item.entry) || this.snapshot.results[item.key] !== "not_started")) return;
    const batch: SavedBatch = { version: 1, scope: "global", planId: this.snapshot.plan.plan_id, planDigest: this.snapshot.plan.plan_digest, items, results: Object.fromEntries(items.map((item) => [item.key, "not_started"])), currentKey: null, stage: "prepared" };
    this.busy = true;
    this.set({ phase: "repairing", error: null });
    try {
      await this.lock(async () => {
        let ownsBatch = false;
        try {
          const existing = this.loadSaved();
          if (existing && this.unresolved(existing)) throw new Error("another window has an outstanding repair that must be recovered first");
          this.persist(batch);
          ownsBatch = true;
          this.set({ total: items.length, completed: 0 });
          for (const item of items) {
            batch.currentKey = item.key;
            batch.stage = "prepared";
            batch.results[item.key] = "applying";
            this.persist(batch);
            this.set({ results: { ...this.snapshot.results, ...batch.results }, recoveryRequired: true });
            const manifest = item.entry.resolution.disposition === "ready" ? item.entry.resolution.manifest : null;
            if (!manifest || !(await this.deps.repairValidateManifest(manifest))) throw new Error("repair manifest signature is invalid or stale");
            const approval = applyRequestForManifest(manifest);
            batch.stage = "sent";
            this.persist(batch);
            let receipt: RepairApplyReceipt;
            try { receipt = await this.deps.repairApply(approval); }
            catch (error) { batch.stage = "applied_unverified"; batch.results[item.key] = "applied_unverified"; this.persist(batch); throw error; }
            if (!validApplyReceipt(receipt, manifest)) throw new Error("repair apply receipt does not match the approved manifest and allowed effects");
            batch.stage = "applied_unverified"; batch.results[item.key] = "applied_unverified"; batch.applyReceipt = receipt; this.persist(batch);
            this.set({ results: { ...this.snapshot.results, ...batch.results } });
            const fresh = await this.deps.repairLint({ profile: "general" });
            if (!completeGeneral(fresh)) throw new Error("post-repair General check is incomplete or unstable");
            const verification = await this.deps.repairVerify({ manifest_id: manifest.manifest_id, manifest_digest: manifest.manifest_digest, apply_receipt_digest: receipt.receipt_digest, general_report: fresh });
            if (!validateRepairVerificationReceipt(verification, manifest, receipt)) throw new Error("repair verification receipt does not match the apply receipt");
            batch.stage = "verified"; batch.verificationReceipt = verification; this.persist(batch);
            await this.resumeAndProbe(manifest, verification);
            batch.results[item.key] = "verified"; batch.currentKey = null; batch.stage = "prepared"; delete batch.applyReceipt; delete batch.verificationReceipt; this.persist(batch);
            this.set({ report: fresh, results: { ...this.snapshot.results, ...batch.results }, completed: Object.values(batch.results).filter((x) => x === "verified").length });
          }
          batch.stage = "complete"; batch.currentKey = null; this.persist(batch);
        } catch (error) {
          if (ownsBatch) {
            const current = batch.currentKey;
            if (current && batch.results[current] !== "verified") {
              batch.results[current] = batch.stage === "sent" || batch.stage === "applied_unverified" || batch.stage === "verified" ? "applied_unverified" : "failed";
            }
            try { this.persist(batch); } catch { /* keep the initial failure and the last durable record */ }
            this.set({ phase: "paused", results: { ...this.snapshot.results, ...batch.results }, error: errorText(error), recoveryRequired: this.unresolved(batch) });
          }
          throw error;
        }
      });
      const allVerified = Object.values(batch.results).filter((result) => result === "verified").length;
      this.set({ phase: "complete", results: { ...this.snapshot.results, ...batch.results }, completed: allVerified, total: items.length, recoveryRequired: false, error: null });
    } catch (error) {
      this.set({ phase: "paused", error: errorText(error) });
    } finally { this.busy = false; }
  }

  private async resumeAndProbe(manifest: RepairManifest, verification: RepairVerificationReceipt): Promise<void> {
    await this.deps.repairResumeRuntime({ apply: applyRequestForManifest(manifest), verification_receipt_digest: verification.receipt_digest });
    const activity = await this.deps.getActivity();
    if (!activity || !activity.state) throw new Error("runtime did not answer the post-repair activity probe");
  }

  async recover(): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    this.set({ phase: "recovering", error: null });
    try {
      await this.lock(async () => {
        const batch = this.loadSaved();
        if (!batch) { this.set({ phase: "ready", recoveryRequired: false }); return; }
        const key = batch.currentKey;
        if (!key && this.unresolved(batch)) throw new Error("saved repair batch has no current manifest identity");
        const item = key ? batch.items.find((i) => i.key === key) : undefined;
        if (!item || !key) {
          if (this.unresolved(batch)) throw new Error("saved repair batch has no recoverable current entry");
          const complete = Object.values(batch.results).length > 0 && Object.values(batch.results).every((result) => result === "verified");
          batch.stage = complete ? "complete" : "abandoned";
          this.persist(batch);
          this.set({ phase: complete ? "complete" : "paused", entries: batch.items.map((i) => i.entry), results: { ...batch.results }, total: batch.items.length, completed: Object.values(batch.results).filter((x) => x === "verified").length, recoveryRequired: false, error: complete ? null : "Run a fresh check to continue" });
          return;
        }
        if (item.entry.resolution.disposition !== "ready") throw new Error("saved repair entry has no exact manifest");
        const manifest = item.entry.resolution.manifest;
        if (!(await this.deps.repairValidateManifest(manifest))) throw new Error("saved repair manifest is stale or invalid");
        const approval = applyRequestForManifest(manifest);
        const statusValue = await this.deps.repairOperationStatus(approval);
        if (!validateRepairOperationStatus(statusValue, manifest)) throw new Error("repair operation status is not bound to the saved manifest");
        const state = statusValue.state;
        if (state.phase === "prepared") {
          const cancelled: RepairOperationStatus = await this.deps.repairCancel(approval);
          if (!validateRepairOperationStatus(cancelled, manifest) || cancelled.state.phase !== "cancelled") throw new Error("prepared repair could not be safely cancelled");
          batch.stage = "abandoned"; batch.results[key] = "failed"; batch.currentKey = null; this.persist(batch);
          this.set({ phase: "paused", entries: batch.items.map((i) => i.entry), results: { ...batch.results }, total: batch.items.length, completed: Object.values(batch.results).filter((x) => x === "verified").length, recoveryRequired: false, error: "The prepared repair was cancelled; run a fresh check before continuing" });
          return;
        }
        if (state.phase === "cancelled") {
          batch.stage = "abandoned"; batch.results[key] = "failed"; batch.currentKey = null; this.persist(batch);
          this.set({ phase: "paused", results: { ...batch.results }, recoveryRequired: false, error: "The repair was cancelled; run a fresh check before continuing" }); return;
        }
        if (state.phase === "in_progress" || state.phase === "indeterminate") throw new Error("repair outcome is still indeterminate; no retry was sent");
        let applyReceipt: RepairApplyReceipt;
        let verification: RepairVerificationReceipt | undefined;
        if (state.phase === "applied_unverified" || state.phase === "verified") {
          applyReceipt = state.apply_receipt;
          if (state.phase === "verified") verification = state.verification_receipt;
        } else throw new Error("unknown repair operation state; no retry was sent");
        if (!validApplyReceipt(applyReceipt, manifest)) throw new Error("recovered apply receipt does not match the approved manifest and allowed effects");
        batch.stage = verification ? "verified" : "applied_unverified"; batch.results[key] = verification ? "applied_unverified" : "applied_unverified"; batch.applyReceipt = applyReceipt; if (verification) batch.verificationReceipt = verification; this.persist(batch);
        if (!verification) {
          const fresh = await this.deps.repairLint({ profile: "general" });
          if (!completeGeneral(fresh)) throw new Error("recovery General check is incomplete or unstable");
          verification = await this.deps.repairVerify({ manifest_id: manifest.manifest_id, manifest_digest: manifest.manifest_digest, apply_receipt_digest: applyReceipt.receipt_digest, general_report: fresh });
          if (!validateRepairVerificationReceipt(verification, manifest, applyReceipt)) throw new Error("recovery verification receipt does not match");
          batch.stage = "verified"; batch.verificationReceipt = verification; this.persist(batch);
          this.set({ report: fresh });
        }
        await this.resumeAndProbe(manifest, verification);
        batch.results[key] = "verified"; batch.currentKey = null;
        const complete = Object.values(batch.results).length > 0 && Object.values(batch.results).every((result) => result === "verified");
        batch.stage = complete ? "complete" : "abandoned";
        delete batch.applyReceipt; delete batch.verificationReceipt; this.persist(batch);
        this.set({ phase: complete ? "complete" : "paused", entries: batch.items.map((i) => i.entry), results: { ...batch.results }, total: batch.items.length, completed: Object.values(batch.results).filter((x) => x === "verified").length, recoveryRequired: false, error: complete ? null : "Run a fresh check to continue" });
      });
    } catch (error) {
      this.set({ phase: "paused", error: errorText(error), recoveryRequired: true });
    } finally { this.busy = false; }
  }
}

export const knowledgeCheckController = new KnowledgeCheckController();
