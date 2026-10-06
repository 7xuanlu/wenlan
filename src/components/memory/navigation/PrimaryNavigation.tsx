import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { GlobalNavigation } from "./viewState";
import { SpaceMark } from "./SpaceMark";
import { DEFAULT_VISIBLE_NAVIGATION, NAVIGATION_DESTINATION_ORDER, readNavigationPreferences, sanitizeVisibleNavigation, writeNavigationPreferences } from "./navigationPreferences";

type PrimaryNavigationLabels = {
  readonly backToMore: string;
  readonly customizationHint: string;
  readonly customize: string;
  readonly resetNavigation: string;
  readonly entities: string;
  readonly graph: string;
  readonly memories: string;
  readonly more: string;
  readonly navigation: string;
  readonly pages: string;
  readonly sources: string;
  readonly spaces: string;
};

type PrimaryNavigationProps = {
  readonly active: GlobalNavigation | null;
  readonly labels: PrimaryNavigationLabels;
  readonly onNavigateEntities?: () => void;
  readonly onNavigateGraph?: () => void;
  readonly onNavigateLog?: () => void;
  readonly onNavigatePages?: () => void;
  readonly onNavigateSources?: () => void;
  readonly onNavigateSpaces: (create: boolean) => void;
};

type NavButtonProps = {
  readonly active: boolean;
  readonly compact?: boolean;
  readonly icon: React.ReactNode;
  readonly label: string;
  readonly onClick?: () => void;
};

function NavButton({ active, compact = false, icon, label, onClick }: NavButtonProps) {
  if (!onClick) return null;
  return (
    <button
      aria-current={active ? "page" : undefined}
      aria-label={label}
      className={compact ? "notes-rail-button" : "notes-more-item"}
      data-active={active ? "true" : undefined}
      onClick={onClick}
      title={compact ? label : undefined}
      type="button"
    >
      <span aria-hidden="true" className="notes-navigation-glyph">{icon}</span>
      {!compact && <span>{label}</span>}
      {active && <span aria-hidden="true" className="notes-nav-active-marker" data-primary-navigation-active-marker="true" />}
    </button>
  );
}

