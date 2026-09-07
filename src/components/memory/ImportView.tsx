import { useState, useRef, useCallback } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  importMemories,
  clipboardWrite,
  IMPORT_CHUNK_SIZE,
  type ImportResult,
} from "../../lib/tauri";
import { IMPORT_SOURCE_LABELS, ImportPhaseList, useImportBatchStatus } from "./ImportPhases";

type Source = "chatgpt" | "claude" | "other";

interface ImportViewProps {
  onBack: () => void;
  onComplete: (source: string, result: ImportResult) => void;
  /** When true, show "Continue" instead of "View memories" / "Import more" in summary. */
  wizardMode?: boolean;
  /** Called when internal phase changes. */
  onPhaseChange?: (phase: Phase) => void;
  /** When provided, shows a skip link in the footer. */
  onSkip?: () => void;
  /** Optional onboarding copy rendered above the import form. */
  wizardHint?: React.ReactNode;
}

type Phase = "input" | "progress" | "summary";

/**
 * Split pasted lines into upload chunks. One memory per non-empty line; empty
 * lines and separators never reach the daemon.
 */
export function chunkImportText(text: string, chunkSize: number = IMPORT_CHUNK_SIZE): string[] {
  const lines = text.split("\n").filter((l) => l.trim() !== "");
  const chunks: string[] = [];
  for (let i = 0; i < lines.length; i += chunkSize) {
    chunks.push(lines.slice(i, i + chunkSize).join("\n"));
  }
  return chunks;
}

const EXPORT_PROMPT = `Export all of my stored memories and any context you've learned about me. Preserve my words verbatim where possible.

EVERY line MUST follow this exact format — no exceptions:
[TYPE] - content

TYPE must be exactly one of: identity, preference, decision, lesson, gotcha, fact

Where:
- identity = who I am (name, location, education, family, languages)
- preference = how I like things (opinions, tastes, working style, rules like "always do X" or "never do Y")
- decision = choices I made with rationale (tech, career, project directions)
- lesson = reusable learnings from experience
- gotcha = pitfalls, traps, or things to avoid
- fact = things about my work, projects, skills, situation

Example output:
[identity] - Lives in San Francisco, originally from Taiwan
[preference] - Prefers concise responses without trailing summaries
[decision] - Chose Rust + Tauri for the desktop app over Electron
[preference] - Never use emojis unless explicitly asked
[lesson] - TDD caught the config regression before launch
[gotcha] - Tauri rolling::daily suffixes file names with the date
[fact] - Building a local-first AI memory layer called Wenlan

Rules:
- NO section headers, category labels, or grouping text
- NO explanations before or after — ONLY the tagged lines
- One memory per line
- Wrap entire output in a single code block`;

const SOURCE_LABELS: Record<Source, string> = IMPORT_SOURCE_LABELS;

// Aligned with Wenlan's --mem-accent-* palette (see FACET_COLORS in tauri.ts)
const TYPE_BADGE_STYLES: Record<string, { bg: string; text: string }> = {
  identity: { bg: "color-mix(in srgb, var(--mem-accent-indigo) 15%, transparent)", text: "var(--mem-accent-indigo)" },
  preference: { bg: "color-mix(in srgb, var(--mem-accent-warm) 15%, transparent)", text: "var(--mem-accent-warm)" },
  fact: { bg: "color-mix(in srgb, var(--mem-accent-glow) 12%, transparent)", text: "var(--mem-accent-glow)" },
  decision: { bg: "color-mix(in srgb, var(--mem-accent-amber) 15%, transparent)", text: "var(--mem-accent-amber)" },
  lesson: { bg: "color-mix(in srgb, var(--mem-accent-sage) 15%, transparent)", text: "var(--mem-accent-sage)" },
  gotcha: { bg: "color-mix(in srgb, #ef4444 15%, transparent)", text: "#ef4444" },
  goal: { bg: "color-mix(in srgb, var(--mem-accent-sage) 15%, transparent)", text: "var(--mem-accent-sage)" },
};

