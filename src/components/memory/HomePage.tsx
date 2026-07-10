// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import {
  getMemoryStats,
  listPages,
  listRecentChanges,
  listRecentRetrievals,
  type Page,
} from "../../lib/tauri";
import { Greeting } from "./Greeting";
import { RefiningList } from "./RefiningList";
import { ConnectionsList } from "./ConnectionsList";
import { RetrievalsList } from "./RetrievalsList";
import { WhatHappensNextCard, type HomePageState } from "../onboarding/WhatHappensNextCard";
import { GhostPagesRow } from "../onboarding/GhostPagesRow";
import { useMilestones } from "../onboarding/useMilestones";
import { FirstPageModal } from "../onboarding/FirstPageModal";
import { MilestoneHighlight } from "../onboarding/MilestoneHighlight";

interface HomePageProps {
  onNavigateMemory: (sourceId: string) => void;
  onNavigateStream: () => void;
  onNavigateLog: () => void;
  onNavigateGraph: () => void;
  onSelectPage?: (pageId: string) => void;
  onOpenDistillReview?: () => void;
}

const FIRST_CONCEPT_SHOWN_KEY = "onboarding:firstConceptShownCount";
const MAX_MODAL_SHOWS = 3;

function deriveHomePageState(params: {
  intelligenceReady: boolean;
  memoryCount: number;
  conceptCount: number;
}): HomePageState {
  // Pages take precedence over memories: if any page exists the home page is
  // "alive", even if memories were subsequently deleted. The local counter
  // still uses the legacy concept name because the daemon activity contract
  // has not been renamed yet.
  if (params.conceptCount > 0) return "alive";
  if (params.memoryCount > 0) return "gathering";
  if (params.intelligenceReady) return "listening";
  return "seed";
}

