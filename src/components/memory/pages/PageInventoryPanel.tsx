// SPDX-License-Identifier: AGPL-3.0-only
import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { Page } from "../../../lib/tauri";
import { listAllActivePages, listAllDraftPages } from "./listAllPages";
import { pageSpaceContext } from "./pagePresentation";

type PageInventoryPanelProps = {
  readonly currentPageId?: string | null;
  readonly onCreatePage?: () => void;
  readonly onOpenDraft?: (draftId: string, space: string | null) => void;
  readonly onOpenPage?: (page: Page) => void;
};

function modifiedAt(page: Page): number {
  const value = Date.parse(page.last_modified || page.last_compiled || page.created_at);
  return Number.isFinite(value) ? value : 0;
}

export function PageInventoryPanel({
  currentPageId = null,
  onCreatePage,
  onOpenDraft,
  onOpenPage,
}: PageInventoryPanelProps) {
  const { i18n, t } = useTranslation();
  const [filter, setFilter] = useState("");
  // This is an ambient workspace list. Its query keys must stay separate from
  // Wiki's explicit-browse cache: merely mounting the sidebar is not a human
  // truth-manifest browse and must never record one.
  const activePages = useQuery({
    queryKey: ["pages", "inventory", "passive", "active"],
    queryFn: listAllActivePages,
    staleTime: 30_000,
  });
  const draftPages = useQuery({
    queryKey: ["pages", "inventory", "passive", "draft"],
    queryFn: listAllDraftPages,
    staleTime: 30_000,
  });

  const pages = useMemo(() => {
    const byId = new Map<string, Page>();
    for (const page of [...(draftPages.data ?? []), ...(activePages.data ?? [])]) {
      if (page.entity_id || page.creation_kind === "entity") continue;
      byId.set(page.id, page);
    }
    return [...byId.values()].sort(
      (left, right) => modifiedAt(right) - modifiedAt(left) || left.title.localeCompare(right.title),
    );
  }, [activePages.data, draftPages.data]);
  const normalizedFilter = filter.trim().toLocaleLowerCase(i18n.language);
  const visiblePages = normalizedFilter
    ? pages.filter((page) => page.title.toLocaleLowerCase(i18n.language).includes(normalizedFilter))
    : pages;
  const isLoading = activePages.isPending || draftPages.isPending;
  const isError = activePages.isError || draftPages.isError;

  return (
    <section aria-label={t("sidebar.notes")} className="notes-list-panel">
      <div className="notes-list-header">
        <h2>{t("sidebar.notes")}</h2>
        {onCreatePage && (
          <button
            aria-label={t("sidebar.newNote")}
            className="notes-list-create"
            onClick={onCreatePage}
            title={t("sidebar.newNote")}
            type="button"
          >
            <svg aria-hidden="true" fill="none" height="16" stroke="currentColor" strokeLinecap="round" strokeWidth="1.8" viewBox="0 0 24 24" width="16"><path d="M12 4v16M4 12h16" /></svg>
          </button>
        )}
      </div>
      <label className="notes-list-filter">
        <span className="sr-only">{t("sidebar.filterNotes")}</span>
        <svg aria-hidden="true" fill="none" height="14" stroke="currentColor" strokeWidth="1.8" viewBox="0 0 24 24" width="14"><circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 5 5" /></svg>
        <input
          aria-label={t("sidebar.filterNotes")}
          onChange={(event) => setFilter(event.target.value)}
          type="search"
          value={filter}
        />
      </label>
      <div className="notes-list-scroll">
        {isLoading ? (
          <p className="notes-list-state" role="status">{t("pages.overview.loading")}</p>
        ) : isError ? (
          <div className="notes-list-state" role="alert">
            <p>{t("pages.overview.error")}</p>
            <button onClick={() => { void activePages.refetch(); void draftPages.refetch(); }} type="button">{t("pageDetail.retry")}</button>
          </div>
        ) : visiblePages.length === 0 ? (
          <p className="notes-list-state">{filter ? t("pages.overview.noMatches") : t("pages.overview.empty")}</p>
        ) : (
          <ul className="notes-page-list">
            {visiblePages.map((page) => {
              const isDraft = page.status === "draft";
              const title = isDraft && !page.title.trim()
                ? t("pages.overview.untitledDraft")
                : page.title;
              const canOpen = isDraft ? !!onOpenDraft : !!onOpenPage;
              return (
                <li key={page.id}>
                  <button
                    aria-current={currentPageId === page.id ? "page" : undefined}
                    aria-label={t("pages.overview.openPage", { title })}
                    className="notes-page-button"
                    data-active={currentPageId === page.id ? "true" : undefined}
                    disabled={!canOpen}
                    onClick={() => {
                      if (isDraft) onOpenDraft?.(page.id, pageSpaceContext(page) ?? null);
                      else onOpenPage?.(page);
                    }}
                    title={title}
                    type="button"
                  >
                    <span className="notes-page-title">{title}</span>
                    {isDraft && <span className="notes-page-status">{t("pages.overview.draft")}</span>}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </section>
  );
}
