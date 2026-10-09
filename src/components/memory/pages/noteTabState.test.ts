// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from "vitest";
import { matchesNote, noteReadingKey, promoteNoteTab, reorderNoteTab, upsertNoteTab } from "./noteTabState";
describe("note tab identities", () => {
 it("deduplicates repeated canonical opens and preserves label", () => {
  const first=upsertNoteTab([], {kind:"page",pageId:"a"}).map(tab=>({...tab,title:"A"}));
  const second=upsertNoteTab(first,{kind:"page",pageId:"b"});
  expect(upsertNoteTab(second,{kind:"page",pageId:"a",mode:"read"})).toHaveLength(2);
  expect(upsertNoteTab(second,{kind:"page",pageId:"a"})[0].title).toBe("A");
 });
 it("keeps separate unsaved sessions and promotes saved draft identity in place", () => {
  const a={kind:"page-draft",space:null,sessionKey:1} as const;
  const b={...a,sessionKey:2};
  const tabs=upsertNoteTab(upsertNoteTab([],a),b);
  const saved=upsertNoteTab(tabs,{...a,draftId:"saved"});
  expect(saved).toHaveLength(2);expect(saved[0].key).toBe(tabs[0].key);
  expect(matchesNote(saved[0],{...a,draftId:"saved",sessionKey:9})).toBe(true);
 });
 it("publishes a not-yet-identified draft without a ghost or duplicate tab", () => {
  const draft={kind:"page-draft",space:null,sessionKey:4} as const;
  const tabs=upsertNoteTab(upsertNoteTab([],draft),{kind:"page",pageId:"published"});
  const next=promoteNoteTab(tabs,draft,{kind:"page",pageId:"published"});
  expect(next).toHaveLength(1);expect(next[0].key).toBe(tabs[0].key);
  expect(next[0].view).toEqual({kind:"page",pageId:"published"});
 });
});

describe("stable tab drop ordering", () => {
 const notes = ["a", "b", "c"].map(id => ({ key: id, title: id, view: { kind: "page" as const, pageId: id } }));
 it("moves before the destination without replacing any editor identity", () => {
  const next = reorderNoteTab(notes, "c", "b");
  expect(next.map(tab=>tab.key)).toEqual(["a","c","b"]);
  expect(next[1]).toBe(notes[2]);
  expect(notes.map(tab=>tab.key)).toEqual(["a","b","c"]);
 });
 it("appends and treats self or disappeared target as a no-op", () => {
  expect(reorderNoteTab(notes,"a",null).map(tab=>tab.key)).toEqual(["b","c","a"]);
  expect(reorderNoteTab(notes,"a","a")).toEqual(notes);
  expect(reorderNoteTab(notes,"a","removed")).toEqual(notes);
  expect(reorderNoteTab(notes,"removed","b")).toEqual(notes);
 });
 it("keeps a draft's reading key stable across its first save", () => {
  expect(noteReadingKey({kind:"page-draft",space:null,sessionKey:4})).toBe(noteReadingKey({kind:"page-draft",space:null,sessionKey:4,draftId:"saved"}));
  expect(noteReadingKey({kind:"page",pageId:"saved"})).toBe("page:saved");
 });
});
