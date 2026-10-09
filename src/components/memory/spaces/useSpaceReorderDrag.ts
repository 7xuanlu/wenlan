import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { CSSProperties, MouseEvent as ReactMouseEvent, PointerEvent as ReactPointerEvent } from "react";
import type { Space } from "../../../lib/tauri";
import { canReorderTogether } from "./spaceHelpers";

type Reorder = (source: Space, target: Space) => void | boolean | Promise<boolean>;
type Projection = {
  sourceId: string;
  targetId: string;
  deltaY: number;
  order: readonly Space[];
  rects: ReadonlyMap<string, DOMRect>;
};
type Drag = {
  pointerId: number;
  sourceId: string;
  startY: number;
  grabOffsetY: number;
  moved: boolean;
  rects: Map<string, DOMRect>;
  order: readonly Space[];
  targetId: string;
  listRect: DOMRect;
};

function insertionTarget(drag: Drag, clientY: number): Space | null {
  const source = drag.order.find((space) => space.id === drag.sourceId);
  if (!source) return null;
  const group = drag.order.filter((space) => canReorderTogether(source, space) || space.id === source.id);
  const sourceRect = drag.rects.get(source.id);
  if (!sourceRect) return null;
  const center = clientY - drag.grabOffsetY + sourceRect.height / 2;
  let destinationIndex = 0;
  for (const space of group) {
    if (space.id === source.id) continue;
    const rect = drag.rects.get(space.id);
    if (!rect) break;
    const peerCenter = rect.top + rect.height / 2;
    // At the midpoint, an upward drag belongs before the hovered row.
    if (peerCenter > center || (clientY < drag.startY && Math.abs(peerCenter - center) < 0.5)) break;
    destinationIndex += 1;
  }
  return group[Math.min(destinationIndex, group.length - 1)] ?? source;
}

function makeProjection(drag: Drag, target: Space, deltaY: number): Projection {
  return { sourceId: drag.sourceId, targetId: target.id, deltaY, order: drag.order, rects: new Map(drag.rects) };
}

