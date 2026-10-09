// SPDX-License-Identifier: AGPL-3.0-only
import type { View } from "../navigation/viewState";
export type NoteView = Extract<View, { kind: "page" | "page-draft" }>;
export type NoteTab = { readonly key: string; readonly view: NoteView; readonly title: string };
export function isNoteView(view: View): view is NoteView { return view.kind === "page" || view.kind === "page-draft"; }
export function noteId(view: NoteView): string | undefined { return view.kind === "page" ? view.pageId : view.draftId; }
export function matchesNote(tab: NoteTab, view: NoteView): boolean {
  const id = noteId(view);
  if (id && noteId(tab.view) === id) return true;
  return tab.view.kind === "page-draft" && view.kind === "page-draft"
    && tab.view.sessionKey === view.sessionKey && (view.sessionKey != null || (!id && !noteId(tab.view)));
}
export function upsertNoteTab(tabs: readonly NoteTab[], view: NoteView): NoteTab[] {
  const existing = tabs.find(tab => matchesNote(tab, view));
  if (existing) return tabs.map(tab => tab === existing ? { ...tab, view } : tab);
  return [...tabs, { key: noteId(view) ? `note:${noteId(view)}` : `draft:${view.kind === "page-draft" ? view.sessionKey ?? "initial" : "initial"}`, view, title: "" }];
}
// Publish promotes the current draft in place, including before its first
// saved identity has reached Main. An existing canonical tab is deduplicated.
export function promoteNoteTab(tabs: readonly NoteTab[], source: NoteView, next: NoteView): NoteTab[] {
  const previous = tabs.find(tab => matchesNote(tab, source));
  if (!previous) return upsertNoteTab(tabs, next);
  return tabs.filter(tab => tab === previous || !matchesNote(tab, next))
    .map(tab => tab === previous ? { ...tab, view: next } : tab);
}

/** A drop inserts before a stable tab identity, never a stale numeric index. */
export function reorderNoteTab(tabs: readonly NoteTab[], key: string, beforeKey: string | null): NoteTab[] {
  const moving = tabs.find(tab => tab.key === key);
  if (!moving || beforeKey === key || (beforeKey !== null && !tabs.some(tab => tab.key === beforeKey))) return [...tabs];
  const remaining = tabs.filter(tab => tab.key !== key);
  const index = beforeKey === null ? remaining.length : remaining.findIndex(tab => tab.key === beforeKey);
  return [...remaining.slice(0, index), moving, ...remaining.slice(index)];
}

export function noteReadingKey(view: NoteView): string {
  return view.kind === "page" ? `page:${view.pageId}` : `page-draft:${view.sessionKey ?? view.draftId ?? "new"}`;
}

const readingSessions = new WeakMap<object, Map<string, number>>();
/** Share scroll state within one workspace, never across windows or users. */
export function noteReadingPositionsFor(owner: object): Map<string, number> {
  let positions = readingSessions.get(owner);
  if (!positions) { positions = new Map(); readingSessions.set(owner, positions); }
  return positions;
}
