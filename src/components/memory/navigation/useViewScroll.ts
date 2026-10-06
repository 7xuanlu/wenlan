// SPDX-License-Identifier: AGPL-3.0-only
import { useLayoutEffect, useRef, type RefObject } from "react";

/** Session-only reading positions; wait for a lazy editor before restoring. */
export function useViewScroll(ref: RefObject<HTMLElement | null>, key: string, ready: boolean) {
  const positions = useRef(new Map<string, number>());
  const restoring = useRef(true);

  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    restoring.current = true;
    element.scrollTop = 0;
    element.scrollLeft = 0;
    const remember = () => {
      if (!restoring.current) positions.current.set(key, element.scrollTop);
    };
    element.addEventListener("scroll", remember);
    return () => { element.removeEventListener("scroll", remember); };
  }, [key, ref]);

  useLayoutEffect(() => {
    if (!ready) return;
    const frame = requestAnimationFrame(() => {
      if (ref.current) ref.current.scrollTop = positions.current.get(key) ?? 0;
      restoring.current = false;
    });
    return () => cancelAnimationFrame(frame);
  }, [key, ready, ref]);
}
