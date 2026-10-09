// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import {
  ArrowLeft,
  ArrowRight,
  CircleNotch,
  FileText,
  LinkSimple,
  Note,
  WarningCircle,
  Wrench,
} from "@phosphor-icons/react";
import type { RepairPlanEntry } from "../../../lib/repairTypes";
import { getMemoryDetail, getPage, type MemoryItem, type Page } from "../../../lib/tauri";
import {
  isAutomaticRepair,
  knowledgeCheckController,
  knowledgeEntryKey,
  type KnowledgeCheckState,
} from "../../../lib/knowledgeCheckWorkflow";
import { Button } from "../settings/primitives";
import PageInfoDrawer from "../page/PageInfoDrawer";
import "./KnowledgeCheck.css";

interface KnowledgeCheckProps {
  onBack: () => void;
  onOpenReview?: (reviewId?: string) => void;
  onOpenPage?: (id: string) => void;
  onNavigateMemory?: (id: string) => void;
}

type NamedRecord = { kind: "page"; id: string; title: string; item: Page } | { kind: "memory"; id: string; title: string; item: MemoryItem };
const PAGE_SIZE = 50;

type IssueKey = "memoryState" | "tagIntegrity" | "supersession" | "enrichment" | "memoryEntity" | "duplicateTitles" | "orphanLabels" | "projectionVersion" | "projectionIdentity" | "sourcePage" | "semantic" | "unknown";

function issueKey(checkId: string): IssueKey {
  if (checkId === "identity.memory_state_integrity") return "memoryState";
  if (checkId === "identity.tag_integrity") return "tagIntegrity";
  if (checkId === "memories.supersession_integrity") return "supersession";
  if (checkId === "memories.enrichment_failures") return "enrichment";
  if (checkId === "memory_entities.integrity") return "memoryEntity";
  if (checkId === "pages.duplicate_active_titles") return "duplicateTitles";
  if (checkId === "pages.links.orphan_labels") return "orphanLabels";
  if (checkId === "pages.projection.version_alignment") return "projectionVersion";
  if (checkId === "pages.projection.identity") return "projectionIdentity";
  if (checkId === "pages.source_page_integrity") return "sourcePage";
  if (checkId.startsWith("semantic.")) return "semantic";
  return "unknown";
}

type SystemNextStepKey = "schema" | "searchIndex" | "update" | "restart" | "routeScope" | "unknown";
function systemNextStepKey(kind: string): SystemNextStepKey {
  switch (kind) {
    case "run_schema_migration": return "schema";
    case "rebuild_search_index": return "searchIndex";
    case "update_daemon": return "update";
    case "restart_daemon": return "restart";
    case "correct_route_scope_contract": return "routeScope";
    default: return "unknown";
  }
}

function safeAffectedTarget(entry: RepairPlanEntry): { kind: "page" | "memory"; id: string } | null {
  const target = entry.resolution.disposition === "ready" ? entry.resolution.manifest.target : null;
  if (target?.kind === "page" || target?.kind === "page_projection") return { kind: "page", id: target.page_id };
  if (target?.kind === "page_link") return { kind: "page", id: target.source_page_id };
  if (target?.kind === "memory") return { kind: "memory", id: target.source_id };
  if (target?.kind === "memory_entity_link" || target?.kind === "memory_entity_extraction") return { kind: "memory", id: target.memory_id };
  const record = entry.affected_records.find((candidate) => candidate.kind === "page" || candidate.kind === "memory");
  return record ? { kind: record.kind as "page" | "memory", id: record.durable_id } : null;
}

function typeLabel(value: string | null | undefined, t: TFunction): string {
  if (!value) return t("knowledgeCheck.memoryType.unset");
  switch (value) {
    case "identity": return t("knowledgeCheck.memoryType.identity");
    case "preference": return t("knowledgeCheck.memoryType.preference");
    case "decision": return t("knowledgeCheck.memoryType.decision");
    case "lesson": return t("knowledgeCheck.memoryType.lesson");
    case "gotcha": return t("knowledgeCheck.memoryType.gotcha");
    case "fact": return t("knowledgeCheck.memoryType.fact");
    default: return t("knowledgeCheck.memoryType.unknown");
  }
}

