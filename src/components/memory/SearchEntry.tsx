// SPDX-License-Identifier: AGPL-3.0-only
import { useTranslation } from "react-i18next";

interface SearchEntryProps {
  compact?: boolean;
  disabled?: boolean;
  expanded?: boolean;
  onOpen: (trigger: HTMLButtonElement) => void;
}

export function SearchEntry({ compact = false, disabled = false, expanded = false, onOpen }: SearchEntryProps) {
  const { t } = useTranslation();
  const modifier = /Mac|iPhone|iPad/.test(navigator.platform) ? "⌘" : "Ctrl";
  return (
    <button
      aria-label={t("main.searchButton")}
      aria-expanded={expanded}
      disabled={disabled}
      className={`notes-search-entry${compact ? " notes-search-entry--compact" : ""}`}
      onClick={(event) => onOpen(event.currentTarget)}
      title={compact ? `${t("main.searchButton")} (${t("main.searchShortcut", { modifier })})` : undefined}
      type="button"
    >
      <svg aria-hidden="true" fill="none" height="16" stroke="currentColor" viewBox="0 0 24 24" width="16">
        <path d="m21 21-4.35-4.35m2.35-5.65a8 8 0 1 1-16 0 8 8 0 0 1 16 0Z" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.8" />
      </svg>
      {!compact && <span className="notes-search-entry-label">{t("main.searchButton")}</span>}
      {!compact && <kbd className="notes-search-entry-shortcut">{t("main.searchShortcut", { modifier })}</kbd>}
    </button>
  );
}
