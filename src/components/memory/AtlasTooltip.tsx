// SPDX-License-Identifier: AGPL-3.0-only
import { cloneElement, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import type { ReactElement } from "react";
import { createPortal } from "react-dom";

const OPEN_DELAY_MS = 200;

interface AtlasTooltipProps {
  content: string;
  children: ReactElement<{ "aria-describedby"?: string }>;
}

interface TooltipPosition {
  left: number;
  top: number;
}

/**
 * A small, viewport-clamped tooltip for graph controls. It deliberately uses
 * a portal so the compact toolbar can sit near the edge of a narrow window
 * without clipping the explanatory text inside an overflow container.
 */
export default function AtlasTooltip({ content, children }: AtlasTooltipProps) {
  const anchorRef = useRef<HTMLSpanElement>(null);
  const tooltipRef = useRef<HTMLSpanElement>(null);
  const openTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<TooltipPosition>({ left: 0, top: 0 });
  const [positioned, setPositioned] = useState(false);
  const tooltipId = useId();

  const clearTimer = () => {
    if (openTimer.current !== null) {
      clearTimeout(openTimer.current);
      openTimer.current = null;
    }
  };
  const openAfterDelay = () => {
    clearTimer();
    openTimer.current = setTimeout(() => setOpen(true), OPEN_DELAY_MS);
  };
  const close = () => {
    clearTimer();
    setOpen(false);
    setPositioned(false);
  };

  useEffect(() => clearTimer, []);

  useEffect(() => {
    if (!open) return;
    const dismiss = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.stopPropagation();
      setOpen(false);
      setPositioned(false);
    };
    window.addEventListener("keydown", dismiss, true);
    return () => window.removeEventListener("keydown", dismiss, true);
  }, [open]);

  useLayoutEffect(() => {
    if (!open || !anchorRef.current || !tooltipRef.current) return;
    const anchor = anchorRef.current.getBoundingClientRect();
    const tooltip = tooltipRef.current.getBoundingClientRect();
    const edge = 8;
    const gap = 7;
    const left = Math.min(
      Math.max(edge, anchor.left + anchor.width / 2 - tooltip.width / 2),
      Math.max(edge, window.innerWidth - tooltip.width - edge),
    );
    const above = anchor.top - tooltip.height - gap;
    const top = above >= edge ? above : Math.min(window.innerHeight - tooltip.height - edge, anchor.bottom + gap);
    setPosition({ left, top: Math.max(edge, top) });
    setPositioned(true);
  }, [open]);

  const describedBy = open
    ? [children.props["aria-describedby"], tooltipId].filter(Boolean).join(" ")
    : children.props["aria-describedby"];
  const trigger = cloneElement(children, { "aria-describedby": describedBy || undefined });

  return (
    <span
      ref={anchorRef}
      className="atlas-tooltip-anchor"
      onPointerEnter={openAfterDelay}
      onPointerLeave={close}
      onClick={close}
      onFocusCapture={() => {
        clearTimer();
        setOpen(true);
      }}
      onBlurCapture={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) close();
      }}
    >
      {trigger}
      {open && typeof document !== "undefined" && createPortal(
        <span
          ref={tooltipRef}
          id={tooltipId}
          role="tooltip"
          className="atlas-tooltip"
          style={{ left: position.left, top: position.top, visibility: positioned ? "visible" : "hidden" }}
        >
          {content}
        </span>,
        document.body,
      )}
    </span>
  );
}
