// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useId, useMemo, useState } from "react";
import { CaretRight, FileText, Folder, MagnifyingGlass, NotePencil, Plus, Stack } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { Page } from "../../../lib/tauri";
import { listAllActivePages, listAllDraftPages, listAllActivePagesExplicitBrowse, listAllDraftPagesExplicitBrowse, EXPLICIT_BROWSE_QUERY_POLICY } from "./listAllPages";
import { pageSpaceContext } from "./pagePresentation";
import {
  collectPageInventory,
  filterPageInventory,
  inventoryPageFilename,
  pageInventoryScope,
  pageMatchesInventoryScope,
  type PageCollectionScope,
  type WikiInventoryScope,
} from "./pageInventory";
import "./pageInventory.css";

type PageInventoryPanelProps = {
  readonly currentPageId?: string | null;
  readonly inventoryScope?: WikiInventoryScope;
  readonly browsing?: boolean;
  readonly onBrowse?: (scope: WikiInventoryScope) => void;
  readonly onCreatePage?: () => void;
  readonly onOpenDraft?: (draftId: string, space: string | null) => void;
  readonly onOpenPage?: (page: Page) => void;
};

const collections: readonly PageCollectionScope[] = ["files", "drafts", "unfiled"];
const collectionIcons = { files: Folder, drafts: NotePencil, unfiled: Stack };

