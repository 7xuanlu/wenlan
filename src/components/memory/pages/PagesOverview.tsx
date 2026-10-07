import { useEffect, useMemo, useRef, useState } from "react";
import { DotsThree, Folder } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { readAssetLens, writeAssetLens, type AssetLens } from "../../../lib/assetLens";
import { formatLocaleDate, type LocaleDateDisplay } from "../../../lib/dateFormat";
import { listRefinements, type DistillReviewResponse, type Page } from "../../../lib/tauri";
import { useTruthStatus } from "../../../hooks/useTruthStatus";
import { AssetCard } from "../assets/AssetCard";
import "../assets/assetCards.css";
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
import { inventoryFolderPath, folderScope, pageMatchesInventoryScope, collectPageInventory, type WikiInventoryScope } from "./pageInventory";
import "./wikiInventoryOverview.css";
import { useKnowledgeFolders } from "./useKnowledgeFolders";
import { pageSpaceContext } from "./pagePresentation";
import "./pageActions.css";
import { FirstPageMilestone } from "../../onboarding/FirstPageMilestone";

interface PagesOverviewProps {
  readonly inventoryScope?: WikiInventoryScope;
  readonly onBrowseAll?: () => void;
  readonly onBrowseFolder?: (scope: WikiInventoryScope) => void;
  readonly onOpenReview?: () => void;
  readonly onCreatePage: (space: string | null, folderPath?: string) => void;
  readonly onSelectDraft: (draftId: string, space: string | null) => void;
  readonly onSelectPage: (pageId: string) => void;
  readonly onSelectSpace: (spaceName: string) => void;
}

type PageSort = "recent" | "title";

// The review badge belongs to distilled prose awaiting a human look. Entity
// rows are excluded from the Wiki because the Entities view is their home.
function isUnconfirmedPage(page: Page): boolean {
  return page.status !== "draft"
    && page.review_status === "unconfirmed";
}

const PAGE_SIZE = 12;

function PageOptions({ onOpenReview }: { readonly onOpenReview: () => void }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const anchorRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const reviewRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    reviewRef.current?.focus();
    const closeOutside = (event: PointerEvent) => {
      if (!anchorRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", closeOutside);
    return () => document.removeEventListener("pointerdown", closeOutside);
  }, [open]);

  return (
    <div
      className="wiki-options-anchor"
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false);
      }}
      onKeyDown={(event) => {
        if (event.key !== "Escape" || !open) return;
        event.preventDefault();
        event.stopPropagation();
        setOpen(false);
        triggerRef.current?.focus();
      }}
      ref={anchorRef}
    >
      <button
        aria-expanded={open}
        aria-haspopup="menu"
        aria-label={t("pages.overview.options")}
        className="mem-icon-action"
        onClick={() => setOpen((current) => !current)}
        ref={triggerRef}
        title={t("pages.overview.options")}
        type="button"
      >
        <DotsThree aria-hidden="true" size={20} />
      </button>
      {open && (
        <div aria-label={t("pages.overview.options")} className="mem-popover-surface wiki-options-menu" role="menu">
          <button
            className="wiki-review-action"
            onClick={() => { setOpen(false); onOpenReview(); }}
            ref={reviewRef}
            role="menuitem"
            type="button"
          >
            {t("home.reviewPageChanges")}
          </button>
        </div>
      )}
    </div>
  );
}

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

interface WikiPageView {
  readonly isDraft: boolean;
  readonly displayTitle: string;
  readonly assignedSpace: string | undefined;
  readonly updated: LocaleDateDisplay | null;
  readonly spaceDestination: string;
  readonly isUnconfirmed: boolean;
  readonly hasCleanupSuggestion: boolean;
  readonly pageActionLabel: string;
  readonly openPage: () => void;
}

// One source of truth for the per-page display both lenses share: the row
// title button and the card open control show the same title, carry the same
// accessible action label, and navigate identically.
function describeWikiPage(
  page: Page,
  deps: {
    readonly t: TFunction;
    readonly language: string;
    readonly cleanupSuggestionIds: ReadonlySet<string>;
    readonly onSelectDraft: (draftId: string, space: string | null) => void;
    readonly onSelectPage: (pageId: string) => void;
  },
): WikiPageView {
  const { t, language, cleanupSuggestionIds, onSelectDraft, onSelectPage } = deps;
  const isDraft = page.status === "draft";
  const displayTitle = isDraft && page.title.trim().length === 0
    ? t("pages.overview.untitledDraft")
    : page.title;
  const assignedSpace = pageSpaceContext(page);
  const timestamp = modifiedAt(page);
  const updated = timestamp > 0
    ? formatLocaleDate(new Date(timestamp), language)
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
  return {
    isDraft,
    displayTitle,
    assignedSpace,
    updated,
    spaceDestination,
    isUnconfirmed,
    hasCleanupSuggestion,
    pageActionLabel,
    openPage,
  };
}

