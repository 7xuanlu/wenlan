// SPDX-License-Identifier: AGPL-3.0-only
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { i18n } from "../../i18n";
import type { GraphEdge, GraphNode } from "../../lib/graph/model";
import AtlasInspector from "./AtlasInspector";

const node = (id: string, kind: GraphNode["kind"], name: string, entityType = "person"): GraphNode => ({
  id, kind, name, entityType, confirmed: true, degree: 1, space: "test", createdAt: 1, updatedAt: 1,
});

const selected = node("entity-selected", "entity", "Selected", "person");
const neighbors = [
  node("page-1", "page", "Research page"),
  node("entity-1", "entity", "Ada Lovelace", "person"),
  node("memory-1", "memory", "Launch note"),
];
const edges: GraphEdge[] = [
  { id: "edge-page", source: selected.id, target: "page-1", type: "documents", confidence: null, createdAt: 1 },
  { id: "edge-entity", source: "entity-1", target: selected.id, type: "inspired", confidence: null, createdAt: 1 },
  { id: "edge-memory", source: selected.id, target: "memory-1", type: "mentions", confidence: null, createdAt: 1 },
];

function renderInspector(overrides: Partial<React.ComponentProps<typeof AtlasInspector>> = {}) {
  const onSelect = vi.fn();
  const onClose = vi.fn();
  return {
    user: userEvent.setup(),
    onSelect,
    onClose,
    ...render(<AtlasInspector node={selected} neighbors={neighbors} edges={edges} onSelect={onSelect} onClose={onClose} {...overrides} />),
  };
}

beforeEach(async () => { await i18n.changeLanguage("en"); });

describe("AtlasInspector", () => {
  it("groups connections in page, entity, memory order and shows directional relation context", () => {
    renderInspector();
    const summary = screen.getByRole("navigation", { name: "Connections" });
    expect(within(summary).getByRole("button", { name: "Wiki pages 1" })).toBeInTheDocument();
    expect(within(summary).getByRole("button", { name: "Entities 1" })).toBeInTheDocument();
    expect(within(summary).getByRole("button", { name: "Memories 1" })).toBeInTheDocument();
    const sections = screen.getAllByRole("region");
    expect(sections.map((section) => within(section).getByRole("heading").textContent?.replace(/\s+\d+$/, ""))).toEqual([
      "Wiki pages", "Entities", "Memories",
    ]);
    expect(within(sections[0]).getByText("documents")).toBeInTheDocument();
    expect(within(sections[1]).getByText("inspired")).toBeInTheDocument();
    expect(screen.getByText("Person")).toBeInTheDocument();
    expect(within(sections[2]).getByText("mentions")).toBeInTheDocument();
  });

  it("filters by relation type and keeps neighbor button names stable", async () => {
    const { user } = renderInspector({
      neighbors: Array.from({ length: 13 }, (_, index) => node(`entity-${index}`, "entity", `Person ${index}`, "person")),
      edges: [{ id: "edge-11", source: selected.id, target: "entity-11", type: "references", confidence: null, createdAt: 1 }],
    });
    const input = screen.getByRole("textbox", { name: "Filter connections" });
    await user.type(input, "references");
    expect(screen.getByRole("button", { name: "Person 11" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Person 0" })).not.toBeInTheDocument();
  });

  it("reveals twelve more rows per group", async () => {
    const many = Array.from({ length: 25 }, (_, index) => node(`entity-${index}`, "entity", `Person ${index}`, "person"));
    const { user } = renderInspector({ neighbors: many });
    const entitySection = screen.getByRole("region", { name: /Entities/ });
    expect(within(entitySection).getAllByRole("button")).toHaveLength(13);
    await user.click(within(entitySection).getByRole("button", { name: "Show more" }));
    expect(within(entitySection).getAllByRole("button")).toHaveLength(25);
  });
});