function titleForEntry(entry: RepairPlanEntry, t: TFunction): string {
  return t(`knowledgeCheck.issue.${issueKey(entry.check_id)}`);
}

function entryStatus(entry: RepairPlanEntry, result: KnowledgeCheckState["results"][string] | undefined, t: TFunction): { label: string; key: string } {
  if (result === "verified") return { label: t("knowledgeCheck.status.verified"), key: "verified" };
  if (result === "applied_unverified") return { label: t("knowledgeCheck.status.unverified"), key: "unverified" };
  if (result === "applying") return { label: t("knowledgeCheck.status.applying"), key: "applying" };
  if (result === "failed") return { label: t("knowledgeCheck.status.failed"), key: "failed" };
  switch (entry.resolution.disposition) {
    case "ready": return isAutomaticRepair(entry)
      ? { label: t("knowledgeCheck.status.automatic"), key: "automatic" }
      : { label: t("knowledgeCheck.status.manual"), key: "manual" };
    case "review": return { label: t("knowledgeCheck.status.review"), key: "review" };
    case "blocked": return { label: t("knowledgeCheck.status.blocked"), key: "blocked" };
    case "system_action": return { label: t("knowledgeCheck.status.system"), key: "system" };
  }
}

function getMutationChange(entry: RepairPlanEntry, t: TFunction): { label: string; before: string; after: string } | null {
  if (entry.resolution.disposition !== "ready") return null;
  const mutation = entry.resolution.manifest.mutation;
  if (mutation.kind === "reclassify_memory") {
    return {
      label: t("knowledgeCheck.mutation.reclassify"),
      before: typeLabel(mutation.before_memory_type, t),
      after: typeLabel(mutation.after_memory_type, t),
    };
  }
  if (mutation.kind === "rename_page_title") {
    return { label: t("knowledgeCheck.change"), before: mutation.before_title, after: mutation.after_title };
  }
  if (mutation.kind === "bind_page_link") {
    const target = entry.resolution.manifest.target;
    return {
      label: t("knowledgeCheck.mutation.bindLink"),
      before: mutation.before_target_page_id
        ? t("knowledgeCheck.mutation.linked")
        : t("knowledgeCheck.mutation.notLinked", { label: target.kind === "page_link" ? target.label_key : "" }),
      after: t("knowledgeCheck.mutation.targetLoading"),
    };
  }
  if (mutation.kind === "normalize_memory_source_agent") {
    return { label: t("knowledgeCheck.mutation.sourceLabel"), before: t("knowledgeCheck.mutation.emptySourceLabel"), after: t("knowledgeCheck.mutation.noSourceLabel") };
  }
  if (mutation.kind === "clear_memory_supersedes") {
    return { label: t("knowledgeCheck.mutation.memoryRelationship"), before: t("knowledgeCheck.mutation.selfReference"), after: t("knowledgeCheck.mutation.selfReferenceRemoved") };
  }
  if (mutation.kind === "delete_memory_entity_link") {
    return { label: t("knowledgeCheck.mutation.memoryTopicLink"), before: t("knowledgeCheck.mutation.brokenMemoryTopicLink"), after: t("knowledgeCheck.mutation.brokenMemoryTopicLinkRemoved") };
  }
  return null;
}

function phaseBusy(phase: KnowledgeCheckState["phase"]): boolean {
  return phase === "checking" || phase === "repairing" || phase === "recovering";
}