export default function HomePage({
  onNavigateMemory: _onNavigateMemory,
  onNavigateStream: _onNavigateStream,
  onNavigateLog: _onNavigateLog,
  onNavigateGraph: _onNavigateGraph,
  onSelectPage,
  onOpenDistillReview,
}: HomePageProps) {
  const { t } = useTranslation();
  const { data: retrievals = [] } = useQuery({
    queryKey: ["recentRetrievals"],
    queryFn: () => listRecentRetrievals(12),
    refetchInterval: 30_000,
  });

  const { data: changes = [] } = useQuery({
    queryKey: ["recentChanges"],
    queryFn: () => listRecentChanges(3),
    refetchInterval: 30_000,
  });

  const { data: recentConcepts = [] } = useQuery({
    queryKey: ["recent-concepts"],
    queryFn: () => listPages("active", undefined, 1000),
    refetchInterval: 10_000,
  });

  const { data: stats } = useQuery({
    queryKey: ["memoryStats"],
    queryFn: getMemoryStats,
    refetchInterval: 10_000,
  });

  const memoryCount = stats?.total ?? 0;
  const conceptCount = recentConcepts.length;
  const hasPages = recentConcepts.length > 0;
  const recentlyRefinedPages = useMemo(
    () =>
      [...recentConcepts]
        .sort((a, b) => Date.parse(b.last_modified) - Date.parse(a.last_modified))
        .slice(0, 6),
    [recentConcepts],
  );
  const pageUpdateItems = useMemo(
    () => pageUpdatesFromPages(recentConcepts),
    [recentConcepts],
  );

  const { milestones, acknowledge } = useMilestones();
  const intelligenceReady = milestones.some(
    (m) => m.id === "intelligence-ready" && m.first_triggered_at != null,
  );
  const firstTriggeredAt = milestones.find((m) => m.id === "intelligence-ready")
    ?.first_triggered_at;
  const daysInListening = firstTriggeredAt
    ? Math.floor((Date.now() / 1000 - firstTriggeredAt) / 86_400)
    : 0;

  const firstConceptMs = milestones.find(
    (m) => m.id === "first-concept" && m.acknowledged_at == null,
  );
  const firstConceptData = firstConceptMs?.payload as
    | { page_id?: string; title?: string }
    | undefined;
  const firstConcept = firstConceptData?.page_id
    ? recentConcepts.find((c) => c.id === firstConceptData.page_id)
    : null;

  const homePageState = deriveHomePageState({
    intelligenceReady,
    memoryCount,
    conceptCount,
  });

  // Track whether we've incremented shown_count for the current first-concept
  // modal instance, so StrictMode double-invokes don't double-count.
  const incrementedOnMountRef = useRef(false);
  useEffect(() => {
    if (!firstConcept || !firstConceptMs) return;
    if (incrementedOnMountRef.current) return;
    incrementedOnMountRef.current = true;
    const current = parseInt(
      localStorage.getItem(FIRST_CONCEPT_SHOWN_KEY) || "0",
      10,
    );
    const next = current + 1;
    localStorage.setItem(FIRST_CONCEPT_SHOWN_KEY, String(next));
    if (next > MAX_MODAL_SHOWS) {
      acknowledge("first-concept");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [firstConcept?.id, firstConceptMs?.id]);

  const firstConceptShownCount = parseInt(
    localStorage.getItem(FIRST_CONCEPT_SHOWN_KEY) || "0",
    10,
  );
  const shouldShowFirstConceptModal =
    !!firstConcept &&
    !!firstConceptMs &&
    firstConceptShownCount <= MAX_MODAL_SHOWS;

  const isEmpty =
    retrievals.length === 0 &&
    changes.length === 0;

  const distillReviewEntry = onOpenDistillReview ? (
    <div style={{ display: "flex", justifyContent: "flex-end", margin: "-4px 0 10px" }}>
      <button
        type="button"
        onClick={onOpenDistillReview}
        style={{
          border: "1px solid var(--mem-border)",
          background: "var(--mem-surface)",
          color: "var(--mem-text)",
          borderRadius: 8,
          padding: "7px 11px",
          fontSize: 13,
          cursor: "pointer",
        }}
      >
        {t("home.reviewPageChanges")}
      </button>
    </div>
  ) : null;

  if (hasPages) {
    return (
      <>
        <WikiHome
          allPages={recentConcepts}
          pages={recentlyRefinedPages}
          pageUpdates={pageUpdateItems}
          onSelectPage={onSelectPage}
          onOpenDistillReview={onOpenDistillReview}
        />

        {shouldShowFirstConceptModal && firstConcept && (
          <FirstPageModal
            page={firstConcept}
            onOpen={(id) => {
              localStorage.removeItem(FIRST_CONCEPT_SHOWN_KEY);
              acknowledge("first-concept");
              onSelectPage?.(id);
            }}
            onDismiss={() => {
              localStorage.removeItem(FIRST_CONCEPT_SHOWN_KEY);
              acknowledge("first-concept");
            }}
          />
        )}
      </>
    );
  }

  return (
    <div className="flex flex-col gap-8 pb-16">
      <Greeting memoryCount={memoryCount} pageCount={conceptCount} />

      {isEmpty ? (
        <>
          <WhatHappensNextCard
            state={homePageState}
            memoryCount={memoryCount}
            daysInListening={daysInListening}
          />
          {distillReviewEntry}
          <MilestoneHighlight
            active={
              !shouldShowFirstConceptModal &&
              milestones.some(
                (m) => m.id === "first-concept" && m.acknowledged_at == null,
              )
            }
            onSeen={() => {}}
            intensity="full"
          >
            <section>
              <GhostPagesRow />
            </section>
          </MilestoneHighlight>
        </>
      ) : (
        <>
          {distillReviewEntry}
          <RefiningList changes={changes} pages={recentConcepts} onSelectPage={onSelectPage} />
          <RetrievalsList
            events={retrievals}
            onSelectPageById={(pageId) => onSelectPage?.(pageId)}
          />
          <ConnectionsList
            onSelectPage={onSelectPage}
            onSelectEntity={undefined}
          />
        </>
      )}

      {shouldShowFirstConceptModal && firstConcept && (
        <FirstPageModal
          page={firstConcept}
          onOpen={(id) => {
            localStorage.removeItem(FIRST_CONCEPT_SHOWN_KEY);
            acknowledge("first-concept");
            onSelectPage?.(id);
          }}
          onDismiss={() => {
            localStorage.removeItem(FIRST_CONCEPT_SHOWN_KEY);
            acknowledge("first-concept");
          }}
        />
      )}
    </div>
  );
}

function pageUpdatesFromPages(pages: Page[]): Page[] {
  return [...pages]
    .filter((page) => page.stale_reason?.trim())
    .sort((a, b) => {
      const aPriority = a.stale_reason === "source_conflict" ? 0 : 1;
      const bPriority = b.stale_reason === "source_conflict" ? 0 : 1;
      if (aPriority !== bPriority) return aPriority - bPriority;
      return Date.parse(b.last_modified) - Date.parse(a.last_modified);
    })
    .slice(0, 3);
}

function pageUpdateReason(t: TFunction, page: Page): string {
  if (page.stale_reason === "source_conflict") return t("home.reasons.sourceConflict");
  if (page.stale_reason === "source_updated") return t("home.reasons.sourceUpdated");
  return t("home.reasons.needsRefresh");
}

function formatPagePath(page: Page): string {
  const domain = page.domain?.trim() || page.space?.trim();
  const title = page.title.replace(/\s+/g, "-");
  return `[[${domain ? `${domain}/` : ""}${title}]]`;
}

function formatSourceCount(t: TFunction, count: number): string {
  return t("home.counts.source", { count });
}

// The rail's list slices to 3, so the metric counts stale pages itself.
function needsReviewCount(pages: Page[]): number {
  return pages.filter((page) => page.stale_reason?.trim()).length;
}

function updatedTodayCount(pages: Page[]): number {
  return pages.filter((page) => {
    const ms = Date.parse(page.last_modified);
    return !Number.isNaN(ms) && Math.floor((Date.now() - ms) / 86_400_000) <= 0;
  }).length;
}

function latestPageUpdate(t: TFunction, pages: Page[]): string {
  const latest = pages.reduce<string | null>((current, page) => {
    if (!current) return page.last_modified;
    return Date.parse(page.last_modified) > Date.parse(current) ? page.last_modified : current;
  }, null);
  return latest ? relativePageDate(t, latest) : t("home.relative.noUpdates");
}

function relativePageDate(t: TFunction, value: string): string {
  const ms = Date.parse(value);
  if (Number.isNaN(ms)) return t("home.relative.updatedRecently");
  const delta = Date.now() - ms;
  const days = Math.floor(delta / 86_400_000);
  if (days <= 0) return t("home.relative.today");
  if (days === 1) return t("home.relative.yesterday");
  if (days < 7) return t("home.relative.daysAgo", { count: days });
  const weeks = Math.floor(days / 7);
  return weeks === 1 ? t("home.relative.weekAgo") : t("home.relative.weeksAgo", { count: weeks });
}

function useElementMinWidth<T extends HTMLElement>(minWidth: number) {
  const ref = useRef<T | null>(null);
  const [matches, setMatches] = useState(false);

  function getMatches() {
    if (typeof window === "undefined") return false;
    const width = ref.current?.getBoundingClientRect().width ?? window.innerWidth;
    return width >= minWidth;
  }

  useEffect(() => {
    if (typeof window === "undefined") return;
    const element = ref.current;
    const update = () => setMatches(getMatches());
    update();

    if (element && "ResizeObserver" in window) {
      const observer = new ResizeObserver(update);
      observer.observe(element);
      return () => observer.disconnect();
    }

    window.addEventListener("resize", update);
    return () => window.removeEventListener("resize", update);
  }, [minWidth]);

  return [ref, matches] as const;
}

function WikiHome({
  allPages,
  pages,
  pageUpdates,
  onSelectPage,
  onOpenDistillReview,
}: {
  allPages: Page[];
  pages: Page[];
  pageUpdates: Page[];
  onSelectPage?: (pageId: string) => void;
  onOpenDistillReview?: () => void;
}) {
  const [containerRef, isWideLayout] = useElementMinWidth<HTMLDivElement>(820);
  return (
    <div
      data-testid="wiki-home"
      ref={containerRef}
      className="wiki-home"
      style={{
        display: "grid",
        gap: isWideLayout ? 24 : 22,
        gridTemplateColumns: "minmax(0, 1fr)",
        maxWidth: 1280,
        margin: "0 auto",
        width: "100%",
        paddingBottom: 64,
        alignItems: "start",
      }}
    >
      <section
        data-testid="wiki-daily-desk"
        className="wiki-daily-desk"
      >
        <TodayHeader />

        <HomeContextRail pages={allPages} />
      </section>

      <div
        data-testid="wiki-content-grid"
        className="wiki-content-grid"
        style={{
          display: "grid",
          gap: isWideLayout ? 28 : 24,
          gridTemplateColumns: isWideLayout ? "minmax(0, 1fr) minmax(320px, 360px)" : "minmax(0, 1fr)",
          gridColumn: "1 / -1",
          alignItems: "start",
          minWidth: 0,
        }}
      >
        <PageList
          pages={pages}
          onSelectPage={onSelectPage}
          isWideLayout={isWideLayout}
        />
        <PageUpdatesRail
          embedded
          pages={pageUpdates}
          onSelectPage={onSelectPage}
          onOpenDistillReview={onOpenDistillReview}
        />
      </div>
    </div>
  );
}

function SectionHeading({
  title,
  action,
  size = "default",
}: {
  title: string;
  action?: React.ReactNode;
  size?: "default" | "compact";
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "baseline",
        justifyContent: "space-between",
        gap: 12,
        marginBottom: 12,
      }}
    >
      <h2
        style={{
          fontFamily: "var(--mem-font-heading)",
          fontSize: size === "compact" ? 14 : 18,
          fontWeight: 500,
          color: "var(--mem-text)",
          letterSpacing: 0,
          lineHeight: 1.2,
          margin: 0,
        }}
      >
        {title}
      </h2>
      {action}
    </div>
  );
}

