// SPDX-License-Identifier: AGPL-3.0-only
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery } from "@tanstack/react-query";
import {
  ArrowLeft,
  ArrowRight,
  BookOpenText,
  Brain,
  FileText,
  Sparkle,
  Tray,
} from "@phosphor-icons/react";
import {
  getActiveImportBatches,
  getImportBatchStatus,
  getResolvedRouting,
  type ImportBatchStatus,
} from "../../lib/tauri";
import { isKnowledgePage, listAllActivePages } from "../memory/pages/listAllPages";
import {
  ImportPhaseList,
  importSourceLabel,
  summarizeImportBatches,
} from "../memory/ImportPhases";
import { FirstUseSample } from "./FirstUseSample";
import "./firstUseGuide.css";

export type FirstUseGuideView = "guide" | "live";

export interface FirstUseGuideProps {
  onBack: () => void;
  onImport: () => void;
  onSources: () => void;
  onConnect: () => void;
  onOpenIntelligence: () => void;
  onOpenPage: (id: string) => void;
  /** Entry view. The parent passes "live" when returning from a real import. */
  initialView?: FirstUseGuideView;
  /** The import just completed, if this guide was opened from that import. */
  batchId?: string;
}

type GuideView = "guide" | "choose" | "sample" | "live";

/** Live queries poll only while the live view is mounted: entering the view
 *  mounts them, leaving it unmounts them. The sample view mounts no queries. */
const LIVE_BATCH_POLL_MS = 5_000;
/** Library pages re-read on this cadence so a newly distilled page appears
 *  without a manual refresh. Batch phases keep their own faster poll. */
const LIVE_PAGE_POLL_MS = 15_000;
/** Real library pages shown as "yours". Never attributed to one import: the
 *  current batch status carries no produced-page ids. */
const LIVE_PAGE_COUNT = 3;

export function FirstUseGuide({
  onBack,
  onImport,
  onSources,
  onConnect,
  onOpenIntelligence,
  onOpenPage,
  initialView = "guide",
  batchId,
}: FirstUseGuideProps) {
  const { t } = useTranslation();
  const [view, setView] = useState<GuideView>(initialView);

  return (
    <div
      data-testid="first-use-guide"
      data-view={view}
      className="fug-root"
    >
      {view === "sample" ? (
        <FirstUseSample
          onBackToGuide={() => setView("guide")}
          onBringData={() => setView("choose")}
          onConnect={onConnect}
        />
      ) : null}

      {view === "live" ? (
        <FirstUseLive
          onBackToGuide={() => setView("guide")}
          onImport={onImport}
          onOpenIntelligence={onOpenIntelligence}
          onOpenPage={onOpenPage}
          batchId={batchId}
        />
      ) : null}

      {view === "guide" ? (
        <div className="fug-guide">
          <div className="fug-row">
            <button type="button" className="fug-back" onClick={onBack}>
              <ArrowLeft aria-hidden="true" weight="regular" />
              {t("firstUse.guide.back")}
            </button>
            <span className="fug-eyebrow">{t("firstUse.guide.eyebrow")}</span>
          </div>
          <h1 className="fug-title">{t("firstUse.guide.title")}</h1>
          <p className="fug-lede">{t("firstUse.guide.lede")}</p>
          <ol className="fug-steps">
            <li>
              <span className="fug-step-icon" aria-hidden="true">
                <Tray weight="regular" />
              </span>
              <div>
                <p className="fug-step-title">{t("firstUse.guide.step1Title")}</p>
                <p className="fug-step-body">{t("firstUse.guide.step1Body")}</p>
              </div>
            </li>
            <li>
              <span className="fug-step-icon" aria-hidden="true">
                <BookOpenText weight="regular" />
              </span>
              <div>
                <p className="fug-step-title">{t("firstUse.guide.step2Title")}</p>
                <p className="fug-step-body">{t("firstUse.guide.step2Body")}</p>
              </div>
            </li>
            <li>
              <span className="fug-step-icon" aria-hidden="true">
                <Sparkle weight="regular" />
              </span>
              <div>
                <p className="fug-step-title">{t("firstUse.guide.step3Title")}</p>
                <p className="fug-step-body">{t("firstUse.guide.step3Body")}</p>
              </div>
            </li>
          </ol>
          <div className="fus-cta-row">
            <button
              type="button"
              className="fug-button-primary"
              onClick={() => setView("sample")}
            >
              {t("firstUse.guide.tryExample")}
              <ArrowRight aria-hidden="true" weight="regular" />
            </button>
            <button
              type="button"
              className="fug-button-secondary"
              onClick={() => setView("choose")}
            >
              {t("firstUse.guide.bringData")}
            </button>
          </div>
          <button type="button" className="fus-link" onClick={() => setView("live")}>
            {t("firstUse.guide.seeKnowledge")}
          </button>
        </div>
      ) : null}

      {view === "choose" ? (
        <div className="fug-guide">
          <div className="fug-row">
            <button
              type="button"
              className="fug-back"
              onClick={() => setView("guide")}
            >
              <ArrowLeft aria-hidden="true" weight="regular" />
              {t("firstUse.chooser.back")}
            </button>
          </div>
          <h1 className="fug-title">{t("firstUse.chooser.title")}</h1>
          <p className="fug-lede">{t("firstUse.chooser.body")}</p>
          <ul className="fug-options">
            <li>
              <span className="fug-step-icon" aria-hidden="true">
                <Brain weight="regular" />
              </span>
              <div>
                <p className="fug-step-title">{t("firstUse.chooser.pasteTitle")}</p>
                <p className="fug-step-body">{t("firstUse.chooser.pasteBody")}</p>
              </div>
              <button
                type="button"
                className="fug-button-secondary"
                onClick={onImport}
              >
                {t("firstUse.chooser.pasteAction")}
              </button>
            </li>
            <li>
              <span className="fug-step-icon" aria-hidden="true">
                <FileText weight="regular" />
              </span>
              <div>
                <p className="fug-step-title">{t("firstUse.chooser.filesTitle")}</p>
                <p className="fug-step-body">{t("firstUse.chooser.filesBody")}</p>
              </div>
              <button
                type="button"
                className="fug-button-secondary"
                onClick={onSources}
              >
                {t("firstUse.chooser.filesAction")}
              </button>
            </li>
            <li>
              <span className="fug-step-icon" aria-hidden="true">
                <Sparkle weight="regular" />
              </span>
              <div>
                <p className="fug-step-title">{t("firstUse.chooser.connectTitle")}</p>
                <p className="fug-step-body">{t("firstUse.chooser.connectBody")}</p>
              </div>
              <button
                type="button"
                className="fug-button-secondary"
                onClick={onConnect}
              >
                {t("firstUse.chooser.connectAction")}
              </button>
            </li>
          </ul>
        </div>
      ) : null}
    </div>
  );
}

