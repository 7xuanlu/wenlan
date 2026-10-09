// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from "vitest";
import type { PageMapNode } from "../tauri";
import {
  appendOutline,
  buildActiveOutline,
} from "./outline";

function node(
  id: string,
  parent_id: string | null,
  label: string,
  rank = 0,
  status: PageMapNode["status"] = "active",
): PageMapNode {
  return {
    id,
    parent_id,
    rank,
    ref_kind: id === "root" ? "page" : "idea",
    ref_id: id,
    label,
    status,
    pinned: false,
    placed: false,
    collapsed: false,
    x: null,
    y: null,
    width: null,
    height: null,
    ref_state: "live",
  };
}

describe("page map outline", () => {
  it("builds a deterministic escaped outline from active hierarchy only", () => {
    const nodes = [
      node("root", null, "Page title"),
      node("b", "root", "Second [idea]", 2),
      node("a", "root", "First *idea*", 1),
      node("child", "a", "Nested\nidea"),
      node("suggested", "root", "Suggested", 3, "suggested"),
      node("dismissed", "root", "Dismissed", 4, "dismissed"),
      node("suggested-child", "suggested", "Hidden child"),
    ];

    expect(buildActiveOutline(nodes, "Mind map outline")).toBe(
      "## Mind map outline\n\n- First \\*idea\\*\n  - Nested idea\n- Second \\[idea\\]\n",
    );
  });

  it("omits the note root and returns null for a map without active ideas", () => {
    expect(buildActiveOutline([node("root", null, "Note")], "Outline")).toBeNull();
  });

  it("does not loop on cycles and rejects pathological depth", () => {
    const cycle = [
      node("root", null, "Note"),
      node("a", "root", "A"),
      node("b", "a", "B"),
    ];
    cycle[1]!.parent_id = "b";
    expect(buildActiveOutline(cycle, "Outline")).toBeNull();

    let parent = "root";
    const deep = [node("root", null, "Note")];
    for (let index = 0; index < 40; index += 1) {
      const id = `n${index}`;
      deep.push(node(id, parent, id));
      parent = id;
    }
    expect(() => buildActiveOutline(deep, "Outline")).toThrow(/depth/i);
  });

  it("appends without altering existing bytes and recognizes an exact block", () => {
    const body = "# Existing\r\n\r\nOriginal text  \r\n";
    const outline = "## Mind map outline\n\n- Idea\n";
    const appended = appendOutline(body, outline);

    expect(appended.alreadyPresent).toBe(false);
    expect(appended.appendedText).toBe("\n" + outline);
    expect(appended.content).toBe(body + appended.appendedText);
    expect(appendOutline(appended.content, outline)).toEqual({
      content: appended.content,
      appendedText: "\n" + outline,
      alreadyPresent: true,
    });
  });
});