const iconStyle = { color: "currentColor" } as const;
const pageIcon = <svg aria-hidden="true" data-navigation-icon="wiki-page" height="16" style={iconStyle} viewBox="0 0 24 24" width="16"><path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.8" /></svg>;
const entityIcon = <svg aria-hidden="true" data-navigation-icon="entities" height="16" style={iconStyle} viewBox="0 0 24 24" width="16"><path d="M5 9h14M4 15h14M10 4 8 20M16 4l-2 16" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.8" /></svg>;
const graphIcon = <svg aria-hidden="true" data-navigation-icon="graph" height="16" style={iconStyle} viewBox="0 0 24 24" width="16"><circle cx="5" cy="6" fill="none" r="2" stroke="currentColor" strokeWidth="2" /><circle cx="19" cy="6" fill="none" r="2" stroke="currentColor" strokeWidth="2" /><circle cx="12" cy="18" fill="none" r="2" stroke="currentColor" strokeWidth="2" /><path d="M7 6h10M6 8l5 8M18 8l-5 8" fill="none" stroke="currentColor" strokeWidth="2" /></svg>;
const memoryIcon = <svg aria-hidden="true" data-navigation-icon="brain" fill="none" height="16" style={iconStyle} viewBox="0 0 24 24" width="16">{[
  "M15.5 13a3.5 3.5 0 0 0 -3.5 3.5v1a3.5 3.5 0 0 0 7 0v-1.8",
  "M8.5 13a3.5 3.5 0 0 1 3.5 3.5v1a3.5 3.5 0 0 1 -7 0v-1.8",
  "M17.5 16a3.5 3.5 0 0 0 0 -7h-.5",
  "M19 9.3v-2.8a3.5 3.5 0 0 0 -7 0",
  "M6.5 16a3.5 3.5 0 0 0 0 -7h.5",
  "M5 9.3v-2.8a3.5 3.5 0 0 1 7 0v10",
].map((d) => <path d={d} key={d} stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.8" />)}</svg>;
const sourceIcon = <svg aria-hidden="true" data-navigation-icon="sources-intake-tray" height="16" style={iconStyle} viewBox="0 0 24 24" width="16"><path d="M7 4v6M12 4v6M17 4v6" fill="none" stroke="currentColor" strokeLinecap="round" strokeWidth="1.8" /><path d="M5 13l1.5 5h11l1.5-5" fill="none" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.8" /></svg>;

export function PrimaryNavigation({
  active,
  labels,
  onNavigateEntities,
  onNavigateGraph,
  onNavigateLog,
  onNavigatePages,
  onNavigateSources,
  onNavigateSpaces,
}: PrimaryNavigationProps) {
  const [visible, setVisible] = useState(readNavigationPreferences);
  const [moreOpen, setMoreOpen] = useState(false);
  const [customizing, setCustomizing] = useState(false);
  const [panelPosition, setPanelPosition] = useState({ left: 43, top: 0, maxHeight: 0 });
  const moreRef = useRef<HTMLDivElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const moreButtonRef = useRef<HTMLButtonElement>(null);
  const backButtonRef = useRef<HTMLButtonElement>(null);
  const customizeButtonRef = useRef<HTMLButtonElement>(null);
  const focusAfterModeChange = useRef<"back" | "customize" | null>(null);
  const options = {
    pages: { icon: pageIcon, onClick: onNavigatePages },
    spaces: { icon: <SpaceMark active={active === "spaces"} inheritColor />, onClick: () => onNavigateSpaces(false) },
    graph: { icon: graphIcon, onClick: onNavigateGraph },
    sources: { icon: sourceIcon, onClick: onNavigateSources },
    memories: { icon: memoryIcon, onClick: onNavigateLog },
    entities: { icon: entityIcon, onClick: onNavigateEntities },
  };
  const destinations = NAVIGATION_DESTINATION_ORDER.map((key) => ({ key, label: labels[key], ...options[key] })).filter((destination) => destination.onClick !== undefined);
  const railDestinations = destinations.filter((destination) => visible.includes(destination.key));
  const hiddenDestinations = destinations.filter((destination) => !visible.includes(destination.key));
  const moreActive = hiddenDestinations.some((destination) => destination.key === active);
  // Customization replaces the hidden destination list, so its current row carries
  // the same marker while the rail continues to mark pinned current destinations.
  const moreMarked = moreActive && !moreOpen;

  const closeMore = () => {
    setMoreOpen(false);
    setCustomizing(false);
  };

  useEffect(() => {
    if (!moreOpen) return;
    const closeOutside = (event: PointerEvent) => {
      if (!moreRef.current?.contains(event.target as Node)) {
        setMoreOpen(false);
        setCustomizing(false);
      }
    };
    document.addEventListener("pointerdown", closeOutside);
    return () => document.removeEventListener("pointerdown", closeOutside);
  }, [moreOpen]);

  useLayoutEffect(() => {
    if (!moreOpen) return;
    const positionPanel = () => {
      const anchor = moreRef.current?.getBoundingClientRect();
      const panel = panelRef.current?.getBoundingClientRect();
      if (!anchor || !panel) return;
      const margin = 8;
      const maxHeight = Math.max(0, window.innerHeight - margin * 2);
      const left = Math.max(margin, Math.min(anchor.right + 7, window.innerWidth - panel.width - margin)) - anchor.left;
      const top = Math.max(margin, Math.min(anchor.top, window.innerHeight - Math.min(panel.height, maxHeight) - margin)) - anchor.top;
      setPanelPosition((current) => current.left === left && current.top === top && current.maxHeight === maxHeight
        ? current : { left, top, maxHeight });
    };
    positionPanel();
    window.addEventListener("resize", positionPanel);
    window.addEventListener("scroll", positionPanel, true);
    return () => {
      window.removeEventListener("resize", positionPanel);
      window.removeEventListener("scroll", positionPanel, true);
    };
  }, [moreOpen, customizing, visible]);

  useLayoutEffect(() => {
    if (focusAfterModeChange.current === "back") backButtonRef.current?.focus();
    if (focusAfterModeChange.current === "customize") customizeButtonRef.current?.focus();
    focusAfterModeChange.current = null;
  }, [customizing]);

  const updateVisible = (next: readonly GlobalNavigation[]) => {
    const sanitized = sanitizeVisibleNavigation(next);
    setVisible(sanitized);
    writeNavigationPreferences(sanitized);
  };
  const navigate = (action: (() => void) | undefined) => action && (() => {
    closeMore();
    action();
  });

  return (
    <nav aria-label={labels.navigation} className="notes-rail-nav">
      {railDestinations.map((destination) => (
        <NavButton active={active === destination.key} compact icon={destination.icon} key={destination.key} label={destination.label} onClick={destination.onClick} />
      ))}
      <div
        className="notes-more-anchor"
        data-sidebar-escape-scope={moreOpen ? "true" : undefined}
        onKeyDown={(event) => {
          if (event.key !== "Escape") return;
          event.preventDefault();
          event.stopPropagation();
          closeMore();
          moreButtonRef.current?.focus();
        }}
        ref={moreRef}
      >
        <button
          aria-controls={moreOpen ? "notes-more-destinations" : undefined}
          aria-current={moreMarked ? "page" : undefined}
          aria-expanded={moreOpen}
          aria-label={labels.more}
          className="notes-rail-button"
          data-active={moreMarked ? "true" : undefined}
          data-sidebar-more="true"
          onClick={() => {
            setMoreOpen((current) => !current);
            setCustomizing(false);
          }}
          ref={moreButtonRef}
          title={labels.more}
          type="button"
        >
          <span aria-hidden="true" className="notes-navigation-glyph"><svg aria-hidden="true" fill="currentColor" height="16" viewBox="0 0 24 24" width="16"><circle cx="5" cy="12" r="1.5" /><circle cx="12" cy="12" r="1.5" /><circle cx="19" cy="12" r="1.5" /></svg></span>
          {moreMarked && <span aria-hidden="true" className="notes-nav-active-marker" data-primary-navigation-active-marker="true" />}
        </button>
        {moreOpen && (
          <div
            aria-label={customizing ? labels.customize : labels.more}
            className="notes-more-panel"
            id="notes-more-destinations"
            ref={panelRef}
            role="group"
            style={{ left: panelPosition.left, top: panelPosition.top, maxHeight: panelPosition.maxHeight || undefined }}
          >
            {customizing ? (
              <>
                <button
                  className="notes-more-item notes-customize-back"
                  onClick={() => {
                    focusAfterModeChange.current = "customize";
                    setCustomizing(false);
                  }}
                  ref={backButtonRef}
                  type="button"
                >
                  <span aria-hidden="true" className="notes-navigation-glyph"><svg aria-hidden="true" fill="none" height="16" stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.8" viewBox="0 0 24 24" width="16"><path d="m14 6-6 6 6 6" /></svg></span>
                  <span>{labels.backToMore}</span>
                </button>
                <p className="notes-customize-hint">{labels.customizationHint}</p>
                {destinations.map((destination) => {
                  const current = active === destination.key && !visible.includes(destination.key);
                  return (
                    <label aria-current={current ? "page" : undefined} className="notes-navigation-option" data-active={current ? "true" : undefined} key={destination.key}>
                      <input
                        checked={visible.includes(destination.key)}
                        onChange={(event) => updateVisible(event.target.checked ? [...visible, destination.key] : visible.filter((key) => key !== destination.key))}
                        type="checkbox"
                      />
                      <span aria-hidden="true" className="notes-navigation-glyph">{destination.icon}</span>
                      <span>{destination.label}</span>
                      {current && <span aria-hidden="true" className="notes-nav-active-marker" data-primary-navigation-active-marker="true" />}
                    </label>
                  );
                })}
                <button className="notes-more-item notes-customize-reset" onClick={() => updateVisible(DEFAULT_VISIBLE_NAVIGATION)} type="button">{labels.resetNavigation}</button>
              </>
            ) : (
              <>
                {hiddenDestinations.map((destination) => (
                  <NavButton active={active === destination.key} icon={destination.icon} key={destination.key} label={destination.label} onClick={navigate(destination.onClick)} />
                ))}
                <button
                  className="notes-more-item"
                  onClick={() => {
                    focusAfterModeChange.current = "back";
                    setCustomizing(true);
                  }}
                  ref={customizeButtonRef}
                  type="button"
                >
                  <span aria-hidden="true" className="notes-navigation-glyph"><svg aria-hidden="true" fill="none" height="16" stroke="currentColor" strokeLinecap="round" strokeWidth="1.8" viewBox="0 0 24 24" width="16"><path d="M4 6h16M4 12h16M4 18h16" /><circle cx="9" cy="6" fill="var(--mem-sidebar)" r="2" /><circle cx="15" cy="12" fill="var(--mem-sidebar)" r="2" /><circle cx="9" cy="18" fill="var(--mem-sidebar)" r="2" /></svg></span>
                  <span>{labels.customize}</span>
                </button>
              </>
            )}
          </div>
        )}
      </div>
    </nav>
  );
}
