// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";
import { queryEntities, type Entity } from "../../../lib/tauri";
import { DEFAULT_FILTERS } from "./entitiesViewModel";
import { loadActiveTopicsPage, type TopicCursor } from "./topicBrowse";

vi.mock("../../../lib/tauri", () => ({ queryEntities: vi.fn() }));
const topic = (index: number, status: Entity["status"], updated = 1000 - index): Entity => ({
  id: `${status}-${String(index).padStart(4, "0")}`, name: `Topic ${index}`,
  entity_type: "concept", domain: null, source_agent: null, confidence: null,
  confirmed: status === "established", status, established_by: null,
  created_at: 1, updated_at: updated, memory_count: 0,
});
const sorted = (items: Entity[]) => [...items].sort((a, b) => b.updated_at - a.updated_at || (a.id < b.id ? -1 : 1));
beforeEach(() => vi.resetAllMocks());

describe("active topic browsing", () => {
  it.each([[180, 160], [305, 3], [0, 205], [0, 0]])(
    "pages %i detected and %i established topics in one stable order without losing rows",
    async (detected, established) => {
      const fixture = sorted([
        ...Array.from({ length: detected }, (_, i) => topic(i, "detected")),
        ...Array.from({ length: established }, (_, i) => topic(i, "established")),
        topic(0, "archived", 9999),
      ]);
      vi.mocked(queryEntities).mockImplementation(async req => {
        const matches = fixture.filter(entity => entity.status === req.status);
        return { entities: matches.slice(req.offset, (req.offset ?? 0) + req.limit!), total: matches.length };
      });
      const collected: Entity[] = [];
      let cursor: TopicCursor | undefined;
      for (let batch = 0; batch < 10; batch++) {
        const result = await loadActiveTopicsPage(DEFAULT_FILTERS, cursor);
        expect(result.entities.length).toBeLessThanOrEqual(100);
        expect(result.total).toBe(detected + established);
        collected.push(...result.entities);
        cursor = result.cursor;
        if (!result.hasMore) break;
        if (batch === 9) throw new Error("Paging did not finish");
      }
      expect(collected.map(e => e.id)).toEqual(fixture.filter(e => e.status !== "archived").map(e => e.id));
      expect(new Set(collected.map(e => e.id)).size).toBe(detected + established);
      expect(queryEntities).not.toHaveBeenCalledWith(expect.objectContaining({ status: "archived" }));
    },
  );

  it("applies the same search and type filters to both lifecycles", async () => {
    vi.mocked(queryEntities).mockResolvedValue({ entities: [], total: 0 });
    await loadActiveTopicsPage({ ...DEFAULT_FILTERS, query: "  Ada  ", type: "person" });
    for (const status of ["detected", "established"]) {
      expect(queryEntities).toHaveBeenCalledWith({ status, query: "Ada", entity_type: "person", offset: 0, limit: 100 });
    }
  });

  it("fails as a whole if either lifecycle cannot load and retains the previous cursor for retry", async () => {
    const fixture = Array.from({ length: 220 }, (_, i) => topic(i, "detected"));
    vi.mocked(queryEntities).mockImplementation(async req => ({
      entities: req.status === "detected" ? fixture.slice(req.offset, (req.offset ?? 0) + req.limit!) : [],
      total: req.status === "detected" ? fixture.length : 0,
    }));
    const first = await loadActiveTopicsPage(DEFAULT_FILTERS);
    const snapshot = structuredClone(first.cursor);
    vi.mocked(queryEntities).mockRejectedValueOnce(new Error("Unavailable"));
    await expect(loadActiveTopicsPage(DEFAULT_FILTERS, first.cursor)).rejects.toThrow("Unavailable");
    expect(first.cursor).toEqual(snapshot);
    const retry = await loadActiveTopicsPage(DEFAULT_FILTERS, first.cursor);
    expect(retry.entities.map(e => e.id)).toEqual(fixture.slice(100, 200).map(e => e.id));
  });

  it("stops an empty response with a stale total from creating an endless load-more loop", async () => {
    vi.mocked(queryEntities).mockResolvedValue({ entities: [], total: 100 });
    const result = await loadActiveTopicsPage(DEFAULT_FILTERS);
    expect(result.entities).toEqual([]);
    expect(result.hasMore).toBe(false);
  });
});