function KnowledgeCheckRow({ entry, selected, status, onSelect, t }: {
  entry: RepairPlanEntry;
  selected: boolean;
  status: { label: string; key: string };
  onSelect: (button: HTMLButtonElement) => void;
  t: TFunction;
}) {
  const target = safeAffectedTarget(entry);
  const [title, setTitle] = useState<string | null>(null);
  const [unavailable, setUnavailable] = useState(false);
  useEffect(() => {
    setTitle(null);
    setUnavailable(false);
    if (!target) return;
    let alive = true;
    const load = async () => {
      try {
        const record = target.kind === "page" ? await getPage(target.id) : await getMemoryDetail(target.id);
        if (!alive) return;
        if (record) setTitle(record.title);
        else setUnavailable(true);
      } catch {
        if (alive) setUnavailable(true);
      }
    };
    void load();
    return () => { alive = false; };
  }, [target?.id, target?.kind]);
  return <button type="button" className={`knowledge-check-row${selected ? " is-selected" : ""}`} aria-pressed={selected} onClick={(event) => onSelect(event.currentTarget)}>
    <span className="knowledge-check-row-icon">
      {entry.resolution.disposition === "ready" ? <LinkSimple aria-hidden="true" size={20} />
        : entry.resolution.disposition === "review" ? <Note aria-hidden="true" size={20} />
          : entry.resolution.disposition === "blocked" ? <WarningCircle aria-hidden="true" size={20} />
            : <Wrench aria-hidden="true" size={20} />}
    </span>
      <span className="knowledge-check-row-copy">
      <span className="knowledge-check-row-title">{titleForEntry(entry, t)}</span>
        <span className="knowledge-check-row-context">{title ?? (unavailable ? t("knowledgeCheck.titleUnavailable") : t("knowledgeCheck.affected"))}</span>
    </span>
      <span className={`knowledge-check-row-status knowledge-check-row-status-${status.key}`}>{status.label}</span>
    <ArrowRight aria-hidden="true" className="knowledge-check-row-chevron" size={17} />
  </button>;
}