function TodayHeader() {
  const { t } = useTranslation();
  return (
    <section data-testid="wiki-today-heading" className="wiki-today-heading">
      <SectionHeading title={t("home.todayInWenlan")} />
    </section>
  );
}

function PageList({
  pages,
  onSelectPage,
  isWideLayout,
}: {
  pages: Page[];
  onSelectPage?: (pageId: string) => void;
  isWideLayout: boolean;
}) {
  const { t } = useTranslation();
  if (!pages.length) return null;
  return (
    <div>
      <div
        data-testid="wiki-page-list"
        style={{
          display: "grid",
          gap: 0,
          borderTopStyle: "none",
          borderTopWidth: 0,
          borderTopColor: "transparent",
        }}
      >
        {pages.map((page) => (
          <button
            key={page.id}
            type="button"
            aria-label={t("home.openPage", { title: page.title })}
            className="transition-colors duration-150 hover:bg-[var(--mem-hover)]"
            style={{
              display: "grid",
              width: "100%",
              gap: isWideLayout ? 20 : 12,
              gridTemplateColumns: isWideLayout
                ? "minmax(240px, 1fr) minmax(128px, auto)"
                : "minmax(0, 1fr)",
              padding: isWideLayout ? "14px 4px" : "15px 4px",
              textAlign: "left",
              border: "none",
              borderBottom: "1px solid color-mix(in srgb, var(--mem-border) 70%, transparent)",
              background: "transparent",
              color: "inherit",
              cursor: onSelectPage ? "pointer" : "default",
            }}
            onClick={() => onSelectPage?.(page.id)}
          >
            <div style={{ display: "flex", minWidth: 0, gap: 12 }}>
              <PageIcon />
              <div className="min-w-0">
                <p
                  style={{
                    display: "-webkit-box",
                    WebkitLineClamp: 2,
                    WebkitBoxOrient: "vertical",
                    overflow: "hidden",
                    fontFamily: "var(--mem-font-heading)",
                    fontSize: 16,
                    fontWeight: 500,
                    color: "var(--mem-text)",
                    lineHeight: 1.18,
                    margin: 0,
                  }}
                >
                  {page.title}
                </p>
                <p
                  className="truncate"
                  style={{
                    fontFamily: "var(--mem-font-mono)",
                    fontSize: 11,
                    color: "var(--mem-text-tertiary)",
                    margin: "6px 0 0",
                  }}
                >
                  {formatPagePath(page)}
                </p>
                {page.summary && (
                  <p
                    style={{
                      display: isWideLayout ? "none" : "-webkit-box",
                      WebkitLineClamp: 2,
                      WebkitBoxOrient: "vertical",
                      overflow: "hidden",
                      fontFamily: "var(--mem-font-body)",
                      fontSize: 12,
                      color: "var(--mem-text-secondary)",
                      lineHeight: 1.45,
                      margin: "8px 0 0",
                    }}
                  >
                    {page.summary}
                  </p>
                )}
              </div>
            </div>

            <div
              style={{
                display: "flex",
                flexDirection: isWideLayout ? "column" : "row",
                flexWrap: "wrap",
                alignItems: isWideLayout ? "flex-end" : "center",
                justifyContent: isWideLayout ? "center" : "flex-start",
                gap: isWideLayout ? 4 : 12,
                fontFamily: "var(--mem-font-body)",
                fontSize: 12,
                color: "var(--mem-text-tertiary)",
                textAlign: isWideLayout ? "right" : "left",
              }}
            >
              <span>{formatSourceCount(t, page.source_memory_ids.length)}</span>
              <span>{relativePageDate(t, page.last_modified)}</span>
            </div>
          </button>
        ))}
      </div>
    </div>
  );
}

