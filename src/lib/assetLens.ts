// SPDX-License-Identifier: AGPL-3.0-only
import { readPreference, writePreference } from "./preferenceStorage";

export type AssetLens = "rows" | "cards";
export type AssetLensPage = "wiki" | "entities" | "spaces";

export function assetLensStorageKey(page: AssetLensPage): string {
  return `wenlan-${page}-view-mode`;
}

function isAssetLens(value: string | null): value is AssetLens {
  return value === "rows" || value === "cards";
}

/**
 * Reads the persisted rows/cards lens for a page. Unknown or unreadable
 * state (preview harness, private window) falls back to cards, the default
 * lens — mirroring `getStoredViewMode` in MemoryStream.
 */
export function readAssetLens(page: AssetLensPage): AssetLens {
  const stored = readPreference(assetLensStorageKey(page));
  return isAssetLens(stored) ? stored : "cards";
}

export function writeAssetLens(page: AssetLensPage, lens: AssetLens): void {
  writePreference(assetLensStorageKey(page), lens);
}
