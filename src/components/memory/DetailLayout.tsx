// SPDX-License-Identifier: AGPL-3.0-only
import { useLayoutEffect, type ReactNode, type RefObject } from "react";
import { useUnobstructedContentWidth } from "./useUnobstructedContentWidth";

interface DetailLayoutProps {
  readonly children: ReactNode;
  /** Content-specific hooks only; shared geometry belongs in the layout CSS. */
  readonly className?: string;
}

/**
 * Centre the reading column on the window whenever the lane it sits in has room
 * for that, so side columns opening or closing around it never move the text.
 * readingSurface.css falls back to centring in the lane when it does not fit.
 */
function useWindowCentredReading(frameRef: RefObject<HTMLDivElement | null>) {
  useLayoutEffect(() => {
    const frame = frameRef.current;
    const lane = frame?.parentElement;
    if (!frame || !lane) return;
    const measure = () => {
      const box = lane.getBoundingClientRect();
      const style = getComputedStyle(lane);
      const left = box.left + lane.clientLeft + parseFloat(style.paddingLeft);
      const width = lane.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
      const frameMax = parseFloat(getComputedStyle(frame).maxWidth);
      const skew = (document.documentElement.clientWidth - (left + width) - left) / 2;
      if (width > 0 && Number.isFinite(frameMax) && (width - frameMax) / 2 >= Math.abs(skew)) {
        frame.style.setProperty("--reading-window-skew", `${skew}px`);
      } else {
        frame.style.removeProperty("--reading-window-skew");
      }
    };
    measure();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    observer?.observe(lane);
    window.addEventListener("resize", measure);
    return () => { observer?.disconnect(); window.removeEventListener("resize", measure); };
  }, [frameRef]);
}

/**
 * The reading template used by Memory, Topic, Wiki and Sources.
 * Compact is for atomic content; long-form allows a wider document measure.
 * Slots stay owned by each page: title/tools, notices, and the existing editor.
 * Typography and spacing live in readingSurface.css, not in each page.
 */
export function DetailReadingFrame({
  children,
  className,
  variant = "long-form",
}: DetailLayoutProps & { readonly variant?: "compact" | "long-form" }) {
  const frameRef = useUnobstructedContentWidth("--reading-unobstructed-width");
  useWindowCentredReading(frameRef);
  return (
    <div
      ref={frameRef}
      className={`reading-document-frame${variant === "compact" ? " reading-document-frame--compact" : ""}${className ? ` ${className}` : ""}`}
      data-detail-reading={variant}
    >
      {children}
    </div>
  );
}

/**
 * Put page-specific actions, metadata, connections and history in this slot.
 * PageInfoDrawer owns the shared panel chrome and responsive/focus behavior;
 * detailInfo.css owns section rhythm, headings, metadata grids and text styles.
 */
export function DetailInfoContent({ children, className }: DetailLayoutProps) {
  return (
    <div className={`detail-info-content${className ? ` ${className}` : ""}`} data-detail-info>
      {children}
    </div>
  );
}
