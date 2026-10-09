// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  DEFAULT_SIDEBAR_WIDTH,
  MAX_SIDEBAR_WIDTH,
  MIN_SIDEBAR_WIDTH,
  expandedSidebarWidth,
  sanitizeSidebarWidth,
  sidebarLayoutWidth,
  type SidebarPreferences,
} from "./navigationPreferences";
import { SIDEBAR_NARROW_QUERY } from "./useResponsiveSidebar";
import "./sidebar-resize-handle.css";

function viewportIsNarrow(): boolean {
  return typeof window.matchMedia === "function" && window.matchMedia(SIDEBAR_NARROW_QUERY).matches;
}

const HIDDEN_SNAP_MAX = 40;
const ICONS_SNAP_MAX = 160;
const KEYBOARD_STEP = 20;

export function sidebarPreferencesForRawWidth(rawWidth: number, starting: SidebarPreferences): SidebarPreferences {
  if (rawWidth <= HIDDEN_SNAP_MAX) return { ...starting, visible: false };
  if (rawWidth <= ICONS_SNAP_MAX) return { ...starting, visible: true, mode: "icons" };
  return { ...starting, visible: true, mode: "labels", width: sanitizeSidebarWidth(rawWidth) ?? DEFAULT_SIDEBAR_WIDTH };
}

export function sidebarPreferencesForKey(key: string, current: SidebarPreferences): SidebarPreferences | null {
  const width = expandedSidebarWidth(current);
  if (key === "Home") return { ...current, visible: false };
  if (key === "End") return { ...current, visible: true, mode: "labels", width: MAX_SIDEBAR_WIDTH };
  if (key === "ArrowRight") {
    if (!current.visible) return { ...current, visible: true, mode: "icons" };
    if (current.mode === "icons") return { ...current, mode: "labels", width: MIN_SIDEBAR_WIDTH };
    if (width < MAX_SIDEBAR_WIDTH) return { ...current, width: Math.min(MAX_SIDEBAR_WIDTH, width + KEYBOARD_STEP) };
    return null;
  }
  if (key === "ArrowLeft") {
    if (!current.visible) return null;
    if (current.mode === "icons") return { ...current, visible: false };
    if (width <= MIN_SIDEBAR_WIDTH) return { ...current, mode: "icons" };
    return { ...current, width: Math.max(MIN_SIDEBAR_WIDTH, width - KEYBOARD_STEP) };
  }
  return null;
}

type Props = {
  preferences: SidebarPreferences;
  onPreview: (preferences: SidebarPreferences | null) => void;
  onCommit: (preferences: SidebarPreferences) => void;
  onHidden: () => void;
  enabled: boolean;
};

type Gesture = {
  pointerId: number;
  startX: number;
  startWidth: number;
  starting: SidebarPreferences;
  handle: HTMLDivElement;
};

