// SPDX-License-Identifier: AGPL-3.0-only
import { useContext, useLayoutEffect, useRef } from "react";
import { WorkspaceNoteGroupContext, WorkspacePaneHostContext } from "./navigation/WorkspacePaneHost";

/** Measure the workspace before its Info panel takes space, keeping content origins stable. */
export function useUnobstructedContentWidth<T extends HTMLElement = HTMLDivElement>(propertyName: string, contentBox = false) {
  const frameRef = useRef<T>(null);
  const noteGroup = useContext(WorkspaceNoteGroupContext);
  const paneHost = useContext(WorkspacePaneHostContext);
  useLayoutEffect(() => {
    const frame = frameRef.current;
    const lane = frame?.parentElement;
    if (!frame || !lane) return;
    // Use the width before Info occupies the right edge. This keeps the first
    // character in place while only the available line length gets shorter.
    const group = noteGroup?.element;
    let lastWidth = -1;
    const measure = () => {
      const widthOf = (element: HTMLElement) => element.getBoundingClientRect().width;
      const laneStyle = contentBox ? getComputedStyle(lane) : null;
      const laneWidth = contentBox
        ? lane.clientWidth - parseFloat(laneStyle!.paddingLeft) - parseFloat(laneStyle!.paddingRight)
        : widthOf(lane);
      const width = group && !contentBox
        ? widthOf(group)
        : laneWidth + (paneHost ? widthOf(paneHost) : 0);
      if (width > 0 && width !== lastWidth) {
        lastWidth = width;
        frame.style.setProperty(propertyName, `${width}px`);
      }
    };
    measure();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    observer?.observe(group && !contentBox ? group : lane);
    if (paneHost && (!group || contentBox)) observer?.observe(paneHost);
    window.addEventListener("resize", measure);
    return () => { observer?.disconnect(); window.removeEventListener("resize", measure); };
  }, [noteGroup?.element, paneHost, propertyName, contentBox]);
  return frameRef;
}