export function PagesOverview({
  inventoryScope = "all",
  onBrowseAll,
  onBrowseFolder,
  onOpenReview,
  onCreatePage,
  onSelectDraft,
  onSelectPage,
  onSelectSpace,
}: PagesOverviewProps) {
  const { i18n, t } = useTranslation();
  const sort: PageSort = "recent";
  const folderPath = inventoryFolderPath(inventoryScope);
  const folders = useKnowledgeFolders();
  const childFolders = folderPath === null || folders.data?.truncated ? [] : (folders.data?.folders ?? []).filter(folder => folder.parent_path === folderPath);
  const folderTitle = folderPath === null ? t("pages.overview.title") : folderPath === "" ? t("pages.folders.root") : folderPath.split("/").slice(-1)[0];
  const [pageIndex, setPageIndex] = useState(0);
  const [lens, setLens] = useState<AssetLens>(() => readAssetLens("wiki"));
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
    () => collectPageInventory(activePagesQuery.data ?? [], draftPagesQuery.data ?? []),
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

  const filteredPages = useMemo(
    () => pages
      .filter((page) => pageMatchesInventoryScope(page, inventoryScope))
      .sort((left, right) => comparePages(left, right, sort)),
    [pages, sort, inventoryScope],
  );
  const pageCount = Math.max(1, Math.ceil(filteredPages.length / PAGE_SIZE));
  const safePageIndex = Math.min(pageIndex, pageCount - 1);
  const visiblePages = filteredPages.slice(safePageIndex * PAGE_SIZE, (safePageIndex + 1) * PAGE_SIZE);

  useEffect(() => {
    setPageIndex(0);
  }, [sort, inventoryScope]);

  const handleLensChange = (next: AssetLens) => {
    setLens(next);
    writeAssetLens("wiki", next);
  };

  const describePage = (page: Page): WikiPageView => describeWikiPage(page, {
    t,
    language: i18n.language,
    cleanupSuggestionIds,
    onSelectDraft,
    onSelectPage,
  });

  const pagination = (
    <footer className="wiki-pagination">
      <span className="sr-only">{t("pages.overview.paginationRange", { start: safePageIndex * PAGE_SIZE + 1, end: Math.min((safePageIndex + 1) * PAGE_SIZE, filteredPages.length), total: filteredPages.length })}</span>

      <div>
        <button disabled={safePageIndex === 0} onClick={() => setPageIndex((current) => Math.max(0, current - 1))} type="button">{t("pages.overview.previous")}</button>
        <button disabled={safePageIndex >= pageCount - 1} onClick={() => setPageIndex((current) => Math.min(pageCount - 1, current + 1))} type="button">
          {t("pages.overview.next")}
          <span aria-hidden="true">→</span>
        </button>
      </div>
    </footer>
  );

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
      <FirstPageMilestone pages={pages} onSelectPage={onSelectPage} />
      {inventoryScope !== "all" && onBrowseAll && <nav aria-label={t("pages.inventory.browse")} className="wiki-inventory-breadcrumb"><button onClick={onBrowseAll} type="button">{t("pages.overview.title")}</button><span aria-hidden="true">/</span><span>{folderPath || t("pages.folders.root")}</span></nav>}
      <header className="wiki-overview-header">
        <div className="wiki-overview-heading">
          <div className="wiki-overview-title-row">
            <h1 id="pages-overview-title">{folderTitle}</h1>
            {!isPending && !isError && <span className="sr-only">{t("pages.overview.pageCount", { count: filteredPages.length })}</span>}
          </div>
        </div>
        <div className="wiki-overview-actions">
          <button
            className="page-create-action wiki-new-page-action"
            onClick={() => folderPath === null ? onCreatePage(null) : onCreatePage(null, folderPath)}
            type="button"
          >
            {t("pages.overview.newPage")}
          </button>
          {onOpenReview && <PageOptions onOpenReview={onOpenReview} />}
        </div>
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

      <div className="wiki-folder-lenses" role="group" aria-label={t("pages.folders.view")}>
        <button type="button" data-testid="asset-lens-cards" aria-pressed={lens === "cards"} onClick={() => handleLensChange("cards")}>{t("pages.folders.cards")}</button>
        <button type="button" data-testid="asset-lens-rows" aria-pressed={lens === "rows"} onClick={() => handleLensChange("rows")}>{t("pages.folders.list")}</button>
      </div>
      {!folders.isError && childFolders.length > 0 && <div className={lens === "cards" ? "wiki-child-folders wiki-child-folders--cards" : "wiki-child-folders"} aria-label={t("pages.folders.title")}>
        {childFolders.map(folder => <button key={folder.path} type="button" className="wiki-child-folder" onClick={() => onBrowseFolder?.(folderScope(folder.path))}><Folder aria-hidden="true" size={18}/><span>{folder.name}</span></button>)}
      </div>}
      {folderPath !== null && (folders.isError || folders.data?.truncated) && <p className="wiki-state" role="status">{t("pages.folders.unavailable")}</p>}

      {isPending ? (
        <p className="wiki-state">{t("pages.overview.loading")}</p>
      ) : isError ? (
        <p className="wiki-state" role="alert" style={{ color: "var(--mem-danger)" }}>{t("pages.overview.error")}</p>
      ) : pages.length === 0 && childFolders.length === 0 && folderPath === null ? (
        <div className="wiki-empty-state">
          <p>{t("pages.overview.empty")}</p>
          <span>{t("pages.overview.emptyDescription")}</span>
        </div>
      ) : filteredPages.length === 0 ? (
        <p className="wiki-state">{t("pages.folders.empty")}</p>
      ) : lens === "cards" ? (
        <div className="wiki-cards-wrap" data-testid="pages-library">
          <div className="asset-cards" data-testid="wiki-cards">
            {visiblePages.map((page) => {
              const view = describePage(page);
              return (
                <AssetCard
                  key={page.id}
                  context={page.summary}
                  footer={(
                    <>
                      {folderPath === null && view.assignedSpace && <SpaceChip ariaLabel={view.spaceDestination} label={view.assignedSpace} onSelectSpace={onSelectSpace} />}
                      {folderPath === null && view.updated && <time dateTime={view.updated.dateTime}>{view.updated.label}</time>}
                    </>
                  )}
                  status={(view.isDraft || view.isUnconfirmed || view.hasCleanupSuggestion || (cutoverLive && page.truth)) ? (
                    <>
                      {view.isDraft && (
                        <span className="wiki-page-state wiki-page-state--draft">
                          {t("pages.overview.draft")}
                        </span>
                      )}
                      {view.isUnconfirmed && (
                        <span className="wiki-page-state wiki-page-state--unconfirmed">
                          {t("pages.overview.unconfirmed")}
                        </span>
                      )}
                      {view.hasCleanupSuggestion && (
                        <span className="wiki-page-state wiki-page-state--attention">
                          {t("pages.overview.cleanupSuggested")}
                        </span>
                      )}
                      <PageTruthBadges cutoverLive={cutoverLive} truth={page.truth} />
                    </>
                  ) : undefined}
                  onOpen={view.openPage}
                  openLabel={view.pageActionLabel}
                  testId={`wiki-card-${page.id}`}
                  title={view.displayTitle}
                />
              );
            })}
          </div>
          {pageCount > 1 && pagination}
        </div>
      ) : (
        <div className="wiki-table-wrap" data-testid="pages-library">
          <table className="wiki-table">
            <thead className={folderPath !== null ? "sr-only" : undefined}>
              <tr>
                <th scope="col">{t("pages.overview.columns.page")}</th>
                {folderPath === null && <><th scope="col">{t("pages.overview.columns.space")}</th><th scope="col">{t("pages.overview.columns.updated")}</th></>}
              </tr>
            </thead>
            <tbody>
              {visiblePages.map((page) => {
                const view = describePage(page);
                return (
                  <tr className="wiki-page-row" key={page.id} onClick={view.openPage}>
                    <td>
                      <div className="wiki-page-cell">
                        <button
                          className="wiki-page-link"
                          aria-label={view.pageActionLabel}
                          onClick={(event) => {
                            event.stopPropagation();
                            view.openPage();
                          }}
                          type="button"
                        >
                          <span className="wiki-page-link-label">
                            <span className="wiki-page-link-title">{view.displayTitle}</span>
                            {view.isDraft && (
                              <span className="wiki-page-state wiki-page-state--draft">
                                {t("pages.overview.draft")}
                              </span>
                            )}
                            {view.isUnconfirmed && (
                              <span className="wiki-page-state wiki-page-state--unconfirmed">
                                {t("pages.overview.unconfirmed")}
                              </span>
                            )}
                            {view.hasCleanupSuggestion && (
                              <span className="wiki-page-state wiki-page-state--attention">
                                {t("pages.overview.cleanupSuggested")}
                              </span>
                            )}
                            <PageTruthBadges cutoverLive={cutoverLive} truth={page.truth} />
                          </span>
                        </button>
                        <div className="wiki-page-mobile-meta">
                          {folderPath === null && view.assignedSpace && <SpaceChip ariaLabel={view.spaceDestination} label={view.assignedSpace} onSelectSpace={onSelectSpace} />}
                          {folderPath === null && view.updated && <time dateTime={view.updated.dateTime}>{view.updated.label}</time>}
                        </div>
                      </div>
                    </td>
                    {folderPath === null && <><td data-testid={`page-space-${page.id}`}>{folderPath === null && view.assignedSpace && <SpaceChip ariaLabel={view.spaceDestination} label={view.assignedSpace} onSelectSpace={onSelectSpace} />}</td>
                    <td>{folderPath === null && view.updated && <time dateTime={view.updated.dateTime}>{view.updated.label}</time>}</td></>}
                  </tr>
                );
              })}
            </tbody>
          </table>
          {pageCount > 1 && pagination}
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
