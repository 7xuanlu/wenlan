// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useId, useMemo, useState } from "react";
import { DotsSixVertical, FileText } from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import { readRecentPageHistory } from "../../../lib/recentPages";
import { orderWikiRecentEntries, readWikiCustomOrder, writeWikiCustomOrder } from "../../../lib/wikiNotesPreferences";
import { useNoteReorderDrag } from "./useNoteReorderDrag";
import "./recentNotesPanel.css";

export interface RecentNotesPanelProps {
  readonly currentPageId?: string | null;
  readonly revision?: number;
  readonly mode?: "recent" | "custom";
  readonly onOpenPage?: (id: string) => void;
}

export function RecentNotesPanel({ currentPageId = null, revision = 0, mode = "recent", onOpenPage }: RecentNotesPanelProps) {
  const { t } = useTranslation();
  const reorderHintId = useId();
  const isManualOrder = mode === "custom";
  const entries = useMemo(() => readRecentPageHistory().entries, [revision, currentPageId]);
  const [customOrder, setCustomOrder] = useState<readonly string[]>(() => readWikiCustomOrder());
  const orderedEntries = useMemo(() => mode === "custom" ? orderWikiRecentEntries(entries, customOrder) : entries, [entries, mode, customOrder]);
  const orderedIds = useMemo(() => orderedEntries.map(({ id }) => id), [orderedEntries]);
  useEffect(() => {
    if (mode !== "custom") return;
    setCustomOrder(current => current.length === orderedIds.length && current.every((id, index) => id === orderedIds[index]) ? current : orderedIds);
    writeWikiCustomOrder(orderedIds);
  }, [mode, orderedIds]);
  const reorder = useCallback((id: string, toIndex: number) => {
    const fromIndex = orderedIds.indexOf(id);
    if (fromIndex < 0 || toIndex < 0 || toIndex >= orderedIds.length || fromIndex === toIndex) return;
    const next = [...orderedIds];
    next.splice(fromIndex, 1);
    next.splice(toIndex, 0, id);
    setCustomOrder(next);
    writeWikiCustomOrder(next);
  }, [orderedIds]);
  const drag = useNoteReorderDrag(orderedIds, reorder, mode === "custom");

  return (
    <section className="wiki-recent-notes" aria-label={isManualOrder ? t("pages.recent.custom") : t("pages.recent.title")}>
      {mode === "custom" && <span id={reorderHintId} className="wiki-recent-sr-only">{t("pages.recent.reorderHint", { defaultValue: "Use arrow keys to reorder." })}</span>}
      {orderedEntries.length === 0 ? (
        <p className="wiki-recent-notes-empty">{t("pages.recent.empty")}</p>
      ) : (
        <ul className="wiki-recent-notes-list">
          {orderedEntries.map(entry => (
            <li key={entry.id} {...drag.rowProps(entry.id)}>
              {mode === "custom" && (
                <button
                  type="button"
                  className="wiki-recent-note-grip"
                  aria-label={t("pages.recent.dragNote", { title: entry.title, defaultValue: `Drag to reorder ${entry.title}` })}
                  aria-describedby={reorderHintId}
                  title={t("pages.recent.dragNote", { title: entry.title, defaultValue: `Drag to reorder ${entry.title}` })}
                  onPointerDown={event => drag.begin(event, entry.id)}
                  onKeyDown={event => {
                    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
                    event.preventDefault();
                    const index = orderedIds.indexOf(entry.id);
                    reorder(entry.id, index + (event.key === "ArrowUp" ? -1 : 1));
                  }}
                >
                  <DotsSixVertical aria-hidden="true" size={16} />
                </button>
              )}
              <button
                type="button"
                className="wiki-recent-note"
                aria-label={entry.title}
                title={entry.title}
                aria-current={entry.id === currentPageId ? "page" : undefined}
                onClick={() => onOpenPage?.(entry.id)}
                disabled={!onOpenPage}
              >
                <FileText aria-hidden="true" size={14} />
                <span>{entry.title}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
