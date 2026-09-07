// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  getImportBatchStatus,
  type ImportBatchStatus,
  type ImportPhase,
  type ImportPhaseState,
  type ImportPhaseStatus,
} from "../../lib/tauri";

/** Every phase, in the order a user sees them. Matches ImportPhase::ALL. */
export const IMPORT_PHASE_ORDER: ImportPhase[] = [
  "ingest",
  "store",
  "detect",
  "enrich",
  "link",
  "distill",
];

/** How often to re-read the daemon's live row counts. Matches the chat-import
 *  poll in ChatImport/ImportFlow (immediate read, then an interval). */
export const IMPORT_BATCH_POLL_MS = 1_500;

/** Display names for import source slugs. Product names stay English in every
 *  locale, matching the existing settings copy. */
export const IMPORT_SOURCE_LABELS: Record<string, string> = {
  chatgpt: "ChatGPT",
  claude: "Claude",
  other: "Other",
};

export function importSourceLabel(source: string): string {
  return IMPORT_SOURCE_LABELS[source] ?? source;
}

/**
 * Live row counts for one import batch. Same polling shape as ImportFlow's
 * pending-import poll: an immediate read, a 1.5 s interval, an `alive` guard,
 * and cleanup on unmount. Stops polling once the daemon reports `complete`.
 */
export function useImportBatchStatus(batchId: string | null): ImportBatchStatus | null {
  const [status, setStatus] = useState<ImportBatchStatus | null>(null);

  useEffect(() => {
    setStatus(null);
    if (!batchId) return;
    let alive = true;
    let id: ReturnType<typeof setInterval> | null = null;
    const poll = () => {
      getImportBatchStatus(batchId)
        .then((next) => {
          if (!alive) return;
          setStatus(next);
          if (next.complete && id !== null) {
            clearInterval(id);
            id = null;
          }
        })
        .catch(() => {});
    };
    poll();
    id = setInterval(poll, IMPORT_BATCH_POLL_MS);
    return () => {
      alive = false;
      if (id !== null) clearInterval(id);
    };
  }, [batchId]);

  return status;
}

export interface ImportBatchSummary {
  phases: ImportPhaseStatus[];
  memoriesImported: number;
  memoriesSkipped: number;
  entitiesDetected: number;
  entitiesEstablished: number;
  pagesDistilled: number;
  /** Every batch reports every background phase settled. */
  complete: boolean;
  /** Phases still doing work, in display order. */
  runningPhases: ImportPhase[];
  failedPhases: ImportPhase[];
}

function emptyPhase(phase: ImportPhase): ImportPhaseStatus {
  return { phase, state: "pending", done: 0, total: 0, failed: 0 };
}

/**
 * One pill however many batches are active: per-phase counts add up across
 * batches, and a phase reads as failed/running while any batch says so.
 */
export function summarizeImportBatches(batches: ImportBatchStatus[]): ImportBatchSummary {
  const byPhase = new Map<ImportPhase, ImportPhaseStatus>();
  let memoriesImported = 0;
  let memoriesSkipped = 0;
  let entitiesDetected = 0;
  let entitiesEstablished = 0;
  let pagesDistilled = 0;

  for (const batch of batches) {
    memoriesImported += batch.memories_imported;
    memoriesSkipped += batch.memories_skipped;
    entitiesDetected += batch.entities_detected;
    entitiesEstablished += batch.entities_established;
    pagesDistilled += batch.pages_distilled;
    for (const entry of batch.phases) {
      const prev = byPhase.get(entry.phase);
      if (!prev) {
        byPhase.set(entry.phase, { ...entry });
        continue;
      }
      prev.done += entry.done;
      prev.total += entry.total;
      prev.failed += entry.failed;
      prev.state = combineState(prev.state, entry.state);
    }
  }

  // A batch that never reported a phase row leaves it pending — and a phase
  // no batch finished stays out of `complete`, so the pill cannot claim work
  // is done that a batch has not even started.
  const phases = IMPORT_PHASE_ORDER.map((phase) => byPhase.get(phase) ?? emptyPhase(phase));
  const runningPhases = phases
    .filter((p) => p.state === "running" || p.state === "pending")
    .map((p) => p.phase);
  const failedPhases = phases.filter((p) => p.state === "failed").map((p) => p.phase);
  const complete =
    batches.length > 0 &&
    batches.every((b) => b.complete) &&
    runningPhases.length === 0 &&
    failedPhases.length === 0;

  return {
    phases,
    memoriesImported,
    memoriesSkipped,
    entitiesDetected,
    entitiesEstablished,
    pagesDistilled,
    complete,
    runningPhases,
    failedPhases,
  };
}

