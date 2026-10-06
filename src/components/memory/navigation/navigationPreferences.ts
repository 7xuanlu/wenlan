// SPDX-License-Identifier: AGPL-3.0-only
import { readPreference, writePreference } from "../../../lib/preferenceStorage";
import type { GlobalNavigation } from "./viewState";

export type NavigationDestination = Exclude<GlobalNavigation, "home">;

export const NAVIGATION_PREFERENCE_KEY = "wenlan-navigation-v1";
export const NAVIGATION_DESTINATION_ORDER: readonly NavigationDestination[] = [
  "pages", "spaces", "graph", "sources", "memories", "entities",
];
export const DEFAULT_VISIBLE_NAVIGATION: readonly NavigationDestination[] = ["pages", "spaces", "graph"];

export function sanitizeVisibleNavigation(visible: readonly unknown[]): NavigationDestination[] {
  return NAVIGATION_DESTINATION_ORDER.filter((destination) => visible.includes(destination));
}

export function readNavigationPreferences(): NavigationDestination[] {
  const defaults = () => [...DEFAULT_VISIBLE_NAVIGATION];
  const stored = readPreference(NAVIGATION_PREFERENCE_KEY);
  if (stored === null) return defaults();
  try {
    const payload: unknown = JSON.parse(stored);
    if (typeof payload !== "object" || payload === null || !("version" in payload) ||
      payload.version !== 1 || !("visible" in payload) || !Array.isArray(payload.visible)) return defaults();
    const visible = sanitizeVisibleNavigation(payload.visible);
    return payload.visible.length > 0 && visible.length === 0 ? defaults() : visible;
  } catch {
    return defaults();
  }
}

export function writeNavigationPreferences(visible: readonly GlobalNavigation[]): void {
  writePreference(NAVIGATION_PREFERENCE_KEY, JSON.stringify({ version: 1, visible: sanitizeVisibleNavigation(visible) }));
}
