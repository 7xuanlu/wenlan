// SPDX-License-Identifier: AGPL-3.0-only
import { readPreference, writePreference } from "../../../lib/preferenceStorage";

export const RIGHT_SIDEBAR_PREFERENCE_KEY = "wenlan-right-sidebar-v1";
export const MIN_RIGHT_SIDEBAR_WIDTH = 320;
export const MAX_RIGHT_SIDEBAR_WIDTH = 600;

export function sanitizeRightSidebarWidth(value: unknown): number | undefined {
  if (typeof value !== "number" || !Number.isFinite(value)) return undefined;
  return Math.round(Math.min(MAX_RIGHT_SIDEBAR_WIDTH, Math.max(MIN_RIGHT_SIDEBAR_WIDTH, value)));
}

export function clampRightSidebarWidth(value: number, availableWidth: number): number {
  const max = Math.min(MAX_RIGHT_SIDEBAR_WIDTH, Math.max(MIN_RIGHT_SIDEBAR_WIDTH, Math.floor(availableWidth)));
  return Math.min(max, Math.max(MIN_RIGHT_SIDEBAR_WIDTH, Math.round(value)));
}

export function rightSidebarPreferenceKey(groupId?: string): string {
  return groupId === undefined ? RIGHT_SIDEBAR_PREFERENCE_KEY : `${RIGHT_SIDEBAR_PREFERENCE_KEY}:${groupId}`;
}

export function readRightSidebarWidth(groupId?: string): number | undefined {
  const stored = readPreference(rightSidebarPreferenceKey(groupId)) ??
    (groupId === undefined ? null : readPreference(RIGHT_SIDEBAR_PREFERENCE_KEY));
  if (stored === null) return undefined;
  try {
    const payload: unknown = JSON.parse(stored);
    if (typeof payload !== "object" || payload === null || !("version" in payload) || payload.version !== 1 || !("width" in payload)) return undefined;
    return sanitizeRightSidebarWidth(payload.width);
  } catch {
    return undefined;
  }
}

export function writeRightSidebarWidth(width: number, groupId?: string): void {
  const sanitized = sanitizeRightSidebarWidth(width);
  if (sanitized === undefined) return;
  writePreference(rightSidebarPreferenceKey(groupId), JSON.stringify({ version: 1, width: sanitized }));
}