export function SidebarResizeHandle({ preferences, onPreview, onCommit, onHidden, enabled }: Props) {
  const { t } = useTranslation();
  const [preview, setPreview] = useState<SidebarPreferences | null>(null);
  const gestureRef = useRef<Gesture | null>(null);
  const callbacksRef = useRef({ onPreview, onCommit, onHidden });
  callbacksRef.current = { onPreview, onCommit, onHidden };
  const preferencesRef = useRef(preferences);
  preferencesRef.current = preferences;

  const removeGlobalListenersRef = useRef<(() => void) | null>(null);

  const displayPreferences = preview ?? preferences;
  const currentWidth = sidebarLayoutWidth(displayPreferences);
  const valueText = !displayPreferences.visible
    ? t("sidebar.hidden")
    : displayPreferences.mode === "icons"
      ? t("sidebar.iconsOnly")
      : t("sidebar.widthValue", { count: expandedSidebarWidth(displayPreferences) });

  const clearGesture = useCallback((notifyPreview: boolean) => {
    const gesture = gestureRef.current;
    if (!gesture) return;
    gestureRef.current = null;
    removeGlobalListenersRef.current?.();
    removeGlobalListenersRef.current = null;
    gesture.handle.classList.remove("is-resizing");
    gesture.handle.classList.add("is-hover-suppressed");
    document.body.classList.remove("sidebar-resize-active");
    setPreview(null);
    if (notifyPreview) callbacksRef.current.onPreview(null);
    try {
      if (gesture.handle.hasPointerCapture(gesture.pointerId)) gesture.handle.releasePointerCapture(gesture.pointerId);
    } catch {
      // Pointer capture can already be gone when a browser reports cancellation.
    }
  }, []);

  const cancelGesture = useCallback(() => clearGesture(true), [clearGesture]);

  const commitGesture = useCallback((clientX: number, pointerId: number) => {
    const gesture = gestureRef.current;
    if (!gesture || gesture.pointerId !== pointerId) return;
    // Viewport changes can reach pointerup before React receives the media event.
    if (viewportIsNarrow()) { clearGesture(true); return; }
    const rawWidth = gesture.startWidth + clientX - gesture.startX;
    const next = sidebarPreferencesForRawWidth(rawWidth, gesture.starting);
    clearGesture(true);
    callbacksRef.current.onCommit(next);
    if (!next.visible) callbacksRef.current.onHidden();
  }, [clearGesture]);

  const moveGesture = useCallback((event: PointerEvent) => {
    const gesture = gestureRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    const next = sidebarPreferencesForRawWidth(gesture.startWidth + event.clientX - gesture.startX, gesture.starting);
    setPreview(next);
    callbacksRef.current.onPreview(next);
  }, []);

  const onPointerDown = (event: React.PointerEvent<HTMLDivElement>) => {
    if (gestureRef.current || !enabled || viewportIsNarrow() || !preferences.visible || event.button !== 0 || !event.isPrimary || (event.pointerType !== "mouse" && event.pointerType !== "pen")) return;
    event.preventDefault();
    const handle = event.currentTarget;
    const starting = preferencesRef.current;
    gestureRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startWidth: sidebarLayoutWidth(starting),
      starting,
      handle,
    };
    handle.focus({ preventScroll: true });
    handle.classList.remove("is-hover-suppressed");
    handle.classList.add("is-pointer-focused", "is-resizing");
    document.body.classList.add("sidebar-resize-active");
    try {
      handle.setPointerCapture(event.pointerId);
    } catch {
      // Document listeners below still cover browsers without pointer capture.
    }
    document.addEventListener("pointermove", moveGesture);
    const onUp = (nativeEvent: PointerEvent) => commitGesture(nativeEvent.clientX, nativeEvent.pointerId);
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
    event.currentTarget.classList.remove("is-hover-suppressed", "is-pointer-focused");
    if (event.key === "Escape" && gestureRef.current) {
      event.preventDefault();
      event.stopPropagation();
      event.nativeEvent.stopImmediatePropagation();
      cancelGesture();
      return;
    }
    if (gestureRef.current) return;
    const next = sidebarPreferencesForKey(event.key, preview ?? preferences);
    if (!next) return;
    event.preventDefault();
    callbacksRef.current.onPreview(null);
    callbacksRef.current.onCommit(next);
    if (!next.visible) callbacksRef.current.onHidden();
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

  if (!enabled || (!preferences.visible && !gestureRef.current)) return null;

  return (
    <div
      className="sidebar-resize-handle"
      role="separator"
      aria-label={t("sidebar.resize")}
      aria-orientation="vertical"
      aria-valuemin={0}
      aria-valuemax={MAX_SIDEBAR_WIDTH}
      aria-valuenow={currentWidth}
      aria-valuetext={valueText}
      data-sidebar-escape-scope="true"
      tabIndex={0}
      onPointerDown={onPointerDown}
      onPointerEnter={event => { if (!gestureRef.current) event.currentTarget.classList.remove("is-hover-suppressed"); }}
      onBlur={event => event.currentTarget.classList.remove("is-hover-suppressed", "is-pointer-focused")}
      onLostPointerCapture={(event) => {
        if (gestureRef.current?.pointerId === event.pointerId) cancelGesture();
      }}
      onKeyDown={onKeyDown}
      data-testid="sidebar-resize-handle"
    />
  );
}