function HomeContextRail({ pages }: { pages: Page[] }) {
  const { t } = useTranslation();
  return (
    <div
      data-testid="wiki-context-rail"
      className="wiki-context-rail"
    >
      <section
        data-testid="wiki-index-summary"
        className="wiki-index-bar"
        aria-label={t("home.index")}
      >
        <div
          data-testid="wiki-index-strip"
          className="wiki-index-strip"
        >
          <ContextMetric testId="pages" label={t("home.pages")} value={String(pages.length)} />
          <ContextMetric testId="updated-today" label={t("home.relative.today")} value={String(updatedTodayCount(pages))} />
          <ContextMetric testId="needs-review" label={t("home.pageUpdates")} value={String(needsReviewCount(pages))} />
          <ContextMetric testId="latest" label={t("home.latest")} value={latestPageUpdate(t, pages)} />
        </div>
      </section>
    </div>
  );
}

function ContextMetric({ testId, label, value }: { testId: string; label: string; value: string }) {
  return (
    <div data-testid={`wiki-context-${testId}`}>
      <p
        style={{
          fontFamily: "var(--mem-font-mono)",
          fontSize: 11,
          fontWeight: 600,
          color: "var(--mem-text-tertiary)",
          letterSpacing: "0.04em",
          lineHeight: 1.25,
          margin: "0 0 5px",
          textTransform: "uppercase",
        }}
      >
        {label}
      </p>
      <p
        style={{
          fontFamily: "var(--mem-font-body)",
          fontSize: 14,
          fontWeight: 600,
          color: "var(--mem-text)",
          lineHeight: 1.3,
          margin: 0,
        }}
      >
        {value}
      </p>
    </div>
  );
}

