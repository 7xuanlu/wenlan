// SPDX-License-Identifier: AGPL-3.0-only
import { forwardRef, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { GearSix } from "@phosphor-icons/react";
import type { Page, Space } from "../../lib/tauri";
import IdentityCard from "./IdentityCard";
import { PrimaryNavigation } from "./navigation/PrimaryNavigation";
import { ReviewEnvironmentBadge } from "./navigation/ReviewEnvironmentBadge";
import type { GlobalNavigation } from "./navigation/viewState";
import { PageInventoryPanel } from "./pages/PageInventoryPanel";
import "./navigation/notes-sidebar.css";

interface SidebarProps {
  readonly activeNavigation?: GlobalNavigation | null;
  readonly collapsed: boolean;
  readonly currentPageId?: string | null;
  readonly currentSpaceId?: string | null;
  readonly onCreatePage?: () => void;
  readonly onEntityClick: (entityId: string) => void;
  readonly onNavigateEntities?: () => void;
  readonly onNavigateGraph?: () => void;
  readonly onNavigateHome?: () => void;
  readonly onNavigateLog?: () => void;
  readonly onNavigatePages?: () => void;
  readonly onNavigateSettings?: () => void;
  readonly onNavigateSources?: () => void;
  readonly onNavigateSpaces?: (create: boolean) => void;
  readonly onOpenAbout?: () => void;
  readonly onRequestClose?: () => void;
  readonly onSelectDraft?: (draftId: string, space: string | null) => void;
  readonly onSelectPage?: (page: Page) => void;
  readonly onSelectSpace: (space: Space) => void;
  readonly open?: boolean;
  readonly presentation?: "desktop" | "overlay";
  readonly recentPagesRevision?: number;
  readonly recentSpacesRevision?: number;
}

function closeAfterNavigation<Arguments extends readonly unknown[]>(
  navigate: (...arguments_: Arguments) => void,
  close: (() => void) | undefined,
): (...arguments_: Arguments) => void;
function closeAfterNavigation<Arguments extends readonly unknown[]>(
  navigate: ((...arguments_: Arguments) => void) | undefined,
  close: (() => void) | undefined,
): ((...arguments_: Arguments) => void) | undefined;
function closeAfterNavigation<Arguments extends readonly unknown[]>(
  navigate: ((...arguments_: Arguments) => void) | undefined,
  close: (() => void) | undefined,
): ((...arguments_: Arguments) => void) | undefined {
  if (!navigate) return undefined;
  return (...arguments_: Arguments) => {
    navigate(...arguments_);
    close?.();
  };
}

export default function Sidebar({
  activeNavigation = null,
  collapsed,
  currentPageId = null,
  onCreatePage,
  onEntityClick,
  onNavigateEntities,
  onNavigateGraph,
  onNavigateLog,
  onNavigatePages,
  onNavigateSettings,
  onNavigateSources,
  onNavigateSpaces = () => {},
  onOpenAbout,
  onRequestClose,
  onSelectDraft,
  onSelectPage,
  open = !collapsed,
  presentation = "desktop",
}: SidebarProps) {
  const { t } = useTranslation();
  const asideRef = useRef<HTMLElement>(null);
  const overlay = presentation === "overlay";
  const closeOverlay = overlay ? onRequestClose : undefined;
  const listVisible = open;
  const sidebarVisible = !overlay || open;

  useEffect(() => {
    if (!overlay || !open) return;
    const first = asideRef.current?.querySelector<HTMLElement>("button, [href], input, select, textarea, [tabindex]:not([tabindex='-1'])");
    first?.focus();
  }, [open, overlay]);

  const trapFocus = (event: React.KeyboardEvent<HTMLElement>) => {
    if (event.key !== "Tab" || !overlay) return;
    const focusable = asideRef.current?.querySelectorAll<HTMLElement>("button:not(:disabled), [href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])");
    if (!focusable || focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  return (
    <>
      {overlay && open && (
        <button
          aria-label={t("sidebar.close")}
          className="fixed inset-x-0 bottom-0 top-[52px] z-30 border-0 bg-black/40"
          onClick={onRequestClose}
          type="button"
        />
      )}
      <aside
        aria-hidden={!sidebarVisible}
        aria-label={t("sidebar.navigation")}
        className="memory-sidebar notes-workspace-sidebar"
        data-sidebar-overlay={overlay && open ? "true" : undefined}
        inert={!sidebarVisible}
        onKeyDown={trapFocus}
        ref={asideRef}
        style={{
          backgroundColor: "var(--mem-sidebar)",
          borderRight: "none",
          bottom: overlay ? 0 : undefined,
          height: overlay ? "auto" : "100%",
          left: overlay ? 0 : undefined,
          position: overlay ? "fixed" : "relative",
          top: overlay ? 52 : undefined,
          transform: overlay && !open ? "translateX(-100%)" : undefined,
          visibility: overlay && !open ? "hidden" : "visible",
          width: overlay ? 264 : listVisible ? 264 : 48,
          zIndex: overlay ? 40 : 2,
        }}
      >
        <div className="notes-icon-rail">
          <PrimaryNavigation
            active={activeNavigation}
            labels={{
              entities: t("sidebar.entities"),
              graph: t("sidebar.graph"),
              memories: t("sidebar.memories"),
              more: t("sidebar.more"),
              navigation: t("sidebar.navigation"),
              pages: t("sidebar.pages"),
              sources: t("sidebar.sources"),
              spaces: t("sidebar.spaces"),
              customize: t("sidebar.customize"),
              customizationHint: t("sidebar.customizationHint"),
              resetNavigation: t("sidebar.resetNavigation"),
              backToMore: t("sidebar.backToMore"),
            }}
            onNavigateEntities={closeAfterNavigation(onNavigateEntities, closeOverlay)}
            onNavigateGraph={closeAfterNavigation(onNavigateGraph, closeOverlay)}
            onNavigateLog={closeAfterNavigation(onNavigateLog, closeOverlay)}
            onNavigatePages={closeAfterNavigation(onNavigatePages, closeOverlay)}
            onNavigateSources={closeAfterNavigation(onNavigateSources, closeOverlay)}
            onNavigateSpaces={closeAfterNavigation(onNavigateSpaces, closeOverlay)}
          />
          <div className="notes-rail-utilities">
            <button
              aria-label={t("settings.title")}
              className="notes-rail-button"
              onClick={closeAfterNavigation(onNavigateSettings, closeOverlay)}
              title={t("settings.title")}
              type="button"
            >
              <span aria-hidden="true" className="notes-navigation-glyph"><GearSix /></span>
            </button>
            <IdentityCard
              onOpenDetail={closeAfterNavigation(onEntityClick, closeOverlay)}
              onOpenSettings={closeAfterNavigation(onNavigateSettings, closeOverlay)}
              onOpenAbout={closeAfterNavigation(onOpenAbout, closeOverlay)}
            />
          </div>
        </div>
          <div className="notes-workspace-panel" hidden={!listVisible} inert={!listVisible} style={{ display: listVisible ? undefined : "none" }}>
            <PageInventoryPanel
              currentPageId={currentPageId}
              onCreatePage={closeAfterNavigation(onCreatePage, closeOverlay)}
              onOpenDraft={closeAfterNavigation(onSelectDraft, closeOverlay)}
              onOpenPage={closeAfterNavigation(onSelectPage, closeOverlay)}
            />
            <div className="notes-workspace-footer">
              <ReviewEnvironmentBadge />
            </div>
          </div>
      </aside>
    </>
  );
}

/** Sidebar toggle button for use in the header. */
export const SidebarToggleButton = forwardRef<HTMLButtonElement, { readonly collapsed: boolean; readonly onToggle: () => void }>(function SidebarToggleButton({ collapsed, onToggle }, ref) {
  const { t } = useTranslation();
  return (
    <button
      aria-label={collapsed ? t("sidebar.show") : t("sidebar.hide")}
      data-sidebar-toggle="true"
      ref={ref}
      onClick={onToggle}
      className="flex items-center justify-center rounded-md transition-colors duration-150 hover:bg-[var(--mem-hover-strong)]"
      style={{
        width: 28,
        height: 28,
        color: "var(--mem-text-tertiary)",
      }}
      title={collapsed ? t("sidebar.show") : t("sidebar.hide")}
      type="button"
    >
      <svg
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        style={{ transition: "transform 200ms", transform: collapsed ? "scaleX(-1)" : "none" }}
      >
        <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
        <line x1="9" y1="3" x2="9" y2="21" />
      </svg>
    </button>
  );
});