function FirstUseLive({
  onBackToGuide,
  onImport,
  onOpenIntelligence,
  onOpenPage,
  batchId,
}: {
  onBackToGuide: () => void;
  onImport: () => void;
  onOpenIntelligence: () => void;
  onOpenPage: (id: string) => void;
  batchId?: string;
}) {
  const { t } = useTranslation();
  const pagesQuery = useQuery({
    queryKey: ["first-use-pages"],
    queryFn: listAllActivePages,
    refetchInterval: LIVE_PAGE_POLL_MS,
  });
  const activeBatchesQuery = useQuery({
    queryKey: ["first-use-batches"],
    queryFn: getActiveImportBatches,
    refetchInterval: LIVE_BATCH_POLL_MS,
  });
  const batchStatusQuery = useQuery({
    queryKey: ["first-use-batch", batchId],
    queryFn: () => getImportBatchStatus(batchId!),
    enabled: Boolean(batchId),
    refetchInterval: (query) =>
      query.state.data?.complete ? false : LIVE_BATCH_POLL_MS,
  });

  const routingQuery = useQuery({
    queryKey: ["first-use-routing"],
    queryFn: getResolvedRouting,
    refetchInterval: LIVE_BATCH_POLL_MS,
  });

  const retry = () => {
    void routingQuery.refetch();
    void pagesQuery.refetch();
    void activeBatchesQuery.refetch();
    if (batchId) void batchStatusQuery.refetch();
  };

  const pending = pagesQuery.isPending
    || activeBatchesQuery.isPending
    || (Boolean(batchId) && batchStatusQuery.isPending);
  // A failed query is a failure, never an empty success: it gets its own
  // state with an explicit retry, not the empty-state copy.
  const failed = pagesQuery.isError
    || activeBatchesQuery.isError
    || (Boolean(batchId) && batchStatusQuery.isError);

  const activeBatches = activeBatchesQuery.data?.batches ?? [];
  const targetedBatch = batchStatusQuery.data as ImportBatchStatus | undefined;
  const batches = targetedBatch
    ? [
        targetedBatch,
        ...activeBatches.filter((batch) => batch.batch_id !== targetedBatch.batch_id),
      ]
    : activeBatches;
  const summary = summarizeImportBatches(batches);
  const knowledgePages = (pagesQuery.data ?? [])
    .filter(isKnowledgePage)
    .slice(0, LIVE_PAGE_COUNT);
  const routing = routingQuery.data;
  const everydayBlocked = routing != null && routing.everyday.mode !== "pinned";
  const synthesisBlocked = routing != null && routing.synthesis.mode !== "pinned";
  const needsIntelligence = batches.some((batch) => !batch.complete)
    && (everydayBlocked || synthesisBlocked);
  const canReportRunning = routing != null && !routingQuery.isError && !needsIntelligence;
  // Batch counters describe queued work, not whether a model can run it.
  const displayPhases = summary.phases.map((phase) => {
    const blocked = phase.phase === "distill"
      ? everydayBlocked || synthesisBlocked
      : everydayBlocked && ["detect", "enrich", "link"].includes(phase.phase);
    return blocked && phase.state === "running" ? { ...phase, state: "pending" as const } : phase;
  });
  const runningNames = summary.runningPhases.map((phase) =>
    t(`importBatch.phases.${phase}`),
  );
  const settled = batches.length > 0 && batches.every((batch) => batch.complete);

  return (
    <div data-testid="first-use-live" className="fug-guide">
      <div className="fug-row">
        <button type="button" className="fug-back" onClick={onBackToGuide}>
          <ArrowLeft aria-hidden="true" weight="regular" />
          {t("firstUse.live.back")}
        </button>
        <button
          type="button"
          className="fus-link"
          onClick={retry}
          disabled={pending}
        >
          {t("firstUse.live.refresh")}
        </button>
      </div>
      <h1 className="fug-title">{t("firstUse.live.title")}</h1>
      <p className="fug-lede">{t("firstUse.live.body")}</p>

      {pending && !failed ? <p className="fus-note">{t("firstUse.live.waiting")}</p> : null}

      {failed ? (
        <div className="fus-state" role="alert">
          <p>{t("firstUse.live.loadFailed")}</p>
          <button type="button" className="fug-button-secondary" onClick={retry}>
            {t("firstUse.live.retry")}
          </button>
        </div>
      ) : null}

      {!pending && !failed && batches.length > 0 ? (
        <section aria-labelledby="fus-live-active">
          <h2 id="fus-live-active" className="fus-section-title">
            {t("firstUse.live.activeTitle")}
          </h2>
          <ImportPhaseList phases={displayPhases} />
          <p className="fus-note">
            {t("importBatch.summaryImported", {
              count: summary.memoriesImported,
              source: [...new Set(batches.map((batch) => batch.source === "other" ? t("importBatch.otherSource") : importSourceLabel(batch.source)))].join(" · "),
            })}
            {summary.memoriesSkipped > 0
              ? ` · ${t("importBatch.summarySkipped", { count: summary.memoriesSkipped })}`
              : ""}
          </p>
          {canReportRunning && runningNames.length > 0 ? (
            <p className="fus-note">
              {t("firstUse.live.queuedNote")}
            </p>
          ) : null}
          {needsIntelligence ? (
            <p className="fus-note" role="status">{t("firstUse.live.needsIntelligence")}</p>
          ) : null}
          {summary.complete ? (
            <p className="fus-note">{t("importBatch.backgroundSettled")}</p>
          ) : null}
          {summary.failedPhases.length > 0 ? (
            <p className="fus-warning" role="alert">
              {t("firstUse.live.failedNote")}
            </p>
          ) : null}
          {settled && !summary.hasRelatedPages ? (
            <p className="fus-note">{t("firstUse.live.settledNoPages")}</p>
          ) : null}
          {needsIntelligence || (settled && (!summary.hasRelatedPages || summary.failedPhases.length > 0)) ? (
            <button type="button" className="fug-button-secondary" onClick={onOpenIntelligence}>
              {t("firstUse.live.intelligenceAction")}
            </button>
          ) : null}
        </section>
      ) : null}

      {!pending && !failed && batches.length === 0 && knowledgePages.length > 0 ? (
        <p className="fus-note">{t("firstUse.live.noActiveImports")}</p>
      ) : null}

      {!pending && !failed && knowledgePages.length > 0 ? (
        <section aria-labelledby="fus-live-pages">
          <h2 id="fus-live-pages" className="fus-section-title">
            {t("firstUse.live.yourPages")}
          </h2>
          <p className="fus-note">{t("firstUse.live.yourPagesNote")}</p>
          <ul className="fus-pages">
            {knowledgePages.map((page) => (
              <li key={page.id}>
                <button
                  type="button"
                  data-testid={`first-use-page-${page.id}`}
                  className="fus-page-row"
                  onClick={() => onOpenPage(page.id)}
                >
                  <BookOpenText aria-hidden="true" weight="regular" />
                  <span>{page.title}</span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {!pending && !failed && batches.length === 0 && knowledgePages.length === 0 ? (
        <div className="fus-state">
          <p className="fus-section-title">{t("firstUse.live.emptyTitle")}</p>
          <p className="fus-note">{t("firstUse.live.emptyBody")}</p>
          <div className="fus-cta-row">
            <button
              type="button"
              className="fug-button-primary"
              onClick={onImport}
            >
              {t("firstUse.live.importAction")}
            </button>
            <button
              type="button"
              className="fug-button-secondary"
              onClick={onOpenIntelligence}
            >
              {t("firstUse.live.intelligenceAction")}
            </button>
          </div>
        </div>
      ) : null}
    </div>
  );
}
