// SPDX-License-Identifier: AGPL-3.0-only
import { useLayoutEffect, useRef, type RefObject } from "react";

/** Session-only reading positions; wait for a lazy editor before restoring. */
export function useViewScroll(ref: RefObject<HTMLElement | null>, key: string, ready: boolean, scrollSelector?: string, sharedPositions?: Map<string, number>) {
  const positions = useRef(new Map<string, number>());
  const restoring = useRef(true);
  // Join the group session without changing hook order or losing reading
  // positions already collected by an editor during a frontend update.
  if (sharedPositions && positions.current !== sharedPositions) {
    for (const [destination, offset] of positions.current) {
      if (!sharedPositions.has(destination)) sharedPositions.set(destination, offset);
    }
    positions.current = sharedPositions;
  }

  useLayoutEffect(() => {
    const element = scrollSelector ? ref.current?.querySelector<HTMLElement>(scrollSelector) : ref.current;
    if (!element) return;
    restoring.current = true;
    element.scrollTop = 0;
    element.scrollLeft = 0;
    const remember = () => {
      if (!restoring.current) positions.current.set(key, element.scrollTop);
    };
    element.addEventListener("scroll", remember);
    // React may already have replaced the old document and clamped this shared
    // scroller before cleanup. Preserve the position recorded by its scroll
    // events instead of overwriting the previous note with the new geometry.
    return () => { element.removeEventListener("scroll", remember); };
  }, [key, ref, scrollSelector]);

  useLayoutEffect(() => {
    if (!ready) return;
    const frame = requestAnimationFrame(() => {
      const element = scrollSelector ? ref.current?.querySelector<HTMLElement>(scrollSelector) : ref.current;
      if (element) element.scrollTop = positions.current.get(key) ?? 0;
      restoring.current = false;
    });
    return () => cancelAnimationFrame(frame);
  }, [key, ready, ref, scrollSelector]);
}
