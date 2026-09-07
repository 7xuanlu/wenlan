// SPDX-License-Identifier: AGPL-3.0-only
import type { TFunction } from "i18next";
import type { Entity, EntityStatus, ListEntitiesRequest } from "../../../lib/tauri";

/** The three lifecycle tabs the view offers, one per {@link EntityStatus}. */
export type EntityTab = "established" | "detected" | "archived";

/** Type chip values. Fixed to the mock's four options; the daemon's vocabulary
 * has more canonical types, but the filter only exposes these. */
export type EntityTypeFilter = "all" | "concept" | "person" | "organization" | "place";

export type EntityMemoriesFilter = "any" | "none" | "some";

/** Page size for both the row list and "Load more" (spec: 100/page). */
export const ENTITIES_PAGE_SIZE = 100;

const TAB_STATUS: Record<EntityTab, EntityStatus> = {
  established: "established",
  detected: "detected",
  archived: "archived",
};

/** Search and chip state. Every tab renders controls for these and they
 * drive the list request the same way, so "Archive all matching" can read
 * back exactly what is active on any tab. */
export interface EntityFilters {
  readonly query: string;
  readonly type: EntityTypeFilter;
  readonly memories: EntityMemoriesFilter;
}

export const DEFAULT_FILTERS: EntityFilters = { query: "", type: "all", memories: "any" };

/** True once any chip or the search box has moved off its default. */
export function filtersActive(filters: EntityFilters): boolean {
  return filters.type !== "all" || filters.memories !== "any" || filters.query.trim() !== "";
}

/** Maps a tab plus the active filters to the daemon's list request,
 * including `limit`/`offset` for the page the caller is on. The search and
 * chips apply identically on every tab. */
export function entityListRequest(
  tab: EntityTab,
  filters: EntityFilters,
  offset: number,
): ListEntitiesRequest {
  const request: ListEntitiesRequest = {
    status: TAB_STATUS[tab],
    limit: ENTITIES_PAGE_SIZE,
    offset,
  };
  if (filters.type !== "all") request.entity_type = filters.type;
  if (filters.memories === "none") {
    request.min_memories = 0;
    request.max_memories = 0;
  } else if (filters.memories === "some") {
    request.min_memories = 1;
  }
  const query = filters.query.trim();
  if (query !== "") request.query = query;
  return request;
}

/** "{{n}} established · {{n}} detected · {{n}} archived" header subtitle. */
export function subtitle(
  counts: { established: number; detected: number; archived: number },
  t: TFunction,
): string {
  return t("entities.subtitle", counts);
}

/** The matchline sentence above each tab's table: unfiltered
 * ("N detected entities") vs. filtered ("N detected entities match"). Every
 * tab can be filtered, so the sentence follows the `filtered` flag on all
 * three. */
export function matchLabel(
  tab: EntityTab,
  count: number,
  filtered: boolean,
  t: TFunction,
): string {
  if (filtered) return t(`entities.matchFiltered_${tab}`, { count });
  return t(`entities.match_${tab}`, { count });
}

/** The "Filter" line read back in the archive-all-matching confirm dialog,
 * e.g. "Concept, no memories, name contains \"foo\"". */
export function filterReadback(filters: EntityFilters, t: TFunction): string {
  const parts = [
    filters.type === "all" ? t("entities.filters.typeAny") : t(`entities.filters.type_${filters.type}`),
    filters.memories === "any"
      ? t("entities.filters.memoriesAnyDescription")
      : filters.memories === "none"
        ? t("entities.filters.memoriesNoneDescription")
        : t("entities.filters.memoriesSomeDescription"),
  ];
  const query = filters.query.trim();
  if (query !== "") parts.push(t("entities.filters.queryDescription", { query }));
  return parts.join(t("entities.filters.separator"));
}

/** How an established entity earned its place, for the "Established by"
 * column. `established_by` is the daemon's free-form reason string. */
export function establishedByLabel(entity: Entity, t: TFunction): string {
  if (entity.established_by === "manual") return t("entities.establishedBy.manual");
  if (entity.established_by === "auto:citation") return t("entities.establishedBy.citation");
  return t("entities.establishedBy.memories", { count: entity.memory_count });
}

/** "{{n}} selected" for the selection bar. */
export function selectionSummary(count: number, t: TFunction): string {
  return t("entities.selection", { count });
}

export function toggleSelected(selected: ReadonlySet<string>, id: string): Set<string> {
  const next = new Set(selected);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  return next;
}

/** Selects or clears every id currently on screen; used by the header
 * checkbox. Ids not on screen (a later page) are left untouched. */
export function toggleSelectAll(
  selected: ReadonlySet<string>,
  ids: readonly string[],
  checked: boolean,
): Set<string> {
  if (!checked) {
    const next = new Set(selected);
    for (const id of ids) next.delete(id);
    return next;
  }
  return new Set([...selected, ...ids]);
}

export interface SelectAllState {
  readonly checked: boolean;
  readonly indeterminate: boolean;
}

export function selectAllState(selected: ReadonlySet<string>, ids: readonly string[]): SelectAllState {
  if (ids.length === 0) return { checked: false, indeterminate: false };
  const selectedCount = ids.filter((id) => selected.has(id)).length;
  return {
    checked: selectedCount === ids.length,
    indeterminate: selectedCount > 0 && selectedCount < ids.length,
  };
}
