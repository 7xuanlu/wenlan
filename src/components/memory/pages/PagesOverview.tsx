import { useEffect, useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { formatLocaleDate } from "../../../lib/dateFormat";
import { listRefinements, type DistillReviewResponse, type Page } from "../../../lib/tauri";
import { useTruthStatus } from "../../../hooks/useTruthStatus";
import { PageTruthBadges } from "../PageTruthBadges";
import ReviewDialog from "../ReviewDialog";
import { reviewSuppressKey, useSuppressedReviewItems } from "../reviewSuppression";
import { REVIEW_QUEUE_LIMIT, reviewItemId, type ReviewItem } from "../useReviewQueue";
import {
  EXPLICIT_BROWSE_QUERY_POLICY,
  listAllActivePagesExplicitBrowse,
  listAllDraftPagesExplicitBrowse,
} from "./listAllPages";
import {
  DISTILL_REVIEW_SESSION_QUERY_KEY,
  DISTILL_REVIEW_SESSION_QUERY_POLICY,
  pageCandidateItems,
  pageCleanupSuggestionIds,
} from "./pageReviewSignals";
import { classifyPage, pageSpaceContext } from "./pagePresentation";
import "./pageActions.css";

interface PagesOverviewProps {
  readonly onCreatePage: (space: string | null) => void;
  readonly onSelectDraft: (draftId: string, space: string | null) => void;
  readonly onSelectPage: (pageId: string) => void;
  readonly onSelectSpace: (spaceName: string) => void;
}

type PageSort = "recent" | "title";
type StatusFilter = "all" | "unconfirmed";

// The review badge belongs to distilled prose awaiting a human look. Entity
// rows are excluded from the Wiki because the Entities view is their home.
function isUnconfirmedPage(page: Page): boolean {
  return page.status !== "draft"
    && page.review_status === "unconfirmed";
}

const PAGE_SIZE = 7;

function modifiedAt(page: Page): number {
  const value = Date.parse(page.last_modified || page.last_compiled || page.created_at);
  return Number.isFinite(value) ? value : 0;
}

function comparePages(left: Page, right: Page, sort: PageSort): number {
  if (sort === "title") return left.title.localeCompare(right.title);
  return modifiedAt(right) - modifiedAt(left) || left.title.localeCompare(right.title);
}

function SpaceChip({
  ariaLabel,
  label,
  onSelectSpace,
}: {
  readonly ariaLabel: string;
  readonly label: string;
  readonly onSelectSpace: (spaceName: string) => void;
}) {
  return (
    <button
      aria-label={ariaLabel}
      className="wiki-page-space wiki-page-context-link"
      onClick={(event) => {
        event.stopPropagation();
        onSelectSpace(label);
      }}
      type="button"
    >
      {label}
    </button>
  );
}

export function PagesOverview({
  onCreatePage,
  onSelectDraft,
  onSelectPage,
  onSelectSpace,
}: PagesOverviewProps) {
  const { i18n, t } = useTranslation();
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [spaceFilter, setSpaceFilter] = useState("all");
  const [sort, setSort] = useState<PageSort>("recent");
  const [pageIndex, setPageIndex] = useState(0);
  const [openCandidateId, setOpenCandidateId] = useState<string | null>(null);
  const { hiddenKeys, hide: hideReviewItem } = useSuppressedReviewItems();
  const { cutoverLive } = useTruthStatus();
  const activePagesQuery = useQuery({
    queryKey: ["pages", "active"],
    queryFn: listAllActivePagesExplicitBrowse,
    ...EXPLICIT_BROWSE_QUERY_POLICY,
  });
  const draftPagesQuery = useQuery({
    queryKey: ["pages", "draft"],
    queryFn: listAllDraftPagesExplicitBrowse,
    ...EXPLICIT_BROWSE_QUERY_POLICY,
  });
  const pages = useMemo(
    () => Array.from(new Map([
      ...(draftPagesQuery.data ?? []).map((page) => [page.id, page] as const),
      ...(activePagesQuery.data ?? []).map((page) => [page.id, page] as const),
    ]).values()).filter((page) => classifyPage(page) !== "entity"),
    [activePagesQuery.data, draftPagesQuery.data],
  );
  const isPending = activePagesQuery.isPending || draftPagesQuery.isPending;
  const isError = activePagesQuery.isError || draftPagesQuery.isError;
  const { data: cachedDiscovery } = useQuery<DistillReviewResponse>({
    queryKey: DISTILL_REVIEW_SESSION_QUERY_KEY,
    queryFn: async () => {
      throw new Error("Distill review discovery is populated only by an explicit Review run.");
    },
    enabled: false,
    ...DISTILL_REVIEW_SESSION_QUERY_POLICY,
  });
  const { data: refinements } = useQuery({
    queryKey: ["refinement-proposals"],
    queryFn: () => listRefinements(REVIEW_QUEUE_LIMIT),
    staleTime: 30_000,
  });

  const candidateItems = useMemo(
    () => pageCandidateItems(cachedDiscovery, t("review.untitledCluster")),
    [cachedDiscovery, t],
  );
  const visibleCandidateItems = candidateItems.filter((item) => {
    const key = reviewSuppressKey(item);
    return key == null || !hiddenKeys.has(key);
  });
  const cleanupSuggestionIds = useMemo(
    () => pageCleanupSuggestionIds(refinements),
    [refinements],
  );

  const spaces = useMemo(
    () => Array.from(new Set(pages.map(pageSpaceContext).filter((space): space is string => space !== undefined))).sort((left, right) => left.localeCompare(right)),
    [pages],
  );
  const filteredPages = useMemo(
    () => pages
      .filter((page) => statusFilter === "all" || isUnconfirmedPage(page))
      .filter((page) => spaceFilter === "all" || pageSpaceContext(page) === spaceFilter)
      .sort((left, right) => comparePages(left, right, sort)),
    [pages, sort, spaceFilter, statusFilter],
  );
  const pageCount = Math.max(1, Math.ceil(filteredPages.length / PAGE_SIZE));
  const safePageIndex = Math.min(pageIndex, pageCount - 1);
  const visiblePages = filteredPages.slice(safePageIndex * PAGE_SIZE, (safePageIndex + 1) * PAGE_SIZE);
  const rangeStart = filteredPages.length === 0 ? 0 : safePageIndex * PAGE_SIZE + 1;
  const rangeEnd = Math.min((safePageIndex + 1) * PAGE_SIZE, filteredPages.length);

  useEffect(() => {
    setPageIndex(0);
  }, [sort, spaceFilter, statusFilter]);

  const resolveCandidate = async ({
    item,
    approve,
  }: {
    item: ReviewItem;
    approve: boolean;
  }) => {
    if (!approve && item.kind === "page_candidate") hideReviewItem(item);
  };

  return (
    <section aria-labelledby="pages-overview-title" className="wiki-overview mx-auto w-full max-w-[1130px] pb-16">
      <header className="wiki-overview-header">
        <div className="wiki-overview-heading">
          <div className="wiki-overview-title-row">
            <h1 id="pages-overview-title">{t("pages.overview.title")}</h1>
            {!isPending && !isError && pages.length > 0 && (
              <span className="wiki-overview-count">
                {t("pages.overview.pageCount", { count: pages.length })}
              </span>
            )}
          </div>
          <p>{t("pages.overview.description")}</p>
        </div>
        <button
          className="page-create-action wiki-new-page-action"
          onClick={() => onCreatePage(null)}
          type="button"
        >
          {t("pages.overview.newPage")}
        </button>
      </header>

      {visibleCandidateItems.length > 0 && (
        <section aria-labelledby="wiki-page-candidates-title" className="wiki-candidate-lane">
          <header>
            <h2 id="wiki-page-candidates-title">{t("review.sectionPageCandidates")}</h2>
            <span>{visibleCandidateItems.length}</span>
          </header>
          <ul>
            {visibleCandidateItems.map((item) => {
              if (item.kind !== "page_candidate") return null;
              const linkedPageId = item.cluster.existing_page_id;
              const actionLabel = linkedPageId
                ? `${t("review.openPage")}: ${item.title}`
                : t("pages.overview.previewCandidate", {
                    title: item.title,
                  });
              return (
                <li key={reviewItemId(item)}>
                  <button
                    aria-label={actionLabel}
                    className="wiki-candidate-link"
                    onClick={() => {
                      if (linkedPageId) onSelectPage(linkedPageId);
                      else setOpenCandidateId(reviewItemId(item));
                    }}
                    type="button"
                  >
                    <span>{item.title}</span>
                    <small>
                      {t("review.sources", { count: item.cluster.source_ids.length })}
                    </small>
                  </button>
                  <button
                    aria-label={`${t("review.hide")}: ${item.title}`}
                    className="wiki-candidate-hide"
                    onClick={() => hideReviewItem(item)}
                    type="button"
                  >
                    {t("review.hide")}
                  </button>
                </li>
              );
            })}
          </ul>
        </section>
      )}

      <div className="wiki-filters" aria-label={t("pages.overview.filtersLabel")}>
        <label>
          <span>{t("pages.overview.reviewStatusLabel")}</span>
          <select
            aria-label={t("pages.overview.reviewStatusLabel")}
            onChange={(event) => setStatusFilter(event.target.value as StatusFilter)}
            value={statusFilter}
          >
            <option value="all">{t("pages.overview.reviewStatusAll")}</option>
            <option value="unconfirmed">{t("pages.overview.reviewStatusUnconfirmed")}</option>
          </select>
        </label>
        <label>
          <span>{t("pages.overview.spaceLabel")}</span>
          <select aria-label={t("pages.overview.spaceLabel")} onChange={(event) => setSpaceFilter(event.target.value)} value={spaceFilter}>
            <option value="all">{t("pages.overview.spaceAll")}</option>
            {spaces.map((space) => <option key={space} value={space}>{space}</option>)}
          </select>
        </label>
        <label>
          <span>{t("pages.overview.sortLabel")}</span>
          <select aria-label={t("pages.overview.sortLabel")} onChange={(event) => setSort(event.target.value as PageSort)} value={sort}>
            <option value="recent">{t("pages.overview.sortRecent")}</option>
            <option value="title">{t("pages.overview.sortTitle")}</option>
          </select>
        </label>
      </div>

      {isPending ? (
        <p className="wiki-state">{t("pages.overview.loading")}</p>
      ) : isError ? (
        <p className="wiki-state" role="alert" style={{ color: "var(--mem-danger)" }}>{t("pages.overview.error")}</p>
      ) : pages.length === 0 ? (
        <div className="wiki-empty-state">
          <p>{t("pages.overview.empty")}</p>
          <span>{t("pages.overview.emptyDescription")}</span>
        </div>
      ) : filteredPages.length === 0 ? (
        <p className="wiki-state">{t("pages.overview.noMatches")}</p>
      ) : (
        <div className="wiki-table-wrap" data-testid="pages-library">
          <table className="wiki-table">
            <thead>
              <tr>
                <th scope="col">{t("pages.overview.columns.page")}</th>
                <th scope="col">{t("pages.overview.columns.space")}</th>
                <th scope="col">{t("pages.overview.columns.updated")}</th>
              </tr>
            </thead>
            <tbody>
              {visiblePages.map((page) => {
                const isDraft = page.status === "draft";
                const displayTitle = isDraft && page.title.trim().length === 0
                  ? t("pages.overview.untitledDraft")
                  : page.title;
                const assignedSpace = pageSpaceContext(page);
                const timestamp = modifiedAt(page);
                const updated = timestamp > 0
                  ? formatLocaleDate(new Date(timestamp), i18n.language)
                  : null;
                const spaceDestination = assignedSpace
                  ? t("pages.overview.openSpace", { space: assignedSpace })
                  : "";
                const isUnconfirmed = isUnconfirmedPage(page);
                const hasCleanupSuggestion = !isDraft && cleanupSuggestionIds.has(page.id);
                const stateLabels = [
                  isDraft ? t("pages.overview.draft") : null,
                  isUnconfirmed ? t("pages.overview.unconfirmed") : null,
                  hasCleanupSuggestion ? t("pages.overview.cleanupSuggested") : null,
                ].filter((label): label is string => label !== null);
                const pageActionLabel = [
                  t("pages.overview.openPage", { title: displayTitle }),
                  ...stateLabels,
                ].join(" · ");
                const openPage = () => {
                  if (isDraft) onSelectDraft(page.id, assignedSpace ?? null);
                  else onSelectPage(page.id);
                };
                return (
                  <tr className="wiki-page-row" key={page.id} onClick={openPage}>
                    <td>
                      <div className="wiki-page-cell">
                        <button
                          className="wiki-page-link"
                          aria-label={pageActionLabel}
                          onClick={(event) => {
                            event.stopPropagation();
                            openPage();
                          }}
                          type="button"
                        >
                          <span className="wiki-page-link-label">
                            <span className="wiki-page-link-title">{displayTitle}</span>
                            {isDraft && (
                              <span className="wiki-page-state wiki-page-state--draft">
                                {t("pages.overview.draft")}
                              </span>
                            )}
                            {isUnconfirmed && (
                              <span className="wiki-page-state wiki-page-state--unconfirmed">
                                {t("pages.overview.unconfirmed")}
                              </span>
                            )}
                            {hasCleanupSuggestion && (
                              <span className="wiki-page-state wiki-page-state--attention">
                                {t("pages.overview.cleanupSuggested")}
                              </span>
                            )}
                            <PageTruthBadges cutoverLive={cutoverLive} truth={page.truth} />
                          </span>
                        </button>
                        {page.summary && <p>{page.summary}</p>}
                        <div className="wiki-page-mobile-meta">
                          {assignedSpace && <SpaceChip ariaLabel={spaceDestination} label={assignedSpace} onSelectSpace={onSelectSpace} />}
                          {updated && <time dateTime={updated.dateTime}>{updated.label}</time>}
                        </div>
                      </div>
                    </td>
                    <td data-testid={`page-space-${page.id}`}>{assignedSpace && <SpaceChip ariaLabel={spaceDestination} label={assignedSpace} onSelectSpace={onSelectSpace} />}</td>
                    <td>{updated && <time dateTime={updated.dateTime}>{updated.label}</time>}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          <footer className="wiki-pagination">
            <span>{t("pages.overview.paginationRange", { start: rangeStart, end: rangeEnd, total: filteredPages.length })}</span>
            <div>
              <button disabled={safePageIndex === 0} onClick={() => setPageIndex((current) => Math.max(0, current - 1))} type="button">{t("pages.overview.previous")}</button>
              <button disabled={safePageIndex >= pageCount - 1} onClick={() => setPageIndex((current) => Math.min(pageCount - 1, current + 1))} type="button">
                {t("pages.overview.next")}
                <span aria-hidden="true">→</span>
              </button>
            </div>
          </footer>
        </div>
      )}

      <ReviewDialog
        items={visibleCandidateItems}
        openId={openCandidateId}
        onOpenChange={setOpenCandidateId}
        onResolve={resolveCandidate}
        isResolving={false}
        onOpenPage={onSelectPage}
      />
    </section>
  );
}