export function useSpaceReorderDrag(
  spaces: readonly Space[],
  disabled: boolean,
  identity: string,
  onReorder: Reorder,
) {
  const drag = useRef<Drag | null>(null);
  const [projection, setProjection] = useState<Projection | null>(null);
  const rows = useRef(new Map<string, HTMLDivElement>());
  const settling = useRef(false);
  const generation = useRef(0);
  const suppressClick = useRef(false);
  const identityRef = useRef(identity);
  const orderKey = JSON.stringify(spaces.map((space) => space.id));
  const renderedOrder = useRef(orderKey);
  const orderChanged = renderedOrder.current !== orderKey;
  const spacesRef = useRef(spaces);
  const reorderRef = useRef(onReorder);
  spacesRef.current = spaces;
  reorderRef.current = onReorder;

  const cancel = useCallback((pointerId?: number) => {
    if (pointerId !== undefined && drag.current?.pointerId !== pointerId) return;
    drag.current = null;
    settling.current = false;
    generation.current += 1;
    setProjection(null);
  }, []);

  useLayoutEffect(() => {
    if (orderChanged) {
      // React has moved the DOM rows into their saved order. Commit the zero
      // transforms without animation before allowing subsequent transitions;
      // otherwise WebKit animates the old projection from the new DOM position.
      rows.current.values().next().value?.getBoundingClientRect();
      renderedOrder.current = orderKey;
    }
  }, [orderChanged, orderKey]);

  useLayoutEffect(() => {
    if (identityRef.current !== identity || (disabled && !settling.current)) cancel();
    identityRef.current = identity;
  }, [identity, disabled, cancel]);

  useEffect(() => {
    const onMove = (event: PointerEvent) => {
      const active = drag.current;
      if (!active || event.pointerId !== active.pointerId) return;
      const deltaY = event.clientY - active.startY;
      if (!active.moved && Math.abs(deltaY) <= 4) return;
      active.moved = true;
      const target = insertionTarget(active, event.clientY);
      if (!target) return cancel(active.pointerId);
      active.targetId = target.id;
      setProjection(makeProjection(active, target, deltaY));
      event.preventDefault();
    };

    const onUp = (event: PointerEvent) => {
      const active = drag.current;
      if (!active || event.pointerId !== active.pointerId) return;
      const source = active.order.find((space) => space.id === active.sourceId);
      const xInside = event.clientX >= active.listRect.left && event.clientX <= active.listRect.right;
      const yInside = event.clientY >= active.listRect.top && event.clientY <= active.listRect.bottom;
      const hit = [...active.rects.entries()].find(([, rect]) => event.clientX >= rect.left && event.clientX <= rect.right && event.clientY >= rect.top && event.clientY <= rect.bottom)?.[0];
      const hitSpace = active.order.find((space) => space.id === hit);
      const target = active.moved ? insertionTarget(active, event.clientY) : null;
      drag.current = null;
      if (active.moved) {
        suppressClick.current = true;
        window.setTimeout(() => { suppressClick.current = false; }, 0);
      }
      if (!active.moved || !xInside || !yInside || !source || !target || target.id === source.id ||
          (hitSpace !== undefined && !canReorderTogether(source, hitSpace)) || !canReorderTogether(source, target)) {
        setProjection(null);
        return;
      }

      const from = active.order.findIndex((space) => space.id === source.id);
      const to = active.order.findIndex((space) => space.id === target.id);
      const sourceRect = active.rects.get(source.id);
      const targetRect = active.rects.get(target.id);
      if (!sourceRect || !targetRect || from === to) { setProjection(null); return; }
      const snapY = from < to
        ? targetRect.bottom - sourceRect.height - sourceRect.top
        : targetRect.top - sourceRect.top;
      const nextProjection = makeProjection(active, target, snapY);
      const token = ++generation.current;
      settling.current = true;
      setProjection(nextProjection);
      let result: void | boolean | Promise<boolean>;
      try { result = reorderRef.current(source, target); } catch { result = false; }
      const finish = (ok: boolean) => {
        if (generation.current !== token) return;
        if (!ok) setProjection(null);
        settling.current = false;
        setProjection(null);
      };
      if (result && typeof (result as Promise<boolean>).then === "function") {
        void (result as Promise<boolean>).then(finish, () => finish(false));
      } else finish(result !== false);
    };

    const onPointerCancel = (event: PointerEvent) => cancel(event.pointerId);
    const onLostCapture = (event: PointerEvent) => cancel(event.pointerId);
    const onBlur = () => cancel();
    const onResize = () => cancel();
    const onScroll = () => cancel();
    window.addEventListener("pointermove", onMove, { passive: false });
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onPointerCancel);
    window.addEventListener("lostpointercapture", onLostCapture);
    window.addEventListener("blur", onBlur);
    window.addEventListener("resize", onResize);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      drag.current = null;
      settling.current = false;
      generation.current += 1;
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onPointerCancel);
      window.removeEventListener("lostpointercapture", onLostCapture);
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("resize", onResize);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, [cancel]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") cancel(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [cancel]);

  const begin = useCallback((event: ReactPointerEvent<HTMLButtonElement>, space: Space) => {
    if (disabled || event.button !== 0 || !event.isPrimary || drag.current || settling.current) return;
    const row = rows.current.get(space.id);
    const list = row?.parentElement;
    if (!row || !list) return;
    const rects = new Map<string, DOMRect>();
    for (const item of spacesRef.current) {
      const itemRow = rows.current.get(item.id);
      if (itemRow) rects.set(item.id, itemRow.getBoundingClientRect());
    }
    const rowRect = rects.get(space.id);
    if (!rowRect) return;
    drag.current = {
      pointerId: event.pointerId,
      sourceId: space.id,
      startY: event.clientY,
      grabOffsetY: event.clientY - rowRect.top,
      moved: false,
      rects,
      order: [...spacesRef.current],
      targetId: space.id,
      listRect: list.getBoundingClientRect(),
    };
    try { event.currentTarget.setPointerCapture(event.pointerId); } catch { /* pointer capture is unavailable in some test DOMs */ }
  }, [disabled]);

  const rowProps = useCallback((space: Space) => {
    const sourceIndex = projection?.order.findIndex((item) => item.id === projection.sourceId) ?? -1;
    const targetIndex = projection?.order.findIndex((item) => item.id === projection.targetId) ?? -1;
    const rowIndex = projection?.order.findIndex((item) => item.id === space.id) ?? -1;
    const sourceHeight = projection?.rects.get(projection.sourceId)?.height ?? 0;
    let offset = 0;
    if (projection && space.id === projection.sourceId) {
      offset = projection.deltaY;
    } else if (projection && sourceIndex >= 0 && targetIndex >= 0 && rowIndex >= 0 && sourceIndex < targetIndex && rowIndex > sourceIndex && rowIndex <= targetIndex) {
      offset = -sourceHeight;
    } else if (projection && sourceIndex >= 0 && targetIndex >= 0 && rowIndex >= 0 && targetIndex < sourceIndex && rowIndex >= targetIndex && rowIndex < sourceIndex) {
      offset = sourceHeight;
    }
    const activelyDragging = drag.current?.sourceId === space.id;
    const settlingSource = projection?.sourceId === space.id && !activelyDragging;
    return {
      ref: (node: HTMLDivElement | null) => { if (node) rows.current.set(space.id, node); else rows.current.delete(space.id); },
      style: {
        transform: !orderChanged && offset ? `translateY(${offset}px)` : undefined,
        transition: orderChanged ? "none" : undefined,
      } as CSSProperties,
      className: activelyDragging ? "spaces-row-wrap is-dragging" : settlingSource ? "spaces-row-wrap is-settling" : "spaces-row-wrap",
    };
  }, [projection, orderChanged]);

  const onClickCapture = useCallback((event: ReactMouseEvent<HTMLDivElement>) => {
    if (!suppressClick.current) return;
    event.preventDefault();
    event.stopPropagation();
    suppressClick.current = false;
  }, []);

  return { begin, cancel, rowProps, onClickCapture, dragActive: drag.current !== null };
}