export function ImportView({ onBack, onComplete, wizardMode, onPhaseChange, onSkip, wizardHint }: ImportViewProps) {
  const queryClient = useQueryClient();
  const { t } = useTranslation();
  const [phase, setPhaseRaw] = useState<Phase>("input");
  const setPhase = useCallback((p: Phase) => {
    setPhaseRaw(p);
    onPhaseChange?.(p);
  }, [onPhaseChange]);
  const [source, setSource] = useState<Source>("chatgpt");
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<ImportResult | null>(null);
  const [batchId, setBatchId] = useState<string | null>(null);
  const [chunkProgress, setChunkProgress] = useState<{ done: number; total: number } | null>(null);
  const [promptCopied, setPromptCopied] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);
  const batchStatus = useImportBatchStatus(batchId);

  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (ev) => {
      const content = ev.target?.result;
      if (typeof content === "string") {
        setText(content);
      }
    };
    reader.readAsText(file);
    // Reset so the same file can be re-selected
    e.target.value = "";
  };

  const handleImport = async () => {
    setError(null);
    const chunks = chunkImportText(text);
    // One batch id for the whole import, shared by every chunk, so the
    // daemon can report one honest aggregate under import_batch_status_cmd.
    const id = crypto.randomUUID();
    setBatchId(id);
    setChunkProgress({ done: 0, total: chunks.length });
    setPhase("progress");
    try {
      const agg: ImportResult = {
        imported: 0,
        skipped: 0,
        breakdown: {},
        entities_created: 0,
        observations_added: 0,
        relations_created: 0,
        batch_id: id,
      };
      for (let i = 0; i < chunks.length; i++) {
        const res = await importMemories(source, chunks[i]!, undefined, {
          batchId: id,
          chunkIndex: i,
          chunkTotal: chunks.length,
        });
        agg.imported += res.imported;
        agg.skipped += res.skipped;
        agg.entities_created += res.entities_created;
        agg.observations_added += res.observations_added;
        agg.relations_created += res.relations_created;
        for (const [type, count] of Object.entries(res.breakdown ?? {})) {
          agg.breakdown[type] = (agg.breakdown[type] ?? 0) + count;
        }
        setChunkProgress({ done: i + 1, total: chunks.length });
      }
      setResult(agg);
      // The request phases (ingest, store) are done here; detect and later
      // keep running in the background. The summary says so honestly while
      // the status poll keeps the live counts climbing.
      setPhase("summary");
      queryClient.invalidateQueries();
    } catch (err) {
      setError(String(err));
      setBatchId(null);
      setChunkProgress(null);
      setPhase("input");
    }
  };

  const handleReset = () => {
    setPhase("input");
    setText("");
    setError(null);
    setResult(null);
    setBatchId(null);
    setChunkProgress(null);
  };

  // ── Input form ──────────────────────────────────────────────────────
  if (phase === "input") {
    return (
      <div className="flex flex-col mx-auto py-4" style={{ height: "calc(100vh - 120px)", maxWidth: "672px" }}>
        {/* Header row: back + title + source pills */}
        <div className="flex items-center gap-4 mb-4 shrink-0">
          <button
            onClick={onBack}
            className="flex items-center gap-1 shrink-0 transition-colors duration-150"
            style={{
              fontFamily: "var(--mem-font-body)",
              fontSize: "13px",
              color: "var(--mem-text-secondary)",
            }}
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M19 12H5M12 19l-7-7 7-7" />
            </svg>
          </button>
          <h1
            className="shrink-0"
            style={{
              fontFamily: "var(--mem-font-heading)",
              fontSize: "18px",
              fontWeight: 500,
              color: "var(--mem-text)",
            }}
          >
            Import Memories
          </h1>
          <div className="flex gap-1 ml-auto shrink-0">
            {(["chatgpt", "claude", "other"] as Source[]).map((s) => (
              <button
                key={s}
                onClick={() => setSource(s)}
                className="px-2.5 py-1 rounded-md text-xs font-medium transition-colors duration-150"
                style={{
                  fontFamily: "var(--mem-font-body)",
                  backgroundColor: source === s ? "var(--mem-accent-indigo)" : "transparent",
                  color: source === s ? "white" : "var(--mem-text-tertiary)",
                }}
              >
                {SOURCE_LABELS[s]}
              </button>
            ))}
          </div>
        </div>

        {wizardMode && wizardHint && (
          <div
            className="mb-4 rounded-lg px-3 py-2 shrink-0"
            style={{
              backgroundColor: "var(--mem-hover)",
              border: "1px solid var(--mem-border)",
              fontFamily: "var(--mem-font-body)",
              fontSize: "12px",
              color: "var(--mem-text-secondary)",
              lineHeight: "1.5",
            }}
          >
            {wizardHint}
          </div>
        )}

        {/* Two equal panels */}
        <div className="flex flex-col gap-3 flex-1 min-h-0">
          {/* Top panel: export prompt or instructions */}
          <div className="flex flex-col flex-1 min-h-0">
            <div className="flex items-center justify-between mb-1.5 shrink-0">
              <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", fontWeight: 500, color: "var(--mem-text-secondary)" }}>
                {source !== "other" ? "Export prompt" : "Instructions"}
              </span>
              {source !== "other" && (
                <button
                  onClick={() => {
                    clipboardWrite(EXPORT_PROMPT);
                    setPromptCopied(true);
                    setTimeout(() => setPromptCopied(false), 2000);
                  }}
                  className="px-2.5 py-0.5 rounded-md text-xs font-medium transition-colors"
                  style={{
                    fontFamily: "var(--mem-font-body)",
                    color: promptCopied ? "var(--mem-accent-warm)" : "var(--mem-accent-indigo)",
                    backgroundColor: promptCopied ? "rgba(251, 191, 36, 0.15)" : "rgba(123, 123, 232, 0.1)",
                  }}
                >
                  {promptCopied ? "Copied!" : "Copy prompt"}
                </button>
              )}
            </div>
            <pre
              className="rounded-lg px-3 py-2 overflow-auto text-[11px] leading-relaxed flex-1 min-h-0"
              style={{
                fontFamily: "var(--mem-font-mono)",
                backgroundColor: "var(--mem-hover)",
                color: "var(--mem-text-tertiary)",
                border: "1px solid var(--mem-border)",
                whiteSpace: "pre-wrap",
                margin: 0,
              }}
            >
              {source !== "other"
                ? `Copy this prompt, paste into ${SOURCE_LABELS[source]}, then paste the output below.\n\n${EXPORT_PROMPT}`
                : "Paste any list of facts or memories, one per line.\n\nEach line becomes a separate memory in Wenlan.\nEmpty lines and separators (---, ===) are skipped.\n\nOptionally prefix lines with a type tag:\n[identity] - Lives in San Francisco\n[preference] - Prefers concise responses\n[lesson] - TDD caught a config regression before launch\n[gotcha] - Tauri rolling::daily suffixes file names with the date\n[fact] - Building Wenlan, a local-first AI memory app\n\nLines without a tag are stored as facts."}
            </pre>
          </div>

          {/* Bottom panel: paste area */}
          <div className="flex flex-col flex-1 min-h-0">
            <div className="flex items-center justify-between mb-1.5 shrink-0">
              <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", fontWeight: 500, color: "var(--mem-text-secondary)" }}>
                Paste output
              </span>
              {text && (
                <span style={{ fontFamily: "var(--mem-font-mono)", fontSize: "11px", color: "var(--mem-text-tertiary)" }}>
                  {text.split("\n").filter((l) => l.trim()).length} lines
                </span>
              )}
            </div>
            <textarea
              value={text}
              onChange={(e) => setText(e.target.value)}
              placeholder="Paste your memories here, one per line..."
              className="w-full rounded-lg px-3 py-2 focus:outline-none focus:ring-1 focus:ring-[var(--mem-accent-indigo)]/40 flex-1 min-h-0"
              style={{
                fontFamily: "var(--mem-font-body)",
                fontSize: "13px",
                color: "var(--mem-text)",
                backgroundColor: "var(--mem-surface)",
                border: "1px solid var(--mem-border)",
                resize: "none",
                lineHeight: "1.6",
              }}
            />
          </div>
        </div>

        {/* Footer: upload + skip + error + import */}
        <div className="flex items-center gap-3 mt-3 shrink-0">
          <button
            onClick={() => fileRef.current?.click()}
            className="transition-colors"
            style={{
              fontFamily: "var(--mem-font-body)",
              fontSize: "12px",
              color: "var(--mem-accent-indigo)",
            }}
          >
            Upload file
          </button>
          <input ref={fileRef} type="file" accept=".txt,.csv,.json" className="hidden" onChange={handleFileUpload} />
          {error && (
            <span style={{ fontFamily: "var(--mem-font-body)", fontSize: "12px", color: "#ef4444" }}>
              {error}
            </span>
          )}
          <div className="flex items-center gap-3 ml-auto">
            {onSkip && (
              <button
                onClick={onSkip}
                className="transition-colors duration-150"
                style={{
                  fontFamily: "var(--mem-font-body)",
                  fontSize: "13px",
                  color: "var(--mem-text-tertiary)",
                  background: "none",
                  border: "none",
                  cursor: "pointer",
                }}
              >
                Skip
              </button>
            )}
            <button
              onClick={handleImport}
              disabled={!text.trim()}
              className="px-4 py-2 rounded-lg text-sm font-medium transition-colors duration-150 disabled:opacity-40 disabled:cursor-not-allowed"
              style={{
                fontFamily: "var(--mem-font-body)",
                backgroundColor: "var(--mem-accent-indigo)",
                color: "white",
              }}
            >
              Import
            </button>
          </div>
        </div>
      </div>
    );
  }

  // ── Progress: live daemon row counts, never a timer ───────────────
  if (phase === "progress") {
    const lineCount = chunkImportText(text).length;
    return (
      <div className="flex flex-col items-center max-w-md mx-auto py-16" style={{ gap: "32px" }}>
        <div className="text-center" style={{ gap: "8px", display: "flex", flexDirection: "column" }}>
          <p style={{
            fontFamily: "var(--mem-font-heading)",
            fontSize: "28px",
            fontWeight: 400,
            color: "var(--mem-text)",
            letterSpacing: "-0.02em",
          }}>
            {t("importBatch.progressTitle", { count: lineCount })}
          </p>
          {chunkProgress && chunkProgress.total > 1 && (
            <p style={{
              fontFamily: "var(--mem-font-body)",
              fontSize: "13px",
              color: "var(--mem-text-tertiary)",
            }}>
              {t("importBatch.chunkProgress", { done: Math.min(chunkProgress.done + 1, chunkProgress.total), total: chunkProgress.total })}
            </p>
          )}
        </div>

        <ImportPhaseList phases={batchStatus?.phases ?? []} />
      </div>
    );
  }

  // ── Summary: real figures, honest about background work ─────────────
  if (phase === "summary" && result) {
    const breakdownEntries = Object.entries(result.breakdown).filter(
      ([, count]) => count > 0,
    );
    // Live daemon counts once the first status poll lands; the chunk
    // responses before that. Either way these are measured rows, and the
    // note below says which phases are still moving them.
    const imported = batchStatus?.memories_imported ?? result.imported;
    const skipped = batchStatus?.memories_skipped ?? result.skipped;
    const runningNames = (batchStatus?.phases ?? [])
      .filter((p) => p.state === "running" || p.state === "pending")
      .map((p) => t(`importBatch.phases.${p.phase}`));
    // The daemon's `complete` flag is the source of truth for whether the
    // numbers are still moving — never a phase row read in isolation.

    return (
      <div className="flex flex-col gap-6 max-w-2xl mx-auto py-4">
        {/* Summary card */}
        <div
          className="rounded-xl px-6 py-5"
          style={{
            backgroundColor: "var(--mem-surface)",
            border: "1px solid var(--mem-border)",
          }}
        >
          {/* Main stat */}
          <div className="flex items-center gap-3 mb-4">
            <div
              className="w-10 h-10 rounded-full flex items-center justify-center"
              style={{ backgroundColor: "rgba(99, 102, 241, 0.15)" }}
            >
              <svg
                className="w-5 h-5"
                fill="none"
                stroke="rgb(99, 102, 241)"
                viewBox="0 0 24 24"
                strokeWidth="2"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M5 13l4 4L19 7"
                />
              </svg>
            </div>
            <div>
              <p
                style={{
                  fontFamily: "var(--mem-font-heading)",
                  fontSize: "16px",
                  fontWeight: 500,
                  color: "var(--mem-text)",
                }}
              >
                {t("importBatch.summaryImported", { count: imported, source: SOURCE_LABELS[source] })}
              </p>
              {skipped > 0 && (
                <p
                  style={{
                    fontFamily: "var(--mem-font-body)",
                    fontSize: "12px",
                    color: "var(--mem-text-tertiary)",
                    marginTop: "2px",
                  }}
                >
                  {t("importBatch.summarySkipped", { count: skipped })}
                </p>
              )}
            </div>
          </div>

          {/* Type breakdown badges */}
          {breakdownEntries.length > 0 && (
            <div className="flex flex-wrap gap-2 mb-4">
              {breakdownEntries.map(([type, count]) => {
                const style = TYPE_BADGE_STYLES[type] ?? {
                  bg: "var(--mem-hover)",
                  text: "var(--mem-text-secondary)",
                };
                return (
                  <span
                    key={type}
                    className="inline-flex items-center gap-1 px-2 py-1 rounded-full text-xs font-medium"
                    style={{
                      backgroundColor: style.bg,
                      color: style.text,
                      fontFamily: "var(--mem-font-body)",
                    }}
                  >
                    {type}
                    <span style={{ opacity: 0.7 }}>{count}</span>
                  </span>
                );
              })}
            </div>
          )}

          {/* Live enrichment figures from the daemon */}
          {batchStatus && (
            <div
              className="flex flex-wrap gap-x-4 gap-y-1 mb-4"
              style={{
                fontFamily: "var(--mem-font-mono)",
                fontSize: "11px",
                color: "var(--mem-text-tertiary)",
              }}
            >
              <span>{t("importBatch.entitiesDetected", { count: batchStatus.entities_detected })}</span>
              <span>{t("importBatch.entitiesEstablished", { count: batchStatus.entities_established })}</span>
              <span>{t("importBatch.pagesDistilled", { count: batchStatus.pages_distilled })}</span>
            </div>
          )}

          {/* Honesty note: background phases keep moving these numbers */}
          {batchStatus && (
            <p
              style={{
                fontFamily: "var(--mem-font-body)",
                fontSize: "12px",
                color: "var(--mem-text-secondary)",
                lineHeight: "1.5",
                margin: 0,
              }}
            >
              {!batchStatus.complete && runningNames.length > 0
                ? t("importBatch.backgroundRunning", { phases: runningNames.join(", ") })
                : t("importBatch.backgroundSettled")}
            </p>
          )}
        </div>

        {/* Actions */}
        <div className={`flex items-center gap-3 ${wizardMode ? "justify-center" : ""}`}>
          <button
            onClick={() => onComplete(source, result!)}
            className="px-6 py-2 rounded-lg text-sm font-medium transition-colors duration-150"
            style={{
              fontFamily: "var(--mem-font-body)",
              backgroundColor: "var(--mem-accent-indigo)",
              color: "white",
            }}
          >
            {wizardMode ? "Continue" : "View memories"}
          </button>
          {!wizardMode && (
            <button
              onClick={handleReset}
              className="px-4 py-2 rounded-lg text-sm font-medium transition-colors duration-150"
              style={{
                fontFamily: "var(--mem-font-body)",
                backgroundColor: "var(--mem-hover)",
                color: "var(--mem-text-secondary)",
                border: "1px solid var(--mem-border)",
              }}
            >
              Import more
            </button>
          )}
        </div>
      </div>
    );
  }

  return null;
}

export default ImportView;
