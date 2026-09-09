// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from "vitest";
import type { GraphModel, GraphNode } from "./model";
import { entityTypeHidden, filterGraphEntityTypes } from "./typeFilter";

describe("graph entity type filtering", () => {
  const node = (id: string, kind: GraphNode["kind"], entityType: string): GraphNode => ({ id, kind, entityType, name: id, confirmed: true, degree: 3, space: null, createdAt: 0, updatedAt: 0 });
  const model: GraphModel = {
    nodes: [node("person", "entity", "person"), node("tool", "entity", "technology"), node("page", "page", "page"), node("memory", "memory", "memory")],
    edges: [
      { id: "one", source: "person", target: "tool", type: "uses", confidence: null, createdAt: 0 },
      { id: "two", source: "page", target: "tool", type: "mentions", confidence: null, createdAt: 0 },
    ], coverage: { relationsFetchedFor: 2, totalEntities: 2 },
  };
  it("removes only excluded entities and their incident edges", () => {
    const result = filterGraphEntityTypes(model, new Set(["person"]));
    expect(result.nodes.map((n) => n.id)).toEqual(["tool", "page", "memory"]);
    expect(result.edges.map((e) => e.id)).toEqual(["two"]);
    expect(result.nodes[0]).toBe(model.nodes[1]);
    expect(model.nodes).toHaveLength(4);
  });
  it("leaves page and memory layers independent and allows zero entities", () => {
    expect(filterGraphEntityTypes(model, new Set(["person", "technology"])).nodes.map((n) => n.kind)).toEqual(["page", "memory"]);
    expect(entityTypeHidden("memory", new Set(["memory"]))).toBe(false);
    expect(entityTypeHidden("page", new Set(["page"]))).toBe(false);
    expect(entityTypeHidden("technology", new Set(["technology"]))).toBe(true);
  });
});
