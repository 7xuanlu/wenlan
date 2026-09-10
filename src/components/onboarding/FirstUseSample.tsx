// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  ArrowCounterClockwise,
  ArrowLeft,
  ArrowRight,
  BookOpenText,
  Brain,
  Check,
  Copy,
  FileText,
  LinkSimple,
  Pause,
  Play,
  SkipForward,
  X,
} from "@phosphor-icons/react";
import { clipboardWrite } from "../../lib/tauri";
import {
  getSampleData,
  resolveSampleLocale,
  type SampleCitation,
  type SampleClient,
  type SamplePageParagraph,
  type SampleSource,
  type SampleSourceType,
} from "./firstUseSampleData";
import "./firstUseGuide.css";

export interface FirstUseSampleProps {
  onBackToGuide: () => void;
  onBringData: () => void;
  onConnect: () => void;
}

type SamplePhase = 0 | 1 | 2 | 3;

const FINAL_PHASE: SamplePhase = 3;
/** Demo-only autoplay cadence. The live view never uses timers. */
const SAMPLE_PHASE_MS = 3000;

type CopyKey = "recall" | "handoff" | "brief";

type SampleDialogState =
  | { kind: "source"; sourceId: string; citationId: string | null }
  | { kind: "related" };

const FOCUSABLE_SELECTOR =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

function SourceGlyph({ type }: { type: SampleSourceType }) {
  if (type === "memory") return <Brain aria-hidden="true" weight="regular" />;
  if (type === "page") return <BookOpenText aria-hidden="true" weight="regular" />;
  return <FileText aria-hidden="true" weight="regular" />;
}

function HighlightedLine({ line, quote }: { line: string; quote: string }) {
  const index = quote ? line.indexOf(quote) : -1;
  if (index < 0) return <>{line}</>;
  return (
    <>
      {line.slice(0, index)}
      <mark>{quote}</mark>
      {line.slice(index + quote.length)}
    </>
  );
}

/**
 * Accessible modal: focuses the dialog on open, traps Tab/Shift+Tab inside,
 * closes on Escape, restores the trigger's focus on close. The caller renders
 * the background with `inert` while a dialog is open.
 */
function SampleDialog({
  labelId,
  onClose,
  testId,
  sourceType,
  restoreFocusTo,
  children,
}: {
  labelId: string;
  onClose: () => void;
  testId: string;
  sourceType?: SampleSourceType;
  restoreFocusTo: HTMLElement | null;
  children: ReactNode;
}) {
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const restoreRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    restoreRef.current = restoreFocusTo ??
      (document.activeElement instanceof HTMLElement ? document.activeElement : null);
    const node = dialogRef.current;
    node?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key !== "Tab" || !node) return;
      const items = Array.from(
        node.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR),
      );
      if (items.length === 0) {
        event.preventDefault();
        return;
      }
      const first = items[0];
      const last = items[items.length - 1];
      if (event.shiftKey && (document.activeElement === first || document.activeElement === node || !node.contains(document.activeElement))) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && (document.activeElement === last || document.activeElement === node || !node.contains(document.activeElement))) {
        event.preventDefault();
        first.focus();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      restoreRef.current?.focus();
    };
  }, [onClose, restoreFocusTo]);

  return (
    <div
      className="fus-overlay"
      role="presentation"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelId}
        tabIndex={-1}
        data-testid={testId}
        data-source-type={sourceType}
        className="fus-dialog"
      >
        {children}
      </div>
    </div>
  );
}

