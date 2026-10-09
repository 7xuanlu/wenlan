// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { clampRightSidebarWidth, MAX_RIGHT_SIDEBAR_WIDTH, MIN_RIGHT_SIDEBAR_WIDTH } from "./rightSidebarPreferences";
import "./sidebar-resize-handle.css";

const HIDDEN_SNAP_MAX = 40;
const KEYBOARD_STEP = 20;

function viewportIsNarrow(): boolean {
  return typeof window.matchMedia === "function" && window.matchMedia("(max-width: 1099px)").matches;
}

type Props = {
  width: number;
  maxWidth: number;
  enabled: boolean;
  onPreview: (width: number | null) => void;
  onCommit: (width: number) => void;
  onClose: () => void;
};

type Gesture = {
  pointerId: number;
  startX: number;
  startWidth: number;
  viewportWidth: number;
  viewportHeight: number;
  handle: HTMLDivElement;
};

export function rightSidebarWidthForRaw(rawWidth: number, maxWidth: number): number {
  if (rawWidth <= HIDDEN_SNAP_MAX) return 0;
  return clampRightSidebarWidth(rawWidth, maxWidth);
}

export function rightSidebarWidthForKey(key: string, width: number, maxWidth: number): number | "close" | null {
  const current = clampRightSidebarWidth(width, maxWidth);
  if (key === "Home") return "close";
  if (key === "End") return clampRightSidebarWidth(maxWidth, maxWidth);
  if (key === "ArrowLeft") return current < maxWidth ? clampRightSidebarWidth(current + KEYBOARD_STEP, maxWidth) : null;
  if (key === "ArrowRight") return current <= MIN_RIGHT_SIDEBAR_WIDTH ? "close" : clampRightSidebarWidth(current - KEYBOARD_STEP, maxWidth);
  return null;
}

