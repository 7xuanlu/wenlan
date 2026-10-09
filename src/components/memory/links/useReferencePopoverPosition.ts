// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useLayoutEffect, useState, type RefObject } from "react";

export interface PopoverPosition { top: number; left: number; zIndex: number }

export function useReferencePopoverPosition(
  anchor: HTMLElement | null,
  boxRef: RefObject<HTMLElement | null>,
  width: number,
): PopoverPosition {
  const [position, setPopoverPosition] = useState<PopoverPosition>({ top: -9999, left: 8, zIndex: 60 });
  const updatePosition = useCallback(() => {
    if (!anchor?.isConnected) return;
    const anchorRect = anchor.getBoundingClientRect();
    const box = boxRef.current?.getBoundingClientRect();
    const height = box?.height ?? 130;
    const viewportWidth = Math.min(width, Math.max(0, window.innerWidth - 16));
    const flip = anchorRect.bottom + height + 8 > window.innerHeight && anchorRect.top - height - 8 > 0;
    // Portaled previews must sit above the surface that owns their anchor,
    // including the source reader's modal overlay.
    let zIndex = 60;
    for (let parent: HTMLElement | null = anchor; parent; parent = parent.parentElement) {
      const layer = Number.parseInt(window.getComputedStyle(parent).zIndex, 10);
      if (Number.isFinite(layer)) zIndex = Math.max(zIndex, layer + 1);
    }
    setPopoverPosition({
      zIndex,
      top: Math.min(Math.max(flip ? anchorRect.top - height - 8 : anchorRect.bottom + 6, 8), Math.max(8, window.innerHeight - height - 8)),
      left: Math.min(Math.max(anchorRect.left, 8), Math.max(8, window.innerWidth - viewportWidth - 8)),
    });
  }, [anchor, boxRef, width]);

  useLayoutEffect(() => {
    if (!anchor) return;
    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    const observer = typeof ResizeObserver === "undefined" || !boxRef.current ? null : new ResizeObserver(updatePosition);
    if (observer && boxRef.current) observer.observe(boxRef.current);
    return () => {
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
      observer?.disconnect();
    };
  }, [anchor, updatePosition, boxRef]);

  return position;
}
