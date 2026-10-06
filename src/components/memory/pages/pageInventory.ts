// SPDX-License-Identifier: AGPL-3.0-only
import type { Page } from "../../../lib/tauri";

export type WikiInventoryScope = "all" | "files" | "drafts" | "unfiled";
export type PageCollectionScope = Exclude<WikiInventoryScope, "all">;

/** The daemon currently stores note files directly in one physical root. */
export function inventoryPageFilename(page: Page): string | null {
  const path = page.storage_path;
  if (!path || path.trim() !== path || path.startsWith(".")) return null;
  if (/[\\/:\u0000-\u001f\u007f]/u.test(path) || !/\.md$/iu.test(path)) return null;
  return path;
}

export function pageInventoryScope(page: Page): PageCollectionScope | null {
  if (page.entity_id || page.creation_kind === "entity") return null;
  if (page.status === "draft") return "drafts";
  return inventoryPageFilename(page) ? "files" : "unfiled";
}

export function pageMatchesInventoryScope(page: Page, scope: WikiInventoryScope): boolean {
  const collection = pageInventoryScope(page);
  return collection !== null && (scope === "all" || collection === scope);
}

function modifiedAt(page: Page): number {
  const value = Date.parse(page.last_modified || page.last_compiled || page.created_at);
  return Number.isFinite(value) ? value : 0;
}

export function collectPageInventory(active: readonly Page[], drafts: readonly Page[]): Page[] {
  const byId = new Map<string, Page>();
  // Active wins during publication while the independent draft query catches up.
  for (const page of [...drafts, ...active]) {
    if (pageInventoryScope(page) !== null) byId.set(page.id, page);
  }
  return [...byId.values()].sort(
    (left, right) => modifiedAt(right) - modifiedAt(left) || left.title.localeCompare(right.title),
  );
}

export function filterPageInventory(pages: readonly Page[], filter: string, locale: string): Page[] {
  const normalized = filter.trim().toLocaleLowerCase(locale);
  if (!normalized) return [...pages];
  return pages.filter((page) =>
    page.title.toLocaleLowerCase(locale).includes(normalized)
    || (inventoryPageFilename(page) ?? "").toLocaleLowerCase(locale).includes(normalized),
  );
}