describe("topic lifecycle crossover deduplication", () => {
  it.each([
    [20, 10, "detected"],
    [10, 20, "established"],
    [20, 20, "established"],
  ] as const)("keeps the freshest object for timestamps %i/%i with %s winning", async (detectedAt, establishedAt, winner) => {
    const detected = { ...topic(0, "detected", detectedAt), id: "crossing", name: "Detected snapshot" };
    const established = { ...topic(0, "established", establishedAt), id: "crossing", name: "Established snapshot" };
    vi.mocked(queryEntities).mockImplementation(async req => ({
      entities: [req.status === "detected" ? detected : established], total: 1,
    }));
    const result = await loadActiveTopicsPage(DEFAULT_FILTERS);
    expect(result.entities).toHaveLength(1);
    expect(result.entities[0]).toBe(winner === "detected" ? detected : established);
    expect(result.cursor.detected.buffer).toEqual([]);
    expect(result.cursor.established.buffer).toEqual([]);
    expect(result.hasMore).toBe(false);
    expect(queryEntities).toHaveBeenCalledTimes(2);
  });

  it("consumes buffered crossover copies without using a unique slot or changing raw query offsets", async () => {
    const crossing = { ...topic(0, "detected", 2000), id: "crossing" };
    const detected = [crossing, ...Array.from({ length: 99 }, (_, i) => topic(i + 1, "detected"))];
    const established = [
      ...Array.from({ length: 99 }, (_, i) => topic(i, "established", -100 - i)),
      { ...crossing, updated_at: -999, status: "established" as const, confirmed: true },
      ...Array.from({ length: 101 }, (_, i) => topic(i + 99, "established", -1000 - i)),
    ];
    vi.mocked(queryEntities).mockImplementation(async req => {
      const stream = req.status === "detected" ? detected : established;
      return { entities: stream.slice(req.offset, (req.offset ?? 0) + req.limit!), total: stream.length };
    });
    const first = await loadActiveTopicsPage(DEFAULT_FILTERS);
    expect(first.entities).toHaveLength(100);
    expect(first.entities[0]).toBe(crossing);
    expect(first.cursor.established.buffer).toHaveLength(99);
    expect(first.cursor.established.buffer.some(row => row.id === crossing.id)).toBe(false);
    const second = await loadActiveTopicsPage(DEFAULT_FILTERS, first.cursor);
    const third = await loadActiveTopicsPage(DEFAULT_FILTERS, second.cursor);
    expect(second.entities).toHaveLength(100);
    expect(third.entities).toHaveLength(100);
    expect(third.hasMore).toBe(false);
    const collected = [...first.entities, ...second.entities, ...third.entities];
    expect(new Set(collected.map(row => row.id)).size).toBe(300);
    expect(collected.map(row => row.id)).toEqual(sorted([
      ...detected, ...established.filter(row => row.id !== crossing.id),
    ]).map(row => row.id));
    expect(vi.mocked(queryEntities).mock.calls.map(([req]) => [req.status, req.offset, req.limit]))
      .toEqual([["detected", 0, 100], ["established", 0, 100], ["established", 100, 100], ["established", 200, 100]]);
  });

  it("deduplicates multiple versions returned within a single lifecycle response", async () => {
    const old = { ...topic(0, "detected", 10), id: "repeated" };
    const fresh = { ...old, name: "Fresh", updated_at: 20 };
    vi.mocked(queryEntities).mockImplementation(async req => ({
      entities: req.status === "detected" ? [old, fresh, old] : [],
      total: req.status === "detected" ? 3 : 0,
    }));
    const result = await loadActiveTopicsPage(DEFAULT_FILTERS);
    expect(result.entities).toEqual([fresh]);
    expect(result.entities[0]).toBe(fresh);
    expect(result.cursor.detected.offset).toBe(3);
    expect(result.hasMore).toBe(false);
  });
});
