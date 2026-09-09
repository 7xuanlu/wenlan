// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { CaretDown, X } from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import { colorForEntityType, type GraphPalette } from "../../lib/graph/palette";
import { entityTypeLabel } from "../../lib/graph/typeFilter";

interface AtlasTypeFiltersProps {
  types: [string, number][];
  excluded: ReadonlySet<string>;
  palette: GraphPalette;
  onToggle: (type: string) => void;
  onReset: () => void;
}

interface PanelPosition {
  left: number;
  top: number;
}

const VIEWPORT_EDGE = 8;
const TRIGGER_GAP = 6;

export default function AtlasTypeFilters({
  types,
  excluded,
  palette,
  onToggle,
  onReset,
}: AtlasTypeFiltersProps) {
  const { t } = useTranslation();
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const panelId = `atlas-type-filter-${useId().replace(/:/g, "")}`;
  const titleId = `${panelId}-title`;
  const summaryId = `${panelId}-summary`;
  const [open, setOpen] = useState(false);
  const [position, setPanelPosition] = useState<PanelPosition>({ left: VIEWPORT_EDGE, top: VIEWPORT_EDGE });
  const [positioned, setPositioned] = useState(false);

  const total = types.length;
  const visible = types.reduce((count, [type]) => count + (excluded.has(type) ? 0 : 1), 0);
  const allVisible = visible === total;
  const filtered = visible < total;

  const close = (restoreFocus: boolean) => {
    setOpen(false);
    setPositioned(false);
    if (restoreFocus) triggerRef.current?.focus();
  };

  const updatePosition = () => {
    const trigger = triggerRef.current;
    const panel = panelRef.current;
    if (!trigger || !panel) return;

    const anchor = trigger.getBoundingClientRect();
    const panelRect = panel.getBoundingClientRect();
    const panelWidth = panelRect.width || Math.min(340, window.innerWidth - VIEWPORT_EDGE * 2);
    const panelHeight = panelRect.height || Math.min(360, window.innerHeight - VIEWPORT_EDGE * 2);
    const maxLeft = Math.max(VIEWPORT_EDGE, window.innerWidth - panelWidth - VIEWPORT_EDGE);
    const left = Math.min(Math.max(VIEWPORT_EDGE, anchor.left), maxLeft);
    const below = anchor.bottom + TRIGGER_GAP;
    const above = anchor.top - panelHeight - TRIGGER_GAP;
    const top = below + panelHeight <= window.innerHeight - VIEWPORT_EDGE
      ? below
      : above >= VIEWPORT_EDGE
        ? above
        : Math.min(Math.max(VIEWPORT_EDGE, below), Math.max(VIEWPORT_EDGE, window.innerHeight - panelHeight - VIEWPORT_EDGE));

    setPanelPosition({ left, top });
    setPositioned(true);
  };

  useLayoutEffect(() => {
    if (!open) return;
    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    return () => {
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
    };
  }, [open, total]);

  useEffect(() => {
    if (!open) return;
    panelRef.current?.focus({ preventScroll: true });

    const dismissPointer = (event: PointerEvent) => {
      const target = event.target as Node | null;
      if (target && (rootRef.current?.contains(target) || panelRef.current?.contains(target))) return;
      close(false);
    };
    const dismissFocus = (event: FocusEvent) => {
      const target = event.target as Node | null;
      if (target && (rootRef.current?.contains(target) || panelRef.current?.contains(target))) return;
      close(false);
    };
    const dismissEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      close(true);
    };

    document.addEventListener("pointerdown", dismissPointer);
    document.addEventListener("focusin", dismissFocus);
    // Capture prevents the Atlas document listener from clearing a selected node.
    document.addEventListener("keydown", dismissEscape, true);
    return () => {
      document.removeEventListener("pointerdown", dismissPointer);
      document.removeEventListener("focusin", dismissFocus);
      document.removeEventListener("keydown", dismissEscape, true);
    };
  }, [open]);

  const labelFor = (type: string) => t(`atlas.entityType.${type}`, { defaultValue: entityTypeLabel(type) });

  const renderType = ([type, count]: [string, number]) => {
    const label = labelFor(type);
    const enabled = !excluded.has(type);
    return (
      <button
        key={type}
        type="button"
        className="atlas-type-filter-option"
        aria-pressed={enabled}
        aria-label={label}
        title={`${label} · ${count}`}
        onClick={() => onToggle(type)}
      >
        <span
          className="atlas-type-filter-swatch"
          aria-hidden="true"
          style={{ color: colorForEntityType(type, palette) }}
        />
        <span className="atlas-type-filter-label">{label}</span>
      </button>
    );
  };

  return (
    <div ref={rootRef} className="atlas-type-filter">
      <button
        ref={triggerRef}
        type="button"
        className="atlas-type-filter-trigger atlas-content-secondary-toggle"
        aria-label={t("atlas.typesFilter")}
        aria-description={t("atlas.typesSummary", { visible, total })}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        onClick={() => {
          if (open) close(true);
          else {
            setPositioned(false);
            setOpen(true);
          }
        }}
      >
        <span>{t("atlas.typesLabel")}{filtered ? ` ${visible}/${total}` : ""}</span>
        <CaretDown size={12} aria-hidden="true" />
      </button>
      {open && typeof document !== "undefined" && createPortal(
        <div
          ref={panelRef}
          id={panelId}
          role="dialog"
          aria-labelledby={titleId}
          aria-describedby={summaryId}
          tabIndex={-1}
          className="atlas-type-filter-panel"
          style={{ left: position.left, top: position.top, visibility: positioned ? "visible" : "hidden" }}
          onKeyDownCapture={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              close(true);
            }
          }}
        >
          <div className="atlas-type-filter-header">
            <div>
              <h2 id={titleId}>{t("atlas.entityTypes")}</h2>
              <p id={summaryId}>{t("atlas.typesSummary", { visible, total })}</p>
            </div>
            <div className="atlas-type-filter-header-actions">
              <button
                type="button"
                className="atlas-type-filter-reset"
                disabled={allVisible}
                onClick={() => {
                  onReset();
                  panelRef.current?.focus({ preventScroll: true });
                }}
              >
                {t("atlas.restoreAllTypes")}
              </button>
              <button
                type="button"
                className="atlas-type-filter-close"
                aria-label={t("atlas.closeTypeFilters")}
                onClick={() => close(true)}
              >
                <X size={15} aria-hidden="true" />
              </button>
            </div>
          </div>
          <div className="atlas-type-filter-grid">
            {types.map(renderType)}
          </div>
        </div>,
        document.body,
      )}
    </div>
  );
}
