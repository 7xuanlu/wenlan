// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ClockCounterClockwise, Folder } from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import type { Page } from "../../../lib/tauri";
import { MemoryInventoryPanel } from "./MemoryInventoryPanel";
import { PageInventoryPanel } from "../pages/PageInventoryPanel";
import type { WikiInventoryScope } from "../pages/pageInventory";
import "./context-browser.css";

type ContextKind = "pages" | "memories";

interface ContextBrowserProps {
  readonly kind: ContextKind;
  readonly inventoryScope: WikiInventoryScope;
  readonly browsingPages: boolean;
  readonly currentPageId: string | null;
  readonly currentMemoryId: string | null;
  readonly pageFilter?: string;
  readonly onPageFilterChange?: (filter: string) => void;
  readonly onBrowsePages: (scope: WikiInventoryScope) => void;
  readonly onCreatePage: (folderPath?: string) => void;
  readonly onSelectPage: (page: Page) => void;
  readonly onSelectDraft: (draftId: string, space: string | null) => void;
  readonly onSelectMemory: (sourceId: string) => void;
}

export function ContextBrowser(props: ContextBrowserProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const [position, setMenuPosition] = useState({ left: 12, top: 58 });
  const label = props.kind === "pages" ? t("sidebar.contextNotesAndFolders") : t("sidebar.contextRecentMemories");

  const updatePosition = () => {
    const trigger = triggerRef.current?.getBoundingClientRect();
    const panel = panelRef.current?.getBoundingClientRect();
    if (!trigger || !panel) return;
    const margin = 12;
    const left = Math.max(margin, Math.min(trigger.left, window.innerWidth - panel.width - margin));
    const top = Math.max(margin, Math.min(trigger.bottom + 6, window.innerHeight - panel.height - margin));
    setMenuPosition((current) => current.left === left && current.top === top ? current : { left, top });
  };

  useLayoutEffect(() => {
    if (!open) return;
    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    const focusTarget = panelRef.current?.querySelector<HTMLElement>("input:not(:disabled), button:not(:disabled)");
    focusTarget?.focus();
    return () => {
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const closeOutside = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", closeOutside);
    return () => document.removeEventListener("pointerdown", closeOutside);
  }, [open]);

  const close = () => setOpen(false);
  const closeAfter = <Arguments extends readonly unknown[]>(callback: (...arguments_: Arguments) => void) => (...arguments_: Arguments) => {
    close();
    callback(...arguments_);
  };

  return (
    <div
      className="context-browser-anchor"
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) close();
      }}
      onKeyDown={(event) => {
        if (event.key !== "Escape" || !open) return;
        event.preventDefault();
        event.stopPropagation();
        close();
        triggerRef.current?.focus();
      }}
      ref={rootRef}
    >
      <button
        aria-controls={open ? "workspace-context-browser" : undefined}
        aria-expanded={open}
        aria-haspopup="dialog"
        aria-label={label}
        className="context-browser-trigger"
        onClick={() => setOpen((value) => !value)}
        ref={triggerRef}
        title={label}
        type="button"
      >
        {props.kind === "pages" ? <Folder aria-hidden="true" size={18} /> : <ClockCounterClockwise aria-hidden="true" size={18} />}
        <span>{label}</span>
      </button>
      <div
        aria-label={label}
        aria-modal="false"
        className="context-browser-panel"
        hidden={!open}
        id="workspace-context-browser"
        inert={!open}
        ref={panelRef}
        role="dialog"
        style={{ left: position.left, top: position.top }}
      >
        {props.kind === "pages" ? (
          <PageInventoryPanel
            inventoryScope={props.inventoryScope}
            browsing={props.browsingPages}
            filter={props.pageFilter}
            onFilterChange={props.onPageFilterChange}
            onBrowse={closeAfter(props.onBrowsePages)}
            currentPageId={props.currentPageId}
            onCreatePage={closeAfter(props.onCreatePage)}
            onOpenDraft={closeAfter(props.onSelectDraft)}
            onOpenPage={closeAfter(props.onSelectPage)}
          />
        ) : (
          <MemoryInventoryPanel
            currentMemoryId={props.currentMemoryId}
            onOpenMemory={closeAfter(props.onSelectMemory)}
          />
        )}
      </div>
    </div>
  );
}