export default function KnowledgeCheck({ onBack, onOpenReview, onOpenPage, onNavigateMemory }: KnowledgeCheckProps) {
  const { t } = useTranslation();
  const subscribe = useCallback((listener: () => void) => knowledgeCheckController.subscribe(listener), []);
  const getSnapshot = useCallback(() => knowledgeCheckController.getSnapshot(), []);
  const state = useSyncExternalStore(
    subscribe,
    getSnapshot,
    getSnapshot,
  );
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [currentPage, setCurrentPage] = useState(0);
  const selectedRowRef = useRef<HTMLButtonElement | null>(null);
  const restoreSelectedRow = useRef(false);
  const [namedRecord, setNamedRecord] = useState<NamedRecord | null>(null);
  const [recordUnavailable, setRecordUnavailable] = useState(false);
  const [relatedTargetTitle, setRelatedTargetTitle] = useState<string | null>(null);
  const [relatedTargetUnavailable, setRelatedTargetUnavailable] = useState(false);
  const [relatedBeforeTitle, setRelatedBeforeTitle] = useState<string | null>(null);

  useEffect(() => {
    void knowledgeCheckController.initialize();
  }, []);

  const busy = phaseBusy(state.phase);
  const checkGuarded = busy || state.recoveryRequired;
  const repairGuarded = checkGuarded || state.phase !== "ready";
  const outstandingEntries = useMemo(
    () => state.entries.filter((entry) => state.results[knowledgeEntryKey(entry)] !== "verified"),
    [state.entries, state.results],
  );
  const automaticEntries = useMemo(
    () => outstandingEntries.filter((entry) => isAutomaticRepair(entry) && state.results[knowledgeEntryKey(entry)] === "not_started"),
    [outstandingEntries, state.results],
  );
  const selected = state.entries.find((entry) => knowledgeEntryKey(entry) === selectedKey) ?? null;
  const pageCount = Math.ceil(state.entries.length / PAGE_SIZE);
  const visibleEntries = state.entries.slice(currentPage * PAGE_SIZE, (currentPage + 1) * PAGE_SIZE);
  const selectedTarget = selected ? safeAffectedTarget(selected) : null;
  const verifiedCount = state.completed;
  const repairTotal = state.total || (state.phase === "repairing" || verifiedCount > 0 ? state.entries.length : 0);
  const remainingCount = Math.max(0, repairTotal - verifiedCount);
  const hasProgress = repairTotal > 0 && (state.phase === "repairing" || state.phase === "recovering" || state.phase === "paused" || state.phase === "complete" || state.recoveryRequired || Object.values(state.results).some((result) => result !== "not_started"));
  const completeReport = state.report?.complete === true && state.plan?.deterministic_complete !== false;
  const showNoIssues = !!state.report && completeReport && (state.report.totals?.findings ?? 0) === 0 && outstandingEntries.length === 0;
  const showNoActionable = !!state.report && completeReport && !showNoIssues && outstandingEntries.length === 0;
  const hasResultsView = state.report !== null || state.entries.length > 0;

  useLayoutEffect(() => {
    if (!selected && restoreSelectedRow.current) {
      selectedRowRef.current?.focus({ preventScroll: true });
      restoreSelectedRow.current = false;
    }
  }, [selected]);

  useEffect(() => {
    setCurrentPage(0);
    setSelectedKey(null);
    restoreSelectedRow.current = false;
  }, [state.entries]);

  useEffect(() => {
    setNamedRecord(null);
    setRecordUnavailable(false);
    if (!selectedTarget) return;
    let alive = true;
    const load = async () => {
      try {
        if (selectedTarget.kind === "page") {
          const page = await getPage(selectedTarget.id);
          if (alive && page) setNamedRecord({ kind: "page", id: page.id, title: page.title, item: page });
          else if (alive) setRecordUnavailable(true);
        } else {
          const memory = await getMemoryDetail(selectedTarget.id);
          if (alive && memory) setNamedRecord({ kind: "memory", id: memory.source_id, title: memory.title, item: memory });
          else if (alive) setRecordUnavailable(true);
        }
      } catch {
        if (alive) setRecordUnavailable(true);
      }
    };
    void load();
    return () => { alive = false; };
  }, [selectedKey, selectedTarget?.id, selectedTarget?.kind]);

  useEffect(() => {
    setRelatedTargetTitle(null);
    setRelatedTargetUnavailable(false);
    setRelatedBeforeTitle(null);
    if (selected?.resolution.disposition !== "ready" || selected.resolution.manifest.mutation.kind !== "bind_page_link") return;
    const mutation = selected.resolution.manifest.mutation;
    const beforeId = mutation.before_target_page_id;
    let alive = true;
    void Promise.all([
      getPage(mutation.after_target_page_id).catch(() => null),
      beforeId ? getPage(beforeId).catch(() => null) : Promise.resolve(null),
    ]).then(([after, before]) => {
      if (!alive) return;
      if (after) setRelatedTargetTitle(after.title);
      else setRelatedTargetUnavailable(true);
      if (before) setRelatedBeforeTitle(before.title);
    });
    return () => { alive = false; };
  }, [selectedKey]);

  const runCheck = useCallback(() => {
    if (checkGuarded) return;
    void knowledgeCheckController.check();
  }, [checkGuarded]);
  const runRepair = useCallback((keys: string[]) => {
    if (repairGuarded || keys.length === 0) return;
    void knowledgeCheckController.repair(keys);
  }, [repairGuarded]);
  const recover = useCallback(() => {
    if (busy) return;
    void knowledgeCheckController.recover();
  }, [busy]);

  const openOriginal = () => {
    if (!selectedTarget) return;
    if (selectedTarget.kind === "page") onOpenPage?.(selectedTarget.id);
    else onNavigateMemory?.(selectedTarget.id);
  };

  return (
    <main className="knowledge-check" aria-labelledby="knowledge-check-title">
      <button className="knowledge-check-back" type="button" onClick={onBack} disabled={busy}>
        <ArrowLeft aria-hidden="true" size={18} />
        {t("knowledgeCheck.back")}
      </button>
      <div className="knowledge-check-title-row">
        <h1 id="knowledge-check-title">{t("knowledgeCheck.title")}</h1>
        {(hasResultsView || (state.phase === "error" && state.report === null)) && <div className="knowledge-check-header-actions">
          <Button type="button" size="sm" variant="secondary" onClick={runCheck} disabled={checkGuarded}>{state.report || state.entries.length > 0 ? t("knowledgeCheck.checkAgain") : t("knowledgeCheck.retryCheck")}</Button>
          {automaticEntries.length > 0 && state.phase === "ready" && <Button type="button" variant="primary" onClick={() => runRepair(automaticEntries.map(knowledgeEntryKey))} disabled={repairGuarded}>
            <Wrench aria-hidden="true" size={16} />{t("knowledgeCheck.repairItems", { count: automaticEntries.length })}
          </Button>}
        </div>}
      </div>
      <p className="knowledge-check-scope">{t("knowledgeCheck.scope")}</p>

      {!hasResultsView && state.phase === "idle" && (
        <section className="knowledge-check-start">
          <p>{t("knowledgeCheck.noPlan")}</p>
          <Button type="button" variant="primary" onClick={runCheck} disabled={checkGuarded}>{t("knowledgeCheck.check")}</Button>
        </section>
      )}

      {busy && <p className="knowledge-check-state" role="status">
        <CircleNotch aria-hidden="true" className="knowledge-check-spinner" size={18} />
        {t(`knowledgeCheck.${state.phase === "checking" ? "checking" : state.phase === "repairing" ? "repairing" : "recovering"}`)}
      </p>}

      {state.phase === "error" && <div className="knowledge-check-alert" role="alert">
        <p>{t("knowledgeCheck.checkFailed")}</p>
        {state.report && !state.report.complete && <p>{t("knowledgeCheck.incomplete")}</p>}
        {state.error && <details><summary>{t("knowledgeCheck.errorDetails")}</summary><pre>{state.error}</pre></details>}
      </div>}
      {(state.phase === "paused" || state.recoveryRequired) && <div className="knowledge-check-alert" role="status">
        <p>{t(state.recoveryRequired ? "knowledgeCheck.recoverRequired" : "knowledgeCheck.pausedSafe")}</p>
        {state.recoveryRequired && <Button type="button" variant="secondary" onClick={recover} disabled={busy}>{t("knowledgeCheck.recover")}</Button>}
      </div>}

      {state.report && !busy && state.phase !== "error" && (
        <>
          {showNoIssues ? <p className="knowledge-check-summary" role="status">{t("knowledgeCheck.noIssues")}</p> : showNoActionable ? <p className="knowledge-check-summary" role="status">{t("knowledgeCheck.noFurtherRepairs")}</p> : (
            <p className="knowledge-check-summary" role="status">
              {t("knowledgeCheck.checkedSummary", { count: outstandingEntries.length, automatic: automaticEntries.length })}
            </p>
          )}
          {!state.report.complete && <p className="knowledge-check-notice">{t("knowledgeCheck.incomplete")}</p>}
          {state.plan?.deterministic_complete === false && <p className="knowledge-check-notice">{t("knowledgeCheck.planIncomplete")}</p>}
        </>
      )}

      {!state.report && state.entries.length > 0 && !busy && <p className="knowledge-check-summary" role="status">
        {t("knowledgeCheck.restoredSummary", { count: outstandingEntries.length })}
      </p>}
      {hasProgress && <p className="knowledge-check-progress" role="status">
        {remainingCount === 0 && verifiedCount > 0
          ? t("knowledgeCheck.allVerified")
          : t("knowledgeCheck.repairProgress", { verified: verifiedCount, total: repairTotal, remaining: remainingCount })}
      </p>}
      {state.entries.length > 0 && <>
        <ul className="knowledge-check-list" aria-label={t("knowledgeCheck.title")}>
        {visibleEntries.map((entry) => {
          const key = knowledgeEntryKey(entry);
          const status = entryStatus(entry, state.results[key], t);
          const selectedRow = selectedKey === key;
          return <li key={key}><KnowledgeCheckRow entry={entry} selected={selectedRow} status={status} t={t} onSelect={(button) => { selectedRowRef.current = button; setSelectedKey(key); }} /></li>;
        })}
        </ul>
        {pageCount > 1 && <nav className="knowledge-check-pagination" aria-label={t("knowledgeCheck.pagination")}>
          <Button type="button" size="sm" variant="secondary" disabled={currentPage === 0} onClick={() => {
            setSelectedKey(null);
            restoreSelectedRow.current = false;
            setCurrentPage((page) => Math.max(0, page - 1));
          }}>{t("knowledgeCheck.previousPage")}</Button>
          <span>{t("knowledgeCheck.pageCount", { current: currentPage + 1, total: pageCount })}</span>
          <Button type="button" size="sm" variant="secondary" disabled={currentPage + 1 >= pageCount} onClick={() => {
            setSelectedKey(null);
            restoreSelectedRow.current = false;
            setCurrentPage((page) => Math.min(pageCount - 1, page + 1));
          }}>{t("knowledgeCheck.nextPage")}</Button>
        </nav>}
      </>}

      <PageInfoDrawer
        closeLabel={t("common.close")}
        docked
        onClose={() => { restoreSelectedRow.current = true; setSelectedKey(null); }}
        open={!!selected}
        title={t("knowledgeCheck.detail")}
      >
        {selected && <div className="knowledge-check-detail">
          <h3>{titleForEntry(selected, t)}</h3>
          {selected.resolution.disposition === "review" && <p>{t("knowledgeCheck.reviewHint")}</p>}
          {selected.resolution.disposition === "blocked" && <p>{t("knowledgeCheck.blockedHint")}</p>}
          {selected.resolution.disposition === "system_action" && <p>{t("knowledgeCheck.systemHint")}</p>}

          <section className="knowledge-check-detail-section">
            <h4>{t("knowledgeCheck.affected")}</h4>
            {namedRecord ? <button type="button" className="knowledge-check-original" onClick={openOriginal}>
              {namedRecord.kind === "page" ? <FileText aria-hidden="true" size={18} /> : <Note aria-hidden="true" size={18} />}
              <span>{namedRecord.title}</span>
              <ArrowRight aria-hidden="true" size={16} />
            </button> : <p>{recordUnavailable ? t("knowledgeCheck.titleUnavailable") : t("knowledgeCheck.unavailable")}</p>}
          </section>

          {selected.resolution.disposition === "ready" && getMutationChange(selected, t) && (() => {
            const change = getMutationChange(selected, t)!;
            return <section className="knowledge-check-detail-section">
              <h4>{change.label}</h4>
              <dl className="knowledge-check-change">
                <div><dt>{t("knowledgeCheck.before")}</dt><dd>{selected.resolution.manifest.mutation.kind === "bind_page_link" && relatedBeforeTitle ? relatedBeforeTitle : change.before}</dd></div>
                <div><dt>{t("knowledgeCheck.after")}</dt><dd>{selected.resolution.manifest.mutation.kind === "bind_page_link" ? relatedTargetTitle ?? (relatedTargetUnavailable ? t("knowledgeCheck.titleUnavailable") : change.after) : change.after}</dd></div>
              </dl>
            </section>;
          })()}

          {selected.resolution.disposition === "blocked" && <section className="knowledge-check-detail-section">
            <h4>{t("knowledgeCheck.nextStep")}</h4>
            <p>{selected.check_id === "pages.links.orphan_labels" ? t("knowledgeCheck.orphanLinkNextStep") : selected.resolution.blocked.next_action}</p>
          </section>}
          {selected.resolution.disposition === "system_action" && <section className="knowledge-check-detail-section">
            <h4>{t("knowledgeCheck.nextStep")}</h4>
            <p>{t(`knowledgeCheck.systemNext.${systemNextStepKey(selected.resolution.system_action.kind)}`)}</p>
          </section>}

          <div className="knowledge-check-detail-actions">
            {selected.resolution.disposition === "ready" && isAutomaticRepair(selected) && state.phase === "ready" && (
              <Button type="button" variant="secondary" disabled={repairGuarded || state.results[knowledgeEntryKey(selected)] === "verified"} onClick={() => runRepair([knowledgeEntryKey(selected)])}>
                <Wrench aria-hidden="true" size={16} />{t("knowledgeCheck.repairItem")}
              </Button>
            )}
            {selected.resolution.disposition === "ready" && !isAutomaticRepair(selected) && <p>{t("knowledgeCheck.unsupportedReady")}</p>}
            {selected.resolution.disposition === "review" && <Button type="button" variant="secondary" onClick={() => onOpenReview?.(`refinement:${selected.resolution.disposition === "review" ? selected.resolution.review_item.review_id : ""}`)} disabled={!onOpenReview}>
              {t("knowledgeCheck.reviewItem")}
            </Button>}
            {selectedTarget && <Button type="button" variant="ghost" onClick={openOriginal} disabled={selectedTarget.kind === "page" ? !onOpenPage : !onNavigateMemory}>
              {t("knowledgeCheck.openOriginal")}
            </Button>}
          </div>
          {selected.resolution.disposition !== "ready" && <details className="knowledge-check-diagnostics">
            <summary>{t("knowledgeCheck.statusDetails")}</summary>
            {selected.resolution.disposition === "review" && <pre>{selected.resolution.review_item.issue}</pre>}
            {selected.resolution.disposition === "blocked" && <pre>{selected.resolution.blocked.detail}</pre>}
            {selected.resolution.disposition === "system_action" && <pre>{selected.resolution.system_action.evidence.join("\n")}</pre>}
          </details>}
        </div>}
      </PageInfoDrawer>
    </main>
  );
}