function combineState(a: ImportPhaseState, b: ImportPhaseState): ImportPhaseState {
  if (a === "failed" || b === "failed") return "failed";
  if (a === "running" || b === "running") return "running";
  if (a === "pending" || b === "pending") return "pending";
  return "complete";
}

function PulseStyle() {
  return (
    <style>{`
      @keyframes pulse-subtle {
        0%, 100% { opacity: 1; }
        50% { opacity: 0.5; }
      }
    `}</style>
  );
}

function PhaseDot({ state }: { state: ImportPhaseState }) {
  const color =
    state === "complete"
      ? "var(--mem-accent-sage)"
      : state === "failed"
        ? "#ef4444"
        : state === "running"
          ? "var(--mem-accent-indigo)"
          : "var(--mem-text-tertiary)";
  if (state === "complete") {
    return (
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
        stroke={color} strokeWidth="2.5"
        strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <polyline points="20 6 9 17 4 12" />
      </svg>
    );
  }
  return (
    <span
      aria-hidden="true"
      style={{
        width: "8px",
        height: "8px",
        borderRadius: "50%",
        backgroundColor: color,
        flexShrink: 0,
        ...(state === "running"
          ? { animation: "pulse-subtle 2s ease-in-out infinite" }
          : { opacity: state === "pending" ? 0.5 : 1 }),
      }}
    />
  );
}

/**
 * The six import phases with their real daemon counts. Shared by the import
 * view and the Home detail panel — one rendering, never a fork.
 *
 * A phase with no known total reads as waiting, never as 0%. `distill` never
 * reports a usable total, so it always renders as a live count with no bar.
 */