export function PageInventoryPanel({
  currentPageId = null,
  inventoryScope = "all",
  browsing = false,
  onBrowse,
  onCreatePage,
  onOpenDraft,
  onOpenPage,
}: PageInventoryPanelProps) {
  const { i18n, t } = useTranslation();
  const inventoryId = useId();
  const [filter, setFilter] = useState("");
  const [expanded, setExpanded] = useState<Partial<Record<PageCollectionScope, boolean>>>({});
  // The overview is a deliberate browse: share its exact keys, data and policy.
  // Every ambient/detail sidebar keeps separate passive reads without the marker.
  const activePages = useQuery({
    queryKey: browsing ? ["pages", "active"] : ["pages", "inventory", "passive", "active"],
    queryFn: browsing ? listAllActivePagesExplicitBrowse : listAllActivePages,
    ...(browsing ? EXPLICIT_BROWSE_QUERY_POLICY : { staleTime: 30_000 }),
  });
  const draftPages = useQuery({
    queryKey: browsing ? ["pages", "draft"] : ["pages", "inventory", "passive", "draft"],
    queryFn: browsing ? listAllDraftPagesExplicitBrowse : listAllDraftPages,
    ...(browsing ? EXPLICIT_BROWSE_QUERY_POLICY : { staleTime: 30_000 }),
  });
  const pages = useMemo(
    () => collectPageInventory(activePages.data ?? [], draftPages.data ?? []),
    [activePages.data, draftPages.data],
  );
  const selectedPage = pages.find((page) => page.id === currentPageId);
  const selectedCollection = selectedPage ? pageInventoryScope(selectedPage) : null;
  useEffect(() => {
    if (!browsing && selectedCollection) {
      setExpanded((previous) => ({ ...previous, [selectedCollection]: true }));
    }
  }, [browsing, currentPageId, selectedCollection]);

  const hasFilter = !!filter.trim();
  const visiblePages = filterPageInventory(pages, filter, i18n.language);
  const isLoading = activePages.isPending || draftPages.isPending;
  const isError = activePages.isError || draftPages.isError;

  const renderPage = (page: Page) => {
    const isDraft = page.status === "draft";
    const title = isDraft && !page.title.trim() ? t("pages.overview.untitledDraft") : page.title;
    const filename = !isDraft ? inventoryPageFilename(page) : null;
    const canOpen = isDraft ? !!onOpenDraft : !!onOpenPage;
    return (
      <li key={page.id}>
        <button
          aria-current={currentPageId === page.id ? "page" : undefined}
          aria-label={t("pages.overview.openPage", { title })}
          className="notes-page-button notes-inventory-page"
          data-active={currentPageId === page.id ? "true" : undefined}
          disabled={!canOpen}
          onClick={() => {
            if (isDraft) onOpenDraft?.(page.id, pageSpaceContext(page) ?? null);
            else onOpenPage?.(page);
          }}
          title={title}
          type="button"
        >
          <FileText aria-hidden="true" size={14} weight="regular" />
          <span className="notes-page-title">{filename ?? title}</span>
        </button>
      </li>
    );
  };

  return (
    <section aria-label={t("sidebar.notes")} className="notes-list-panel notes-inventory-panel">
      <div className="notes-list-header">
        <h2>{t("sidebar.notes")}</h2>
        {onCreatePage && (
          <button aria-label={t("sidebar.newNote")} className="notes-list-create" onClick={onCreatePage} title={t("sidebar.newNote")} type="button">
            <Plus aria-hidden="true" size={16} weight="regular" />
          </button>
        )}
      </div>
      <label className="notes-list-filter">
        <span className="sr-only">{t("sidebar.filterNotes")}</span>
        <MagnifyingGlass aria-hidden="true" size={14} weight="regular" />
        <input aria-label={t("sidebar.filterNotes")} onChange={(event) => setFilter(event.target.value)} type="search" value={filter} />
      </label>
      <div className="notes-list-scroll">
        {isLoading ? (
          <p className="notes-list-state" role="status">{t("pages.overview.loading")}</p>
        ) : isError ? (
          <div className="notes-list-state" role="alert">
            <p>{t("pages.overview.error")}</p>
            <button onClick={() => { void activePages.refetch(); void draftPages.refetch(); }} type="button">{t("pageDetail.retry")}</button>
          </div>
        ) : (
          <>
            <button
              aria-current={browsing && inventoryScope === "all" ? "page" : undefined}
              aria-label={t("pages.inventory.all")}
              className="notes-inventory-scope notes-inventory-all"
              data-active={browsing && inventoryScope === "all" ? "true" : undefined}
              disabled={!onBrowse}
              onClick={() => onBrowse?.("all")}
              title={t("pages.inventory.browse")}
              type="button"
            >
              <Stack aria-hidden="true" size={16} weight="regular" />
              <span>{t("pages.inventory.all")}</span>
              <span className="notes-inventory-count">{pages.length}</span>
            </button>
            <ul className="notes-inventory-collections">
              {collections.map((scope) => {
                const collection = pages.filter((page) => pageMatchesInventoryScope(page, scope));
                if (scope === "files" && collection.length === 0) return null;
                const matches = visiblePages.filter((page) => pageMatchesInventoryScope(page, scope));
                if (hasFilter && matches.length === 0) return null;
                const name = t(`pages.inventory.${scope}`);
                const Icon = collectionIcons[scope];
                const isExpanded = hasFilter || !!expanded[scope];
                const showPages = hasFilter || (!browsing && isExpanded);
                const listId = `${inventoryId}-${scope}`;
                return (
                  <li key={scope}>
                    <div className="notes-inventory-collection-row">
                      {(!browsing || hasFilter) && (
                        <button
                          aria-controls={listId}
                          aria-expanded={showPages}
                          aria-label={t(isExpanded ? "pages.inventory.collapse" : "pages.inventory.expand", { name })}
                          className="notes-inventory-disclosure"
                          disabled={hasFilter || collection.length === 0}
                          onClick={() => setExpanded((previous) => ({ ...previous, [scope]: !previous[scope] }))}
                          type="button"
                        >
                          <CaretRight aria-hidden="true" size={12} weight="regular" />
                        </button>
                      )}
                      <button
                        aria-current={browsing && inventoryScope === scope ? "page" : undefined}
                        aria-label={name}
                        className="notes-inventory-scope"
                        data-active={browsing && inventoryScope === scope ? "true" : undefined}
                        disabled={!onBrowse}
                        onClick={() => onBrowse?.(scope)}
                        type="button"
                      >
                        <Icon aria-hidden="true" size={16} weight="regular" />
                        <span>{name}</span>
                        <span className="notes-inventory-count">{hasFilter ? matches.length : collection.length}</span>
                      </button>
                    </div>
                    <ul className="notes-page-list notes-inventory-pages" hidden={!showPages} id={listId}>
                      {showPages && matches.map(renderPage)}
                    </ul>
                  </li>
                );
              })}
            </ul>
            {visiblePages.length === 0 && (
              <p className="notes-list-state">{hasFilter ? t("pages.overview.noMatches") : t("pages.inventory.empty")}</p>
            )}
          </>
        )}
      </div>
    </section>
  );
}
