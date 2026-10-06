// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from "vitest";
import type { Page } from "../../../lib/tauri";
import { collectPageInventory, filterPageInventory, inventoryPageFilename, pageInventoryScope, pageMatchesInventoryScope } from "./pageInventory";

function page(overrides: Partial<Page> = {}): Page {
  return {
    id: "a", title: "My actual title", status: "active", summary: null, content: "",
    entity_id: null, domain: null, source_memory_ids: [], version: 1,
    created_at: "2026-07-16T00:00:00Z", last_compiled: "2026-07-16T00:00:00Z",
    last_modified: "2026-07-16T00:00:00Z", ...overrides,
  };
}

describe("page inventory projection", () => {
  it("uses the actual root filename and excludes logical Space folders", () => {
    const note = page({ storage_path: "2026-Project Notes.md", space: "Work", domain: "Projects" });
    expect(inventoryPageFilename(note)).toBe("2026-Project Notes.md");
    expect(pageInventoryScope(note)).toBe("files");
    expect(inventoryPageFilename(page({ space: "Work" }))).toBeNull();
    expect(pageInventoryScope(page({ space: "Work" }))).toBe("unfiled");
  });

  it.each([undefined, null, "", " ", " plan.md", "plan.md ", ".secret.md", "../plan.md", "Work/plan.md", "Work\\plan.md", "/notes/plan.md", "C:plan.md", "file:plan.md", "plan.txt", "plan.md\n", "plan\u0000.md"])(
    "keeps unsafe or missing path %s in Other notes", (storage_path) => {
      const note = page({ storage_path });
      expect(inventoryPageFilename(note)).toBeNull();
      expect(pageInventoryScope(note)).toBe("unfiled");
    },
  );

  it("puts drafts first even if they carry a file path, and excludes both kinds of entities", () => {
    expect(pageInventoryScope(page({ status: "draft", storage_path: "draft.md" }))).toBe("drafts");
    for (const note of [page({ entity_id: "entity" }), page({ creation_kind: "entity" })]) {
      expect(pageInventoryScope(note)).toBeNull();
      expect(pageMatchesInventoryScope(note, "all")).toBe(false);
    }
  });

  it("deduplicates independent responses with active precedence and keeps modification ordering", () => {
    const active = page({ id: "same", title: "Published", storage_path: "published.md" });
    const draft = page({ id: "same", title: "Editing", status: "draft" });
    const recent = page({ id: "recent", last_modified: "2026-07-17T00:00:00Z" });
    const inventory = collectPageInventory([active, recent, page({ id: "entity", creation_kind: "entity" })], [draft]);
    expect(inventory.map((note) => note.id)).toEqual(["recent", "same"]);
    expect(inventory[1]).toBe(active);
    expect(pageMatchesInventoryScope(draft, "drafts")).toBe(true);
    expect(pageMatchesInventoryScope(draft, "files")).toBe(false);
  });

  it("matches titles and actual paths without slugifying titles", () => {
    const note = page({ title: "中文筆記", storage_path: "historic-name.md" });
    expect(filterPageInventory([note], " HISTORIC ", "en")).toEqual([note]);
    expect(filterPageInventory([note], "中文", "zh-TW")).toEqual([note]);
    expect(filterPageInventory([note], "中文筆記.md", "zh-TW")).toEqual([]);
    expect(filterPageInventory([page({ storage_path: "../unsafe.md" })], "unsafe", "en")).toEqual([]);
  });
});
