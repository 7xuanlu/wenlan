// SPDX-License-Identifier: AGPL-3.0-only
import { useTranslation } from "react-i18next";
import { SpaceMark } from "./navigation/SpaceMark";

export interface RecentSearchItem {
  readonly id: string;
  readonly title: string;
  readonly visitedAt: number;
  readonly kind: "page" | "space";
}

interface RecentlyOpenedProps {
  readonly query: string;
  readonly items: readonly RecentSearchItem[];
  readonly onSelect: (item: RecentSearchItem) => void;
}

export function RecentlyOpened({ query, items, onSelect }: RecentlyOpenedProps) {
  const { t } = useTranslation();
  if (query.trim()) return null;
  if (items.length === 0) return <p className="search-modal-hint">{t("main.searchPrompt")}</p>;

  return (
    <section aria-label={t("main.search.recentlyOpened")} className="search-recent-section">
      <p className="search-results-heading">{t("main.search.recentlyOpened")}</p>
      <div className="search-recent-list">
        {items.map((item) => {
          const kind = t(item.kind === "page" ? "main.search.pageKind" : "main.search.spaceKind");
          return (
            <button
              aria-label={t("main.search.openRecent", { kind, title: item.title })}
              className="search-recent-button"
              key={`${item.kind}:${item.id}`}
              onClick={() => onSelect(item)}
              type="button"
            >
              {item.kind === "page" ? (
                <svg aria-hidden="true" fill="none" height="16" stroke="currentColor" viewBox="0 0 24 24" width="16">
                  <path d="M6 3.75h8l4 4V20.25H6zM14 3.75v4h4M9 12h6M9 15.5h6" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.6" />
                </svg>
              ) : (
                <SpaceMark active={false} inheritColor />
              )}
              <span className="search-recent-title">{item.title}</span>
              <span className="search-recent-kind">{kind}</span>
            </button>
          );
        })}
      </div>
    </section>
  );
}
