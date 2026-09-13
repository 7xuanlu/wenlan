// SPDX-License-Identifier: AGPL-3.0-only
//
// Where the Now summary sits on the Activity page. Per-device, so it lives in
// preferenceStorage like the theme and never travels with a Space.
//
// Same shape as src/lib/theme.ts — a module-level value, a listener set and
// useSyncExternalStore — so a change in Settings repaints the Activity page
// without a reload. Unlike theme.ts there is no Tauri event broadcast: only
// the main window renders Activity, so there is no second window to keep in
// sync, and emitting would make quick-capture and toast windows re-render for
// a preference they never read.

import { useSyncExternalStore } from "react";
import { readPreference, writePreference } from "./preferenceStorage";

/** Rail beside the feed, card above the feed, or a plain group inside it. */
export type ActivityNowLayout = "rail" | "card" | "timeline";

export const ACTIVITY_NOW_LAYOUTS: readonly ActivityNowLayout[] = [
  "rail",
  "card",
  "timeline",
];

/** Below this width the rail has nowhere to sit and renders as the card. */
export const ACTIVITY_RAIL_MIN_WIDTH = 900;

const STORAGE_KEY = "wenlan-activity-now-layout";

function parse(value: string | null): ActivityNowLayout {
  return ACTIVITY_NOW_LAYOUTS.includes(value as ActivityNowLayout)
    ? (value as ActivityNowLayout)
    : "rail";
}

const listeners = new Set<() => void>();
let current: ActivityNowLayout = parse(readPreference(STORAGE_KEY));

export function getActivityNowLayout(): ActivityNowLayout {
  return current;
}

export function setActivityNowLayout(layout: ActivityNowLayout): void {
  if (layout === current) return;
  current = layout;
  writePreference(STORAGE_KEY, layout);
  for (const fn of listeners) fn();
}

function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => {
    listeners.delete(fn);
  };
}

export function useActivityNowLayout(): [
  ActivityNowLayout,
  (layout: ActivityNowLayout) => void,
] {
  const layout = useSyncExternalStore(
    subscribe,
    getActivityNowLayout,
    getActivityNowLayout,
  );
  return [layout, setActivityNowLayout];
}

/** Test seam: reset the module value from storage. */
export function __resetActivityNowLayoutForTests(): void {
  current = parse(readPreference(STORAGE_KEY));
  for (const fn of listeners) fn();
}
