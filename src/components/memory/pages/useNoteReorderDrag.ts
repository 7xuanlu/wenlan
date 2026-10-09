// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { CSSProperties, PointerEvent as ReactPointerEvent } from "react";

type RowRect = { readonly top: number; readonly bottom: number; readonly height: number };
type Drag = {
  readonly pointerId: number;
  readonly sourceId: string;
  readonly order: readonly string[];
  readonly rects: ReadonlyMap<string, RowRect>;
  readonly startY: number;
  readonly startCenter: number;
  readonly listTop: number;
  readonly listBottom: number;
  readonly listLeft: number;
  readonly listRight: number;
  readonly rowPitch: number;
  readonly viewportWidth: number;
  readonly viewportHeight: number;
  moved: boolean;
  toIndex: number;
  deltaY: number;
};
type Projection = Pick<Drag, "sourceId" | "order" | "toIndex" | "deltaY" | "rects" | "rowPitch">;

function insertionIndex(drag: Drag, clientY: number): number {
  const center = drag.startCenter + clientY - drag.startY;
  const before = drag.order.filter(id => id !== drag.sourceId).filter(id => {
    const rect = drag.rects.get(id);
    return rect !== undefined && center >= rect.top + rect.height / 2;
  }).length;
  return Math.min(before, drag.order.length - 1);
}

export function useNoteReorderDrag(order: readonly string[], onReorder: (id: string, toIndex: number) => void, enabled = true) {
  const active = useRef<Drag | null>(null);
  const rows = useRef(new Map<string, HTMLLIElement>());
  const [projection, setProjection] = useState<Projection | null>(null);
  const latestOrder = useRef(order);
  const reorder = useRef(onReorder);
  latestOrder.current = order;
  reorder.current = onReorder;

  const cancel = useCallback(() => {
    active.current = null;
    setProjection(null);
  }, []);

  useEffect(() => {
    const onMove = (event: PointerEvent) => {
      const drag = active.current;
      if (!drag || drag.pointerId !== event.pointerId) return;
      drag.deltaY = event.clientY - drag.startY;
      if (!drag.moved && Math.abs(drag.deltaY) < 4) return;
      drag.moved = true;
      drag.toIndex = insertionIndex(drag, event.clientY);
      setProjection({ sourceId: drag.sourceId, order: drag.order, toIndex: drag.toIndex, deltaY: drag.deltaY, rects: drag.rects, rowPitch: drag.rowPitch });
      event.preventDefault();
    };
    const onUp = (event: PointerEvent) => {
      const drag = active.current;
      if (!drag || drag.pointerId !== event.pointerId) return;
      // A native resize event may arrive after pointerup. Compare the viewport
      // synchronously too, so an interrupted drag never commits stale geometry.
      const inside = window.innerWidth === drag.viewportWidth && window.innerHeight === drag.viewportHeight
        && event.clientX >= drag.listLeft && event.clientX <= drag.listRight
        && event.clientY >= drag.listTop && event.clientY <= drag.listBottom;
      const toIndex = insertionIndex(drag, event.clientY);
      active.current = null;
      setProjection(null);
      if (drag.moved && inside && toIndex !== drag.order.indexOf(drag.sourceId)) reorder.current(drag.sourceId, toIndex);
    };
    const onCancel = (event: PointerEvent) => { if (active.current?.pointerId === event.pointerId) cancel(); };
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") cancel(); };
    window.addEventListener("pointermove", onMove, { passive: false });
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onCancel);
    window.addEventListener("lostpointercapture", onCancel);
    window.addEventListener("blur", cancel);
    window.addEventListener("resize", cancel);
    window.addEventListener("scroll", cancel, true);
    window.addEventListener("keydown", onKey);
    return () => {
      cancel();
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onCancel);
      window.removeEventListener("lostpointercapture", onCancel);
      window.removeEventListener("blur", cancel);
      window.removeEventListener("resize", cancel);
      window.removeEventListener("scroll", cancel, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [cancel]);

  const orderKey = JSON.stringify(order);
  useLayoutEffect(() => {
    if (!enabled || (active.current !== null && JSON.stringify(active.current.order) !== orderKey)) cancel();
  }, [cancel, enabled, orderKey]);

  const begin = useCallback((event: ReactPointerEvent<HTMLButtonElement>, id: string) => {
    if (event.button !== 0 || !event.isPrimary || active.current) return;
    const row = rows.current.get(id);
    const list = row?.parentElement;
    if (!row || !list) return;
    const rects = new Map<string, RowRect>();
    for (const itemId of latestOrder.current) {
      const item = rows.current.get(itemId);
      if (item) {
        const rect = item.getBoundingClientRect();
        rects.set(itemId, { top: rect.top, bottom: rect.bottom, height: rect.height });
      }
    }
    const rect = rects.get(id);
    if (!rect) return;
    const listRect = list.getBoundingClientRect();
    const capturedRows = latestOrder.current.map(itemId => rects.get(itemId)).filter((value): value is RowRect => value !== undefined);
    const rowPitch = capturedRows.length > 1 ? capturedRows[1].top - capturedRows[0].top : rect.height;
    active.current = {
      pointerId: event.pointerId,
      sourceId: id,
      order: [...latestOrder.current],
      rects,
      startY: event.clientY,
      startCenter: rect.top + rect.height / 2,
      listTop: listRect.top,
      listBottom: listRect.bottom,
      listLeft: listRect.left,
      listRight: listRect.right,
      rowPitch,
      viewportWidth: window.innerWidth,
      viewportHeight: window.innerHeight,
      moved: false,
      toIndex: latestOrder.current.indexOf(id),
      deltaY: 0,
    };
    try { event.currentTarget.setPointerCapture(event.pointerId); } catch { /* Pointer capture is unavailable in some test DOMs. */ }
  }, []);

  const rowProps = useCallback((id: string) => {
    const current = projection;
    const sourceIndex = current?.order.indexOf(current.sourceId) ?? -1;
    const destinationIndex = current?.toIndex ?? -1;
    const index = current?.order.indexOf(id) ?? -1;
    let offset = 0;
    if (current && id === current.sourceId) offset = current.deltaY;
    else if (current && sourceIndex < destinationIndex && index > sourceIndex && index <= destinationIndex) offset = -current.rowPitch;
    else if (current && destinationIndex < sourceIndex && index >= destinationIndex && index < sourceIndex) offset = current.rowPitch;
    return {
      ref: (node: HTMLLIElement | null) => { if (node) rows.current.set(id, node); else rows.current.delete(id); },
      style: { transform: offset ? `translateY(${offset}px)` : undefined } as CSSProperties,
      className: current?.sourceId === id ? "wiki-recent-note-row is-dragging" : "wiki-recent-note-row",
    };
  }, [projection]);

  return { begin, rowProps };
}