export function ImportPhaseList({ phases }: { phases: ImportPhaseStatus[] }) {
  const { t } = useTranslation();
  const byPhase = new Map<ImportPhase, ImportPhaseStatus>();
  for (const p of phases) byPhase.set(p.phase, p);

  return (
    <div style={{ width: "100%", display: "flex", flexDirection: "column", gap: "4px" }}>
      <PulseStyle />
      {IMPORT_PHASE_ORDER.map((phase) => {
        const entry = byPhase.get(phase) ?? emptyPhase(phase);
        const isDistill = phase === "distill";
        const hasTotal = !isDistill && entry.total > 0;
        const fraction = hasTotal ? Math.min(entry.done / entry.total, 1) : 0;
        return (
          <div
            key={phase}
            data-testid={`import-phase-${phase}`}
            data-state={entry.state}
            style={{
              display: "flex",
              alignItems: "center",
              gap: "14px",
              padding: "10px 14px",
              borderRadius: "10px",
              backgroundColor: entry.state === "running" ? "var(--mem-surface)" : "transparent",
              border: entry.state === "running"
                ? "1px solid var(--mem-border)"
                : "1px solid transparent",
              opacity: entry.state === "pending" ? 0.55 : 1,
            }}
          >
            <div style={{
              width: "32px",
              height: "32px",
              borderRadius: "8px",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              flexShrink: 0,
            }}>
              <PhaseDot state={entry.state} />
            </div>

            <span style={{
              fontFamily: "var(--mem-font-body)",
              fontSize: "13px",
              fontWeight: entry.state === "running" ? 500 : 400,
              color: entry.state === "failed" ? "#ef4444" : "var(--mem-text)",
            }}>
              {t(`importBatch.phases.${phase}`)}
            </span>

            <span style={{
              marginLeft: "auto",
              display: "flex",
              alignItems: "center",
              gap: "10px",
              fontFamily: "var(--mem-font-mono)",
              fontSize: "11px",
              color: "var(--mem-text-tertiary)",
              whiteSpace: "nowrap",
            }}>
              {entry.state === "failed" && (
                <span style={{ color: "#ef4444" }}>{t("importBatch.states.failed")}</span>
              )}
              {isDistill ? (
                <span>{t("importBatch.pagesSoFar", { count: entry.done })}</span>
              ) : hasTotal ? (
                <span>{t("importBatch.countOf", { done: entry.done, total: entry.total })}</span>
              ) : (
                <span>{t("importBatch.states.waiting")}</span>
              )}
              {entry.failed > 0 && (
                <span style={{ color: "#ef4444" }}>
                  {t("importBatch.failedUnits", { count: entry.failed })}
                </span>
              )}
            </span>

            {hasTotal && (
              <div
                role="progressbar"
                aria-label={t(`importBatch.phases.${phase}`)}
                aria-valuemin={0}
                aria-valuemax={entry.total}
                aria-valuenow={entry.done}
                style={{
                  width: "48px",
                  height: "3px",
                  borderRadius: "2px",
                  backgroundColor: "var(--mem-border)",
                  overflow: "hidden",
                  flexShrink: 0,
                }}
              >
                <div style={{
                  height: "100%",
                  borderRadius: "2px",
                  backgroundColor: entry.state === "failed" ? "#ef4444" : "var(--mem-accent-indigo)",
                  width: `${fraction * 100}%`,
                  transition: "width 0.3s ease-out",
                }} />
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

/**
 * One header pill summarising the background phases of every active batch.
 * Renders nothing when no batch is active.
 */
export function ImportStatusPill({
  batches,
  onOpen,
}: {
  batches: ImportBatchStatus[];
  onOpen: () => void;
}) {
  const { t } = useTranslation();
  if (batches.length === 0) return null;
  const summary = summarizeImportBatches(batches);
  const failed = summary.failedPhases.length > 0;
  const nextPhase = summary.runningPhases[0] ?? summary.failedPhases[0];

  return (
    <>
    <PulseStyle />
    <button
      type="button"
      data-testid="import-status-pill"
      onClick={onOpen}
      aria-label={t("importBatch.openDetail")}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: "6px",
        fontFamily: "var(--mem-font-mono)",
        fontVariantNumeric: "tabular-nums",
        fontSize: 11,
        fontWeight: 400,
        borderRadius: 999,
        padding: "1px 8px",
        border: "none",
        cursor: "pointer",
        color: failed ? "#ef4444" : "var(--mem-accent-indigo)",
        backgroundColor: failed
          ? "color-mix(in srgb, #ef4444 12%, transparent)"
          : "var(--mem-indigo-bg)",
        whiteSpace: "nowrap",
      }}
    >
      <span
        aria-hidden="true"
        style={{
          width: "6px",
          height: "6px",
          borderRadius: "50%",
          backgroundColor: "currentColor",
          animation: "pulse-subtle 2s ease-in-out infinite",
        }}
      />
      {failed ? t("importBatch.pillFailed") : t("importBatch.pillActive")}
      {nextPhase && (
        <span style={{ opacity: 0.75 }}>{t(`importBatch.phases.${nextPhase}`)}</span>
      )}
    </button>
    </>
  );
}

/**
 * The pill's detail view: per-phase progress plus the live totals, with a
 * Back control returning to Home.
 */
export function ImportDetailPanel({
  batches,
  onBack,
}: {
  batches: ImportBatchStatus[];
  onBack: () => void;
}) {
  const { t } = useTranslation();
  const summary = summarizeImportBatches(batches);

  return (
    <div data-testid="import-detail-panel" style={{ display: "flex", flexDirection: "column", gap: "16px" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
        <button
          type="button"
          data-testid="import-detail-back"
          onClick={onBack}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: "4px",
            background: "none",
            border: "none",
            cursor: "pointer",
            fontFamily: "var(--mem-font-body)",
            fontSize: "13px",
            color: "var(--mem-text-secondary)",
            padding: 0,
          }}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true">
            <path d="M19 12H5M12 19l-7-7 7-7" />
          </svg>
          {t("main.back")}
        </button>
        <h2 style={{
          fontFamily: "var(--mem-font-heading)",
          fontSize: 18,
          fontWeight: 500,
          color: "var(--mem-text)",
          margin: 0,
        }}>
          {t("importBatch.title")}
        </h2>
      </div>

      <ImportPhaseList phases={summary.phases} />

      <p style={{
        fontFamily: "var(--mem-font-mono)",
        fontSize: "11px",
        color: "var(--mem-text-tertiary)",
        margin: 0,
      }}>
        {t("importBatch.summaryImported", {
          count: summary.memoriesImported,
          source: importSourceLabel(batches[0]?.source ?? ""),
        })}
        {summary.memoriesSkipped > 0 && ` · ${t("importBatch.summarySkipped", { count: summary.memoriesSkipped })}`}
      </p>
    </div>
  );
}
