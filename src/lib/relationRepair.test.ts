// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, expect, it, vi } from "vitest";
import { getEntityDetail, getMemoryDetail, type Entity, type EntityDetail, type MemoryItem } from "./tauri";
import { loadRelationRepairSources } from "./relationRepair";

vi.mock("./tauri", () => ({ getEntityDetail: vi.fn(), getMemoryDetail: vi.fn() }));
const entity = (id: string): Entity => ({ id, name: id, entity_type: "concept", domain: null, source_agent: null, confidence: null, confirmed: false, created_at: 1, updated_at: 1, memory_count: 0, status: "detected", established_by: null });
const details: EntityDetail[] = ["a", "b"].map((id) => ({ entity: entity(id), observations: [], relations: [{ id: "edge", entity_id: id === "a" ? "b" : "a", entity_name: "Other", entity_type: "concept", relation_type: "related_to", direction: id === "a" ? "outgoing" : "incoming", source_agent: null, created_at: 1 }] }));
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(getEntityDetail).mockImplementation(async (id) => {
    const detail = details.find((entry) => entry.entity.id === id);
    if (!detail) throw new Error("entity missing");
    return structuredClone(detail);
  });
  vi.mocked(getMemoryDetail).mockResolvedValue(null);
});

it("resolves only queued owners and deduplicates the two directions of one edge", async () => {
  const sources = await loadRelationRepairSources(["a", "b"]);
  expect(sources.entities.map((entry) => entry.id)).toEqual(["a", "b"]);
  expect(sources.relations).toEqual([{ id: "edge", from_entity: "a", to_entity: "b", relation_type: "related_to", source_agent: null, created_at: 1 }]);
  expect(vi.mocked(getEntityDetail).mock.calls).toEqual([["a"], ["b"]]);
  expect(vi.mocked(getMemoryDetail).mock.calls).toEqual([["a"], ["b"]]);
});

it("refuses missing owners and IDs that resolve to both a memory and an entity", async () => {
  await expect(loadRelationRepairSources(["a", "missing"])).rejects.toThrow("unavailable or ambiguous");
  vi.mocked(getMemoryDetail).mockResolvedValue({ source_id: "a" } as MemoryItem);
  await expect(loadRelationRepairSources(["a", "b"])).rejects.toThrow("unavailable or ambiguous");
});

it("refuses inconsistent edge directions instead of displaying a guessed preview", async () => {
  vi.mocked(getEntityDetail).mockImplementation(async (id) => ({ ...details.find((entry) => entry.entity.id === id)!, relations: [{ ...details[id === "a" ? 0 : 1].relations[0], direction: "outgoing" }] }));
  await expect(loadRelationRepairSources(["a", "b"])).rejects.toThrow("changed while loading");
});
