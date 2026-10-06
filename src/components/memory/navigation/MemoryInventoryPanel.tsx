// SPDX-License-Identifier: AGPL-3.0-only
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { getMemoryDetail, listMemoriesRich } from "../../../lib/tauri";

type MemoryInventoryPanelProps = {
  readonly currentMemoryId?: string | null;
  readonly onOpenMemory?: (sourceId: string) => void;
};

export function MemoryInventoryPanel({
  currentMemoryId = null,
  onOpenMemory,
}: MemoryInventoryPanelProps) {
  const { i18n, t } = useTranslation();
  const [filter, setFilter] = useState("");
  // Share Main's bounded recent-memory cache. This panel is not a full inventory.
  const recentMemories = useQuery({
    queryKey: ["memories"],
    queryFn: () => listMemoriesRich(undefined, undefined, undefined, 200),
  });
  const currentIsRecent = recentMemories.data?.some((memory) => memory.source_id === currentMemoryId) ?? false;
  const needsCurrentMemory = !!currentMemoryId && recentMemories.isSuccess && !currentIsRecent;
  const currentMemory = useQuery({
    queryKey: ["memoryDetail", currentMemoryId],
    queryFn: () => getMemoryDetail(currentMemoryId!),
    enabled: needsCurrentMemory,
  });
  const memories = needsCurrentMemory && currentMemory.data
    ? [currentMemory.data, ...(recentMemories.data ?? [])]
    : recentMemories.data ?? [];
  const normalizedFilter = filter.trim().toLocaleLowerCase(i18n.language);
  const visibleMemories = normalizedFilter
    ? memories.filter((memory) => `${memory.title}\n${memory.content}`.toLocaleLowerCase(i18n.language).includes(normalizedFilter))
    : memories;

  return (
    <section aria-label={t("knowledgeContext.memoryList")} className="notes-list-panel">
      <div className="notes-list-header">
        <h2>{t("knowledgeContext.recentMemories")}</h2>
      </div>
      <p className="notes-memory-hint">{t("knowledgeContext.memoryHint")}</p>
      <label className="notes-list-filter">
        <span className="sr-only">{t("knowledgeContext.filterMemories")}</span>
        <svg aria-hidden="true" fill="none" height="14" stroke="currentColor" strokeWidth="1.8" viewBox="0 0 24 24" width="14"><circle cx="10.5" cy="10.5" r="6.5" /><path d="m16 16 5 5" /></svg>
        <input
          aria-label={t("knowledgeContext.filterMemories")}
          onChange={(event) => setFilter(event.target.value)}
          type="search"
          value={filter}
        />
      </label>
      <div className="notes-list-scroll">
        {recentMemories.isPending ? (
          <p className="notes-list-state" role="status">{t("knowledgeContext.loadingMemories")}</p>
        ) : recentMemories.isError ? (
          <div className="notes-list-state" role="alert">
            <p>{t("knowledgeContext.memoryLoadError")}</p>
            <button onClick={() => { void recentMemories.refetch(); }} type="button">{t("pageDetail.retry")}</button>
          </div>
        ) : (
          <>
            {needsCurrentMemory && currentMemory.isPending && (
              <p className="notes-list-state" role="status">{t("knowledgeContext.loadingMemories")}</p>
            )}
            {needsCurrentMemory && currentMemory.isError && (
              <div className="notes-list-state" role="alert">
                <p>{t("knowledgeContext.memoryLoadError")}</p>
                <button onClick={() => { void currentMemory.refetch(); }} type="button">{t("pageDetail.retry")}</button>
              </div>
            )}
            {needsCurrentMemory && currentMemory.isSuccess && !currentMemory.data && (
              <p className="notes-list-state" role="status">{t("memoryDetail.notFoundTitle")}</p>
            )}
            {visibleMemories.length === 0 && !(needsCurrentMemory && currentMemory.isPending) ? (
              <p className="notes-list-state">{normalizedFilter ? t("knowledgeContext.noMatches") : t("knowledgeContext.noMemories")}</p>
            ) : (
              <ul className="notes-page-list">
                {visibleMemories.map((memory) => {
                  const content = memory.content.trim().split(/\r?\n/)[0]?.trim() ?? "";
                  const title = memory.title.trim();
                  const preview = content || title || t("memoryDetail.untitledMemory");
                  const active = currentMemoryId === memory.source_id;
                  return (
                    <li key={memory.source_id}>
                      <button
                        aria-current={active ? "page" : undefined}
                        aria-label={t("knowledgeContext.openMemory", { title: title || preview })}
                        className="notes-page-button notes-memory-button"
                        data-active={active ? "true" : undefined}
                        disabled={!onOpenMemory}
                        onClick={() => onOpenMemory?.(memory.source_id)}
                        type="button"
                      >
                        <span className="notes-memory-preview">{preview}</span>
                        {content && title && title !== content && <span className="notes-memory-title">{title}</span>}
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
          </>
        )}
      </div>
    </section>
  );
}