export function FirstUseSample({ onBackToGuide, onBringData, onConnect }: FirstUseSampleProps) {
  const { t, i18n } = useTranslation();
  const data = getSampleData(resolveSampleLocale(i18n.language));
  const { sources, citations, page } = data;

  const [phase, setPhase] = useState<SamplePhase>(0);
  const [playing, setPlaying] = useState<boolean>(() => !prefersReducedMotion());
  const [reducedMotion, setReducedMotion] = useState<boolean>(() => prefersReducedMotion());
  const [dialog, setDialog] = useState<SampleDialogState | null>(null);
  const dialogTrigger = useRef<HTMLElement | null>(null);
  const [aiOpen, setAiOpen] = useState(false);
  const [client, setClient] = useState<SampleClient>("chatgpt");
  const [copiedKey, setCopiedKey] = useState<CopyKey | null>(null);
  const [copyFailedKey, setCopyFailedKey] = useState<CopyKey | null>(null);

  // Stale-copy guard: only the latest copy request may report success or
  // failure. Client switches, replay, and unmount invalidate pending ones.
  const copySeq = useRef(0);
  const mountedRef = useRef(true);
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      copySeq.current += 1;
    };
  }, []);

  // Demo-only autoplay: ~3 s per phase, cleaned up on every change/unmount.
  // Reduced-motion users get a fully manual walkthrough instead.
  useEffect(() => {
    if (!playing || reducedMotion || phase >= FINAL_PHASE) return;
    const timer = window.setTimeout(() => {
      const next = (phase + 1) as SamplePhase;
      setPhase(next);
      if (next >= FINAL_PHASE) setPlaying(false);
    }, SAMPLE_PHASE_MS);
    return () => window.clearTimeout(timer);
  }, [playing, phase, reducedMotion]);

  // Follow the OS motion preference live; dropping into reduced motion stops
  // playback and removes the nonfunctional Play control.
  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
      return;
    }
    const query = window.matchMedia("(prefers-reduced-motion: reduce)");
    const onChange = () => {
      const reduced = query.matches;
      setReducedMotion(reduced);
      if (reduced) setPlaying(false);
    };
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  const selectPhase = (next: SamplePhase) => {
    setPhase(next);
    setPlaying(false);
  };

  const replay = () => {
    copySeq.current += 1;
    setCopiedKey(null);
    setCopyFailedKey(null);
    setDialog(null);
    setAiOpen(false);
    setPhase(0);
    setPlaying(!prefersReducedMotion());
  };

  const closeDialog = useCallback(() => setDialog(null), []);

  const copyCommand = async (key: CopyKey, value: string) => {
    const seq = copySeq.current + 1;
    copySeq.current = seq;
    setCopiedKey(null);
    setCopyFailedKey(null);
    try {
      await clipboardWrite(value);
    } catch {
      // A clipboard rejection is reported as a failure, never as success —
      // and only when this request is still the latest one.
      if (!mountedRef.current || copySeq.current !== seq) return;
      setCopyFailedKey(key);
      return;
    }
    if (!mountedRef.current || copySeq.current !== seq) return;
    setCopiedKey(key);
  };

  const switchClient = (next: SampleClient) => {
    copySeq.current += 1;
    setClient(next);
    setCopiedKey(null);
    setCopyFailedKey(null);
  };

  const citationById = (id: string): SampleCitation | undefined =>
    citations.find((entry) => entry.id === id);

  const sourceById = (id: string): SampleSource | undefined =>
    sources.find((source) => source.id === id);

  const keyPointFor = (source: SampleSource): string =>
    citations.find((entry) => entry.sourceId === source.id)?.quote ?? source.excerpt;

  const paragraphSources = (paragraph: SamplePageParagraph): SampleSource[] => {
    const seen = new Set<string>();
    const result: SampleSource[] = [];
    for (const citationId of paragraph.citationIds) {
      const entry = citationById(citationId);
      const source = entry ? sourceById(entry.sourceId) : undefined;
      if (source && !seen.has(source.id)) {
        seen.add(source.id);
        result.push(source);
      }
    }
    return result;
  };

  const openSource = (sourceId: string, citationId: string | null) =>
    setDialog({ kind: "source", sourceId, citationId });

  const dialogSource = dialog?.kind === "source" ? sourceById(dialog.sourceId) : undefined;
  const dialogCitation =
    dialog?.kind === "source" && dialog.citationId ? citationById(dialog.citationId) : undefined;

  const commands = data.commands[client];
  const clientTabs: Array<{ id: SampleClient; label: string }> = [
    { id: "chatgpt", label: t("firstUse.sample.clientChatgpt") },
    { id: "codex", label: t("firstUse.sample.clientCodex") },
    { id: "claude", label: t("firstUse.sample.clientClaude") },
  ];

  const phaseNames = [
    t("firstUse.sample.phaseImport"),
    t("firstUse.sample.phaseExtract"),
    t("firstUse.sample.phaseConnect"),
    t("firstUse.sample.phaseForm"),
  ];
  const phaseBodies = [
    t("firstUse.sample.phaseImportBody"),
    t("firstUse.sample.phaseExtractBody"),
    t("firstUse.sample.phaseConnectBody"),
    t("firstUse.sample.phaseFormBody"),
  ];
  const showPlay = !reducedMotion && !playing && phase < FINAL_PHASE;

  return (
    <>
      <div
        data-testid="first-use-sample"
        className="fug-sample"
        inert={dialog ? true : undefined}
        onClickCapture={(event) => {
          // macOS WebKit does not necessarily focus a clicked button.
          const trigger = event.target instanceof Element
            ? event.target.closest('button[aria-haspopup="dialog"]') : null;
          if (trigger instanceof HTMLElement) dialogTrigger.current = trigger;
        }}
      >
        <div className="fug-row">
          <button type="button" className="fug-back" onClick={onBackToGuide}>
            <ArrowLeft aria-hidden="true" weight="regular" />
            {t("firstUse.sample.back")}
          </button>
          <span className="fus-animation-label">{t("firstUse.sample.animationLabel")}</span>
        </div>

        <ol className="fus-phases" aria-label={t("firstUse.sample.animationLabel")}>
          {phaseNames.map((name, index) => {
            const id = index as SamplePhase;
            return (
              <li key={name}>
                <button
                  type="button"
                  data-testid={`sample-phase-${index}`}
                  data-active={phase === id}
                  data-complete={phase > id}
                  className={phase === id ? "fus-phase is-active" : "fus-phase"}
                  aria-current={phase === id ? "step" : undefined}
                  onClick={() => selectPhase(id)}
                >
                  <span className="fus-phase-number" aria-hidden="true">
                    {phase > id ? <Check weight="bold" /> : index + 1}
                  </span>
                  {name}
                </button>
              </li>
            );
          })}
        </ol>
        <p className="fus-phase-body">{phaseBodies[phase]}</p>

        <div className="fus-controls">
          {playing ? (
            <button
              type="button"
              className="fug-button-secondary"
              onClick={() => setPlaying(false)}
              aria-label={t("firstUse.sample.pause")}
            >
              <Pause aria-hidden="true" weight="regular" />
              {t("firstUse.sample.pause")}
            </button>
          ) : null}
          {showPlay ? (
            <button
              type="button"
              className="fug-button-secondary"
              onClick={() => setPlaying(true)}
              aria-label={t("firstUse.sample.play")}
            >
              <Play aria-hidden="true" weight="regular" />
              {t("firstUse.sample.play")}
            </button>
          ) : null}
          <button
            type="button"
            className="fug-button-secondary"
            onClick={replay}
            aria-label={t("firstUse.sample.replay")}
          >
            <ArrowCounterClockwise aria-hidden="true" weight="regular" />
            {t("firstUse.sample.replay")}
          </button>
          {phase < FINAL_PHASE ? (
            <button
              type="button"
              className="fug-button-secondary"
              onClick={() => selectPhase((phase + 1) as SamplePhase)}
              aria-label={t("firstUse.sample.next")}
            >
              {t("firstUse.sample.next")}
              <ArrowRight aria-hidden="true" weight="regular" />
            </button>
          ) : null}
          {phase < FINAL_PHASE ? (
            <button
              type="button"
              className="fug-button-secondary"
              onClick={() => selectPhase(FINAL_PHASE)}
              aria-label={t("firstUse.sample.skipToResult")}
            >
              <SkipForward aria-hidden="true" weight="regular" />
              {t("firstUse.sample.skipToResult")}
            </button>
          ) : null}
        </div>

        {aiOpen ? (
          <section className="fus-use" aria-labelledby="fus-use-title">
            <button type="button" className="fug-back" onClick={() => setAiOpen(false)}>
              <ArrowLeft aria-hidden="true" weight="regular" />
              {t("firstUse.sample.backToPage")}
            </button>
            <h2 id="fus-use-title">{t("firstUse.sample.useTitle")}</h2>
            <p>{t("firstUse.sample.useBody")}</p>
            <p className="fus-note">{t("firstUse.sample.connectHint")}</p>
            <div className="fus-cta-row">
              <button type="button" className="fug-button-primary" onClick={onConnect}>
                {t("firstUse.sample.connectCta")}
              </button>
            </div>
            <div className="fus-tabs" role="tablist" aria-label={t("firstUse.sample.useTitle")}>
              {clientTabs.map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  role="tab"
                  aria-selected={client === tab.id}
                  className={client === tab.id ? "fus-tab is-selected" : "fus-tab"}
                  onClick={() => switchClient(tab.id)}
                >
                  {tab.label}
                </button>
              ))}
            </div>
            <div className="fus-command" role="tabpanel">
              <p className="fus-command-label">{t("firstUse.sample.recallLabel")}</p>
              <div className="fus-command-row">
                <code>{commands.recall}</code>
                <button
                  type="button"
                  className="fus-copy"
                  onClick={() => copyCommand("recall", commands.recall)}
                >
                  {copiedKey === "recall" ? (
                    <Check aria-hidden="true" weight="bold" data-testid="copy-ok" />
                  ) : (
                    <Copy aria-hidden="true" weight="regular" />
                  )}
                  {copiedKey === "recall"
                    ? t("firstUse.sample.copied")
                    : t("firstUse.sample.copyCommand")}
                </button>
              </div>
              {copyFailedKey === "recall" ? (
                <p role="alert" className="fus-copy-error">
                  {t("firstUse.sample.copyFailed")}
                </p>
              ) : null}
            </div>
            <ul className="fus-secondary-commands">
              <li>
                <div>
                  <code>{commands.handoff}</code>
                  <span>
                    {t("firstUse.sample.handoffLabel")} — {t("firstUse.sample.handoffHint")}
                  </span>
                </div>
                <button
                  type="button"
                  className="fus-icon-button"
                  onClick={() => copyCommand("handoff", commands.handoff)}
                  aria-label={t("firstUse.sample.copyCommand")}
                >
                  {copiedKey === "handoff" ? (
                    <Check aria-hidden="true" weight="bold" data-testid="copy-ok" />
                  ) : (
                    <Copy aria-hidden="true" weight="regular" />
                  )}
                </button>
                {copyFailedKey === "handoff" ? (
                  <p role="alert" className="fus-copy-error">
                    {t("firstUse.sample.copyFailed")}
                  </p>
                ) : null}
              </li>
              <li>
                <div>
                  <code>{commands.brief}</code>
                  <span>
                    {t("firstUse.sample.briefLabel")} — {t("firstUse.sample.briefHint")}
                  </span>
                </div>
                <button
                  type="button"
                  className="fus-icon-button"
                  onClick={() => copyCommand("brief", commands.brief)}
                  aria-label={t("firstUse.sample.copyCommand")}
                >
                  {copiedKey === "brief" ? (
                    <Check aria-hidden="true" weight="bold" data-testid="copy-ok" />
                  ) : (
                    <Copy aria-hidden="true" weight="regular" />
                  )}
                </button>
                {copyFailedKey === "brief" ? (
                  <p role="alert" className="fus-copy-error">
                    {t("firstUse.sample.copyFailed")}
                  </p>
                ) : null}
              </li>
            </ul>
            <p className="fus-note">{t("firstUse.sample.notInLibrary")}</p>
            <div className="fus-cta-row">
              <button type="button" className="fug-button-secondary" onClick={onBringData}>
                {t("firstUse.sample.bringCta")}
              </button>
            </div>
          </section>
        ) : (
          <div className="fus-workbench">
            <div className="fus-rack" aria-label={t("firstUse.sample.sourcesTitle")}>
              <p className="fus-rack-heading">
                {t("firstUse.sample.sourcesTitle")} · {sources.length}
              </p>
              {sources.map((source) => (
                <button
                  key={source.id}
                  type="button"
                  data-testid={`sample-source-${source.id}`}
                  data-source-type={source.type}
                  data-linked={phase >= 2}
                  className="fus-rack-card"
                  aria-haspopup="dialog"
                  onClick={() =>
                    openSource(
                      source.id,
                      citations.find((entry) => entry.sourceId === source.id)?.id ?? null,
                    )
                  }
                >
                  <span className="fus-source-icon" aria-hidden="true">
                    <SourceGlyph type={source.type} />
                  </span>
                  <span className="fus-rack-copy">
                    <span className="fus-source-kind">{source.kind}</span>
                    <span className="fus-rack-title">{source.title}</span>
                    {phase === 0 ? (
                      <span className="fus-rack-excerpt">{source.excerpt}</span>
                    ) : (
                      <span className="fus-rack-point">
                        <HighlightedLine
                          line={keyPointFor(source)}
                          quote={phase >= 1 ? keyPointFor(source) : ""}
                        />
                      </span>
                    )}
                    {phase >= 2 ? (
                      <span className="fus-marker">
                        <LinkSimple aria-hidden="true" weight="regular" />
                        {source.kind}
                      </span>
                    ) : null}
                  </span>
                </button>
              ))}
            </div>

            <div className="fus-doc" data-phase={phase}>
              {phase === 0 ? (
                <p className="fus-forming" key="staging">
                  {t("firstUse.sample.formingNote")}
                </p>
              ) : null}
              {phase === 1 ? (
                <div key="points">
                  <p className="fus-doc-heading">{t("firstUse.sample.pointsTitle")}</p>
                  <ul className="fus-extract-list">
                    {sources.map((source) => (
                      <li key={source.id} data-source-type={source.type}>
                        <span className="fus-source-icon" aria-hidden="true">
                          <SourceGlyph type={source.type} />
                        </span>
                        <span>
                          <HighlightedLine
                            line={keyPointFor(source)}
                            quote={keyPointFor(source)}
                          />
                        </span>
                      </li>
                    ))}
                  </ul>
                </div>
              ) : null}
              {phase === 2 ? (
                <ul className="fus-connect-list" key="connect">
                  {page.paragraphs.map((paragraph) => (
                    <li key={paragraph.text}>
                      <p>{paragraph.text}</p>
                      <div className="fus-connect-badges">
                        {paragraphSources(paragraph).map((source) => (
                          <button
                            key={source.id}
                            type="button"
                            data-source-type={source.type}
                            className="fus-marker"
                            aria-haspopup="dialog"
                            aria-label={source.title}
                            onClick={() =>
                              openSource(
                                source.id,
                                paragraph.citationIds
                                  .map((id) => citationById(id))
                                  .find((entry) => entry?.sourceId === source.id)?.id ?? null,
                              )
                            }
                          >
                            <SourceGlyph type={source.type} />
                            {source.kind}
                          </button>
                        ))}
                      </div>
                    </li>
                  ))}
                </ul>
              ) : null}
              {phase === FINAL_PHASE ? (
                <article className="fus-result-page" key="form" aria-labelledby="fus-result-title">
                  <p className="fug-eyebrow">{t("firstUse.sample.pageEyebrow")}</p>
                  <h2 id="fus-result-title">{page.title}</h2>
                  <p className="fus-result-subtitle">{page.subtitle}</p>
                  <div className="fus-result-rule" aria-hidden="true" />
                  {page.paragraphs.map((paragraph) => (
                    <p key={paragraph.text} className="fus-result-paragraph">
                      {paragraph.text}{" "}
                      {paragraph.citationIds.map((id) => {
                        const entry = citationById(id);
                        const source = entry ? sourceById(entry.sourceId) : undefined;
                        if (!entry || !source) return null;
                        return (
                          <button
                            key={id}
                            type="button"
                            data-testid={`sample-citation-${id}`}
                            data-source-type={source.type}
                            className="fus-citation"
                            aria-haspopup="dialog"
                            aria-label={source.title}
                            onClick={() => openSource(source.id, id)}
                          >
                            <SourceGlyph type={source.type} />
                            {source.kind}
                          </button>
                        );
                      })}
                    </p>
                  ))}
                  <div className="fus-related-card">
                    <p className="fus-related-eyebrow">{t("firstUse.sample.relatedTitle")}</p>
                    <p className="fus-related-title">{page.related.title}</p>
                    <p className="fus-related-body">{page.related.description}</p>
                    <button
                      type="button"
                      className="fus-link"
                      aria-haspopup="dialog"
                      onClick={() => setDialog({ kind: "related" })}
                    >
                      {t("firstUse.sample.openRelated")}
                      <LinkSimple aria-hidden="true" weight="regular" />
                    </button>
                  </div>
                  <div className="fus-cta-row">
                    <button
                      type="button"
                      className="fug-button-primary"
                      onClick={() => setAiOpen(true)}
                    >
                      {t("firstUse.sample.useWithAi")}
                      <ArrowRight aria-hidden="true" weight="regular" />
                    </button>
                  </div>
                </article>
              ) : null}
            </div>
          </div>
        )}
      </div>

      {dialog?.kind === "source" && dialogSource ? (
        <SampleDialog
          labelId="fus-citation-title"
          onClose={closeDialog}
          testId="sample-citation-dialog"
          sourceType={dialogSource.type}
          restoreFocusTo={dialogTrigger.current}
        >
          <div className="fus-dialog-header">
            <div>
              <p className="fug-eyebrow">{t("firstUse.sample.citationTitle")}</p>
              <h2 id="fus-citation-title">{dialogSource.title}</h2>
              <p className="fus-dialog-origin">
                {dialogSource.type === "file" && dialogSource.path != null
                  ? t("firstUse.sample.fileOrigin", {
                      path: dialogSource.path,
                      line: dialogSource.quoteLine ?? 1,
                    })
                  : dialogSource.type === "memory"
                    ? t("firstUse.sample.memoryOrigin")
                    : t("firstUse.sample.pageOrigin")}
              </p>
            </div>
            <button
              type="button"
              className="fus-icon-button"
              onClick={closeDialog}
              aria-label={t("firstUse.sample.close")}
            >
              <X aria-hidden="true" weight="regular" />
            </button>
          </div>
          {dialogCitation ? (
            <blockquote className="fus-quote">{dialogCitation.quote}</blockquote>
          ) : null}
          <div className="fus-original">
            {dialogSource.body.map((line, index) => (
              <p
                key={`${dialogSource.id}-${index}`}
                data-cited={dialogCitation ? line.includes(dialogCitation.quote) : false}
              >
                {dialogSource.type === "file" ? (
                  <span className="fus-line-number" aria-hidden="true">
                    {index + 1}
                  </span>
                ) : null}
                {dialogCitation ? (
                  <HighlightedLine line={line} quote={dialogCitation.quote} />
                ) : (
                  line
                )}
              </p>
            ))}
          </div>
        </SampleDialog>
      ) : null}

      {dialog?.kind === "related" ? (
        <SampleDialog
          labelId="fus-related-title"
          onClose={closeDialog}
          testId="sample-related-dialog"
          restoreFocusTo={dialogTrigger.current}
        >
          <div className="fus-dialog-header">
            <div>
              <p className="fug-eyebrow">{t("firstUse.sample.relatedTitle")}</p>
              <h2 id="fus-related-title">{page.related.title}</h2>
            </div>
            <button
              type="button"
              className="fus-icon-button"
              onClick={closeDialog}
              aria-label={t("firstUse.sample.close")}
            >
              <X aria-hidden="true" weight="regular" />
            </button>
          </div>
          <p className="fus-related-body">{page.related.description}</p>
          <p className="fus-related-body">{page.related.body}</p>
          <button type="button" className="fus-link" onClick={closeDialog}>
            <ArrowLeft aria-hidden="true" weight="regular" />
            {t("firstUse.sample.backToPage")}
          </button>
        </SampleDialog>
      ) : null}
    </>
  );
}
