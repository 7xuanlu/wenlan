// SPDX-License-Identifier: AGPL-3.0-only
import { useRef, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import "../pages/noteTabs.css";
import "./NoteInspectorTabs.css";

export type NoteInspectorTab = "info" | "canvas";

interface NoteInspectorTabsProps {
  readonly active: NoteInspectorTab;
  readonly onSelect: (tab: NoteInspectorTab) => void;
  readonly disabled?: boolean;
  readonly idPrefix: string;
}

const TABS: readonly NoteInspectorTab[] = ["info", "canvas"];

export function NoteInspectorTabs({ active, onSelect, disabled = false, idPrefix }: NoteInspectorTabsProps) {
  const { t } = useTranslation();
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const labels: Record<NoteInspectorTab, string> = {
    info: t("pageInspector.info"),
    canvas: t("pageCanvas.tabCanvas"),
  };

  const onKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>, current: NoteInspectorTab) => {
    if (disabled) {
      if (["ArrowRight", "ArrowLeft", "Home", "End", "Enter", " "].includes(event.key)) {
        event.preventDefault();
        event.stopPropagation();
      }
      return;
    }
    const index = TABS.indexOf(current);
    let nextIndex: number | null = null;
    if (event.key === "ArrowRight") nextIndex = (index + 1) % TABS.length;
    else if (event.key === "ArrowLeft") nextIndex = (index + TABS.length - 1) % TABS.length;
    else if (event.key === "Home") nextIndex = 0;
    else if (event.key === "End") nextIndex = TABS.length - 1;
    if (nextIndex === null) return;
    event.preventDefault();
    tabRefs.current[nextIndex]?.focus();
  };

  return (
    <div aria-label={t("pageInspector.label")} className="note-inspector-tabs" role="tablist">
      {TABS.map((tab, index) => (
        <button
          aria-controls={`${idPrefix}-panel`}
          aria-selected={active === tab}
          className="note-tab"
          data-active={active === tab}
          aria-disabled={disabled || undefined}
          aria-busy={disabled || undefined}
          id={`${idPrefix}-${tab}`}
          key={tab}
          onClick={() => { if (!disabled) onSelect(tab); }}
          onKeyDown={(event) => onKeyDown(event, tab)}
          ref={(element) => { tabRefs.current[index] = element; }}
          role="tab"
          tabIndex={active === tab ? 0 : -1}
          type="button"
        >
          <span>{labels[tab]}</span>
        </button>
      ))}
    </div>
  );
}

export default NoteInspectorTabs;