function PageUpdatesRail({
  embedded = false,
  pages,
  onSelectPage,
  onOpenDistillReview,
}: {
  embedded?: boolean;
  pages: Page[];
  onSelectPage?: (pageId: string) => void;
  onOpenDistillReview?: () => void;
}) {
  const { t } = useTranslation();
  return (
    <section
      data-testid="wiki-page-updates"
      style={{
        border: embedded ? "none" : "1px solid var(--mem-border)",
        borderRadius: embedded ? 0 : 8,
        padding: embedded ? 0 : 12,
        backgroundColor: embedded ? "transparent" : "color-mix(in srgb, var(--mem-surface) 76%, transparent)",
        maxHeight: embedded ? "none" : "min(420px, calc(100vh - 180px))",
        overflowY: embedded ? "visible" : "auto",
      }}
    >
      <SectionHeading
        title={t("home.pageUpdates")}
        size="compact"
      />
      <div data-testid="worth-a-glance" style={{ display: "grid", gap: 7 }}>
        {pages.length === 0 ? (
          <p
            style={{
              fontFamily: "var(--mem-font-body)",
              fontSize: 12,
              color: "var(--mem-text-tertiary)",
              lineHeight: 1.5,
              margin: 0,
            }}
          >
            {t("home.pagesCurrent")}
          </p>
        ) : (
          pages.map((page) => (
            <PageUpdateRailItem
              key={page.id}
              page={page}
              onSelectPage={onSelectPage}
            />
          ))
        )}
      </div>

      {onOpenDistillReview && (
        <button
          type="button"
          onClick={onOpenDistillReview}
          style={{
            width: "100%",
            marginTop: 12,
            borderRadius: 6,
            padding: "6px 0",
            textAlign: "left",
            border: "0",
            backgroundColor: "transparent",
            color: "var(--mem-accent-sage)",
            cursor: "pointer",
            fontFamily: "var(--mem-font-body)",
            fontSize: 11,
          }}
        >
          {t("home.reviewPageChanges")}
        </button>
      )}
    </section>
  );
}

function PageUpdateRailItem({
  page,
  onSelectPage,
}: {
  page: Page;
  onSelectPage?: (pageId: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      aria-label={t("home.openPageUpdate", { title: page.title })}
      onClick={() => onSelectPage?.(page.id)}
      style={{
        display: "grid",
        gap: 6,
        width: "100%",
        textAlign: "left",
        border: "0",
        borderBottom: "1px solid color-mix(in srgb, var(--mem-border) 68%, transparent)",
        borderRadius: 0,
        padding: "8px 0 9px",
        backgroundColor: "transparent",
        color: "inherit",
        cursor: onSelectPage ? "pointer" : "default",
      }}
    >
      <p
        style={{
          display: "-webkit-box",
          WebkitLineClamp: 1,
          WebkitBoxOrient: "vertical",
          overflow: "hidden",
          fontFamily: "var(--mem-font-heading)",
          fontSize: 13,
          fontWeight: 500,
          color: "var(--mem-text)",
          lineHeight: 1.2,
          margin: 0,
        }}
      >
        {page.title}
      </p>
      <p
        style={{
          fontFamily: "var(--mem-font-body)",
          fontSize: 11,
          color: "var(--mem-text-tertiary)",
          lineHeight: 1.35,
          margin: 0,
        }}
      >
        {formatSourceCount(t, page.source_memory_ids.length)} · {pageUpdateReason(t, page)}
      </p>
    </button>
  );
}

function PageIcon() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" aria-hidden="true" className="mt-1.5 shrink-0" style={{ color: "var(--mem-page-icon)" }}>
      <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