export function RightSidebarResizeHandle({ width, maxWidth, enabled, onPreview, onCommit, onClose }: Props) {
  const { t } = useTranslation();
  const [preview, setPreview] = useState<number | null>(null);
  const gestureRef = useRef<Gesture | null>(null);
  const hoverBlockedUntilLeaveRef = useRef(false);
  const callbacksRef = useRef({ onPreview, onCommit, onClose });
  callbacksRef.current = { onPreview, onCommit, onClose };
  const valuesRef = useRef({ width, maxWidth, enabled });
  valuesRef.current = { width, maxWidth, enabled };
  const removeGlobalListenersRef = useRef<(() => void) | null>(null);

  const clearGesture = useCallback((notifyPreview: boolean) => {
    const gesture = gestureRef.current;
    if (!gesture) return;
    gestureRef.current = null;
    removeGlobalListenersRef.current?.();
    removeGlobalListenersRef.current = null;
    gesture.handle.classList.remove("is-resizing");
    gesture.handle.classList.add("is-hover-suppressed");
    hoverBlockedUntilLeaveRef.current = true;
    document.body.classList.remove("sidebar-resize-active");
    setPreview(null);
    if (notifyPreview) callbacksRef.current.onPreview(null);
    try {
      if (gesture.handle.hasPointerCapture(gesture.pointerId)) gesture.handle.releasePointerCapture(gesture.pointerId);
    } catch {
      // Pointer capture may already have been released by the browser.
    }
  }, []);

  const cancelGesture = useCallback(() => clearGesture(true), [clearGesture]);
  const moveGesture = useCallback((event: PointerEvent) => {
    const gesture = gestureRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    const rawWidth = gesture.startWidth + gesture.startX - event.clientX;
    const next = rightSidebarWidthForRaw(rawWidth, valuesRef.current.maxWidth);
    setPreview(next);
    callbacksRef.current.onPreview(next);
  }, []);

  const commitGesture = useCallback((event: PointerEvent) => {
    const gesture = gestureRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    const viewportChanged = window.innerWidth !== gesture.viewportWidth || window.innerHeight !== gesture.viewportHeight;
    if (viewportChanged || viewportIsNarrow()) { clearGesture(true); return; }
    const rawWidth = gesture.startWidth + gesture.startX - event.clientX;
    clearGesture(true);
    if (rawWidth <= HIDDEN_SNAP_MAX) {
      callbacksRef.current.onClose();
      return;
    }
    callbacksRef.current.onCommit(clampRightSidebarWidth(rawWidth, valuesRef.current.maxWidth));
  }, [clearGesture]);

  const onPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (gestureRef.current || !valuesRef.current.enabled || viewportIsNarrow() || event.button !== 0 || !event.isPrimary || (event.pointerType !== "mouse" && event.pointerType !== "pen")) return;
    event.preventDefault();
    event.stopPropagation();
    const handle = event.currentTarget;
    hoverBlockedUntilLeaveRef.current = false;
    gestureRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startWidth: valuesRef.current.width,
      viewportWidth: window.innerWidth,
      viewportHeight: window.innerHeight,
      handle,
    };
    handle.focus({ preventScroll: true });
    handle.classList.remove("is-hover-suppressed");
    handle.classList.add("is-pointer-focused", "is-resizing");
    document.body.classList.add("sidebar-resize-active");
    try { handle.setPointerCapture(event.pointerId); } catch { /* Document listeners remain active. */ }
    document.addEventListener("pointermove", moveGesture);
    const onUp = (nativeEvent: PointerEvent) => commitGesture(nativeEvent);
    const onCancel = (nativeEvent: PointerEvent) => {
      if (gestureRef.current?.pointerId === nativeEvent.pointerId) cancelGesture();
    };
    const onWindowBlur = () => cancelGesture();
    document.addEventListener("pointerup", onUp, true);
    document.addEventListener("pointercancel", onCancel, true);
    window.addEventListener("blur", onWindowBlur);
    window.addEventListener("resize", onWindowBlur);
    removeGlobalListenersRef.current = () => {
      document.removeEventListener("pointermove", moveGesture);
      document.removeEventListener("pointerup", onUp, true);
      document.removeEventListener("pointercancel", onCancel, true);
      window.removeEventListener("blur", onWindowBlur);
      window.removeEventListener("resize", onWindowBlur);
    };
  };

  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    hoverBlockedUntilLeaveRef.current = false;
    event.currentTarget.classList.remove("is-hover-suppressed", "is-pointer-focused");
    if (event.key === "Escape" && gestureRef.current) {
      event.preventDefault();
      event.stopPropagation();
      event.nativeEvent.stopImmediatePropagation();
      cancelGesture();
      return;
    }
    if (gestureRef.current) return;
    const next = rightSidebarWidthForKey(event.key, width, maxWidth);
    if (next === null) return;
    event.preventDefault();
    if (next === "close") callbacksRef.current.onClose();
    else callbacksRef.current.onCommit(next);
  };

  useEffect(() => {
    if (!enabled && gestureRef.current) cancelGesture();
  }, [enabled, cancelGesture]);

  useEffect(() => {
    const onWindowKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || !gestureRef.current) return;
      event.preventDefault();
      event.stopPropagation();
      event.stopImmediatePropagation();
      cancelGesture();
    };
    window.addEventListener("keydown", onWindowKeyDown, true);
    return () => window.removeEventListener("keydown", onWindowKeyDown, true);
  }, [cancelGesture]);

  useEffect(() => () => {
    if (gestureRef.current) clearGesture(true);
  }, [clearGesture]);

  if (!enabled) return null;
  const displayWidth = preview ?? width;
  const value = clampRightSidebarWidth(displayWidth, maxWidth);
  const hiddenPreview = preview === 0 || displayWidth === 0;
  return <div
    aria-label={t("sidebar.resizeRight")}
    aria-orientation="vertical"
    aria-valuemax={Math.min(MAX_RIGHT_SIDEBAR_WIDTH, Math.floor(maxWidth))}
    aria-valuemin={hiddenPreview ? 0 : MIN_RIGHT_SIDEBAR_WIDTH}
    aria-valuenow={hiddenPreview ? 0 : value}
    aria-valuetext={hiddenPreview ? t("sidebar.hidden") : t("sidebar.widthValue", { count: value })}
    className="sidebar-resize-handle sidebar-resize-handle--right"
    data-sidebar-escape-scope="true"
    data-testid="right-sidebar-resize-handle"
    onBlur={event => event.currentTarget.classList.remove("is-pointer-focused")}
    onKeyDown={onKeyDown}
    onLostPointerCapture={event => {
      if (gestureRef.current?.pointerId === event.pointerId) cancelGesture();
    }}
    onPointerDown={onPointerDown}
    onPointerEnter={event => { if (!gestureRef.current && !hoverBlockedUntilLeaveRef.current) event.currentTarget.classList.remove("is-hover-suppressed"); }}
    onPointerLeave={event => {
      hoverBlockedUntilLeaveRef.current = false;
      event.currentTarget.classList.remove("is-hover-suppressed");
    }}
    role="separator"
    tabIndex={0}
  />;
}
