// SPDX-License-Identifier: AGPL-3.0-only
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Entity, ListEntitiesRequest } from "../../../lib/tauri";
import { EntitiesView } from "./EntitiesView";

vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

vi.mock("../../../lib/tauri", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../lib/tauri")>()),
  queryEntities: vi.fn(),
  archiveEntities: vi.fn(),
  restoreEntities: vi.fn(),
  confirmEntity: vi.fn(),
  deleteEntity: vi.fn(),
}));

function entity(overrides: Partial<Entity> & { id: string; name: string }): Entity {
  return {
    entity_type: "concept",
    domain: null,
    source_agent: null,
    confidence: null,
    confirmed: false,
    created_at: 1_700_000_000,
    updated_at: 1_700_000_000,
    memory_count: 0,
    status: "detected",
    established_by: null,
    ...overrides,
  };
}

// A small in-memory stand-in for the daemon's /entities/query and
// /entities/archive|restore routes (crates/wenlan-server/src/entity_graph_routes.rs),
// faithful enough to exercise the view's real request-building and its
// archive-all-matching / restore round trip end to end.
function matchesFilter(candidate: Entity, filter: ListEntitiesRequest): boolean {
  if (filter.status && candidate.status !== filter.status) return false;
  if (filter.entity_type && candidate.entity_type !== filter.entity_type) return false;
  if (typeof filter.min_memories === "number" && candidate.memory_count < filter.min_memories) return false;
  if (typeof filter.max_memories === "number" && candidate.memory_count > filter.max_memories) return false;
  if (filter.query && !candidate.name.toLowerCase().includes(filter.query.toLowerCase())) return false;
  return true;
}

let fixture: Entity[];

function seedFixture(): Entity[] {
  return [
    entity({ id: "ada", name: "Ada Lovelace", entity_type: "person", status: "detected", memory_count: 0 }),
    entity({ id: "babbage", name: "Charles Babbage", entity_type: "person", status: "detected", memory_count: 2 }),
    entity({
      id: "engine",
      name: "Analytical Engine",
      entity_type: "concept",
      status: "established",
      memory_count: 5,
      established_by: "auto:memories",
      confirmed: true,
    }),
    entity({
      id: "countess",
      name: "Countess of Lovelace",
      entity_type: "person",
      status: "archived",
      memory_count: 1,
      confirmed: true,
      established_by: "manual",
    }),
  ];
}

beforeEach(async () => {
  fixture = seedFixture();
  // Pin the pre-existing row tests to the rows lens; the cards tests below
  // manage the key themselves (clear it for the cards default).
  window.localStorage.setItem("wenlan-entities-view-mode", "rows");
  const tauri = await import("../../../lib/tauri");

  vi.mocked(tauri.queryEntities).mockImplementation(async (filter) => {
    const all = fixture.filter((candidate) => matchesFilter(candidate, filter));
    const offset = filter.offset ?? 0;
    const limit = filter.limit ?? 100;
    return { entities: all.slice(offset, offset + limit), total: all.length };
  });

  vi.mocked(tauri.archiveEntities).mockImplementation(async (req) => {
    const eligible = fixture.filter((candidate) => candidate.status !== "archived");
    const selected = req.ids
      ? eligible.filter((candidate) => req.ids?.includes(candidate.id))
      : eligible.filter((candidate) => matchesFilter(candidate, req.filter ?? {}));
    if (!req.dry_run) {
      const ids = new Set(selected.map((candidate) => candidate.id));
      fixture = fixture.map((candidate) =>
        ids.has(candidate.id) ? { ...candidate, status: "archived" } : candidate,
      );
    }
    return { count: selected.length, entity_ids: selected.map((candidate) => candidate.id), dry_run: req.dry_run };
  });

  vi.mocked(tauri.restoreEntities).mockImplementation(async (req) => {
    const eligible = fixture.filter((candidate) => candidate.status === "archived");
    const selected = req.ids
      ? eligible.filter((candidate) => req.ids?.includes(candidate.id))
      : eligible.filter((candidate) => matchesFilter(candidate, req.filter ?? {}));
    if (!req.dry_run) {
      const ids = new Set(selected.map((candidate) => candidate.id));
      fixture = fixture.map((candidate) =>
        ids.has(candidate.id)
          ? { ...candidate, status: candidate.confirmed ? "established" : "detected" }
          : candidate,
      );
    }
    return { count: selected.length, entity_ids: selected.map((candidate) => candidate.id), dry_run: req.dry_run };
  });

  vi.mocked(tauri.confirmEntity).mockImplementation(async (id, confirmed) => {
    fixture = fixture.map((candidate) =>
      candidate.id === id
        ? { ...candidate, confirmed, status: confirmed ? "established" : candidate.status, established_by: confirmed ? "manual" : candidate.established_by }
        : candidate,
    );
  });

  vi.mocked(tauri.deleteEntity).mockImplementation(async (id) => {
    fixture = fixture.filter((candidate) => candidate.id !== id);
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

function renderView(onEntityClick = vi.fn()) {
  return { onEntityClick, user: userEvent.setup(), ...render(<EntitiesView onEntityClick={onEntityClick} />) };
}

async function openTab(user: ReturnType<typeof userEvent.setup>, name: RegExp) {
  await user.click(screen.getByRole("tab", { name }));
}

describe("EntitiesView", () => {
  it("opens on the Detected tab and lists its rows with tab counts", async () => {
    renderView();

    expect(await screen.findByText("Ada Lovelace")).toBeInTheDocument();
    expect(screen.getByText("Charles Babbage")).toBeInTheDocument();
    expect(screen.queryByText("Analytical Engine")).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /Detected/ })).toHaveAttribute("aria-selected", "true");
    // 1 confirmed (Engine), 2 detected (Ada, Babbage), 1 archived (Countess).
    expect(screen.getByRole("tab", { name: /Confirmed/ })).toHaveTextContent("1");
    expect(screen.getByRole("tab", { name: /Detected/ })).toHaveTextContent("2");
    expect(screen.getByRole("tab", { name: /Archived/ })).toHaveTextContent("1");
  });

  it("filters the Detected tab by the Type chip", async () => {
    const { user } = renderView();
    await screen.findByText("Ada Lovelace");

    await user.click(screen.getByRole("button", { name: "Concept" }));

    expect(await screen.findByText("No detected entities match")).toBeInTheDocument();
    expect(screen.queryByText("Ada Lovelace")).not.toBeInTheDocument();
  });

  it("confirms a selected entity from the selection bar", async () => {
    const { user } = renderView();
    await screen.findByText("Ada Lovelace");

    await user.click(screen.getByRole("checkbox", { name: "Select Ada Lovelace" }));
    await user.click(screen.getByRole("button", { name: "Confirm selected" }));

    await screen.findByText("Charles Babbage");
    expect(screen.queryByText("Ada Lovelace")).not.toBeInTheDocument();

    await openTab(user, /Confirmed/);
    expect(await screen.findByText("Ada Lovelace")).toBeInTheDocument();
  });

  it("opens the dossier from a Confirmed row", async () => {
    const { user, onEntityClick } = renderView();
    await screen.findByText("Ada Lovelace");
    await openTab(user, /Confirmed/);

    await user.click(await screen.findByRole("button", { name: "Analytical Engine" }));
    expect(onEntityClick).toHaveBeenCalledWith("engine");
    expect(screen.getByText("5 memories")).toBeInTheDocument();
  });

  it("archives all matching via dry-run-then-confirm, and restores them back by their own state", async () => {
    const { user } = renderView();
    await screen.findByText("Ada Lovelace");

    await user.click(screen.getByRole("button", { name: "Person" }));
    await screen.findByText("Ada Lovelace");
    await screen.findByText("Charles Babbage");

    await user.click(screen.getByRole("button", { name: "Archive all matching" }));

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Archive 2 detected entities?")).toBeInTheDocument();
    expect(within(dialog).getByText("Filter")).toBeInTheDocument();
    expect(within(dialog).getByText(/Person/)).toBeInTheDocument();
    // Babbage (2 memories) is among the Person matches, so the dialog warns
    // that archiving takes memories with it.
    expect(within(dialog).getByText("Includes")).toBeInTheDocument();
    expect(within(dialog).getByText("entities that already have memories")).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        "To keep those, set Memories to None first. Archived entities can be restored from the Archived tab.",
      ),
    ).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: "Archive" }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await screen.findByText("No detected entities match");

    await openTab(user, /Archived/);
    await screen.findByText("Ada Lovelace");
    await screen.findByText("Charles Babbage");
    await screen.findByText("Countess of Lovelace");

    const selectAll = screen.getByRole("checkbox", { name: "Select all" });
    await user.click(selectAll);
    await user.click(screen.getByRole("button", { name: "Restore selected" }));

    // Ada and Babbage were never confirmed, so they land back on Detected;
    // the Countess was confirmed before archiving, so she returns Confirmed.
    await openTab(user, /Detected/);
    await screen.findByText("Ada Lovelace");
    await screen.findByText("Charles Babbage");
    await openTab(user, /Confirmed/);
    await screen.findByText("Countess of Lovelace");
  });

  it("shows an Includes line when the matched entities still have memories", async () => {
    const { user } = renderView();
    await screen.findByText("Ada Lovelace");
    await screen.findByText("Charles Babbage");

    // Default Memories chip is "any"; Babbage (2 memories) matches alongside Ada (0).
    await user.click(screen.getByRole("button", { name: "Archive all matching" }));

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Includes")).toBeInTheDocument();
    expect(within(dialog).getByText("entities that already have memories")).toBeInTheDocument();
    expect(within(dialog).getByText("1")).toBeInTheDocument();
    expect(
      within(dialog).getByText(
        "To keep those, set Memories to None first. Archived entities can be restored from the Archived tab.",
      ),
    ).toBeInTheDocument();
  });

  it("omits the Includes line once the Memories chip already excludes entities with memories", async () => {
    const { user } = renderView();
    await screen.findByText("Ada Lovelace");

    await user.click(screen.getByRole("button", { name: "None" }));
    await screen.findByText("Ada Lovelace");
    expect(screen.queryByText("Charles Babbage")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Archive all matching" }));

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Archive 1 detected entity?")).toBeInTheDocument();
    expect(within(dialog).queryByText("Includes")).not.toBeInTheDocument();
    expect(
      within(dialog).getByText("Archived entities can be restored from the Archived tab."),
    ).toBeInTheDocument();
  });

  it("deletes one archived entity permanently from its row, with an irreversible confirm", async () => {
    const { user } = renderView();
    await screen.findByText("Ada Lovelace");
    await openTab(user, /Archived/);
    await screen.findByText("Countess of Lovelace");

    // Permanent delete is per row (spec): the selection bar only restores.
    await user.click(screen.getByRole("checkbox", { name: "Select Countess of Lovelace" }));
    expect(screen.getByRole("button", { name: "Restore selected" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Delete permanently" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Delete Countess of Lovelace permanently" }));

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Delete Countess of Lovelace permanently?")).toBeInTheDocument();
    expect(within(dialog).getByText("This cannot be undone.")).toBeInTheDocument();
    // The safe action takes focus, not the destructive one.
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();

    await user.click(within(dialog).getByRole("button", { name: "Delete permanently" }));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(await screen.findByText("No archived entities")).toBeInTheDocument();
    const tauri = await import("../../../lib/tauri");
    expect(tauri.deleteEntity).toHaveBeenCalledTimes(1);
    expect(tauri.deleteEntity).toHaveBeenCalledWith("countess");
  });

  it("keeps the confirm open on Escape while the delete is in flight", async () => {
    const tauri = await import("../../../lib/tauri");
    let release: () => void = () => {};
    vi.mocked(tauri.deleteEntity).mockImplementation(
      (id) =>
        new Promise<void>((resolve) => {
          release = () => {
            fixture = fixture.filter((candidate) => candidate.id !== id);
            resolve();
          };
        }),
    );
    const { user } = renderView();
    await screen.findByText("Ada Lovelace");
    await openTab(user, /Archived/);
    await screen.findByText("Countess of Lovelace");
    await user.click(screen.getByRole("button", { name: "Delete Countess of Lovelace permanently" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Delete permanently" }));

    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBeInTheDocument();

    release();
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(await screen.findByText("No archived entities")).toBeInTheDocument();
  });

  it("drops a list response that arrives after the tab changed", async () => {
    const tauri = await import("../../../lib/tauri");
    let releaseDetected: () => void = () => {};
    vi.mocked(tauri.queryEntities).mockImplementation(async (filter) => {
      const all = fixture.filter((candidate) => matchesFilter(candidate, filter));
      const offset = filter.offset ?? 0;
      const limit = filter.limit ?? 100;
      const page = { entities: all.slice(offset, offset + limit), total: all.length };
      // The Detected LIST (not the limit-1 count probe) hangs until released.
      if (filter.status === "detected" && limit !== 1) {
        await new Promise<void>((resolve) => {
          releaseDetected = resolve;
        });
      }
      return page;
    });

    const { user } = renderView();
    await screen.findByRole("tab", { name: /Archived/ });
    await openTab(user, /Archived/);
    await screen.findByText("Countess of Lovelace");

    releaseDetected();
    // Give the stale promise every chance to land, then assert it did not.
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(screen.getByText("Countess of Lovelace")).toBeInTheDocument();
    expect(screen.queryByText("Ada Lovelace")).not.toBeInTheDocument();
    expect(screen.queryByText("Charles Babbage")).not.toBeInTheDocument();
  });

  it("reads back and applies the search term as typed, even before the debounce lands", async () => {
    const tauri = await import("../../../lib/tauri");
    const { user } = renderView();
    await screen.findByText("Ada Lovelace");

    await user.type(screen.getByRole("searchbox", { name: "Find a name" }), "Ada");
    await user.click(screen.getByRole("button", { name: "Archive all matching" }));

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/"Ada"/)).toBeInTheDocument();
    expect(within(dialog).getByText("Archive 1 detected entity?")).toBeInTheDocument();
    const dryRuns = vi.mocked(tauri.archiveEntities).mock.calls.filter(([req]) => req.dry_run);
    expect(dryRuns.length).toBeGreaterThan(0);
    for (const [req] of dryRuns) expect(req.filter?.query).toBe("Ada");

    await user.click(within(dialog).getByRole("button", { name: "Archive" }));
    const applied = vi.mocked(tauri.archiveEntities).mock.calls.find(([req]) => !req.dry_run);
    expect(applied?.[0].filter?.query).toBe("Ada");
    await openTab(user, /Archived/);
    await screen.findByText("Ada Lovelace");
    // Only the "Ada" match went; Babbage is still detected (the search box
    // still says "Ada", so he is filtered out of the list, not archived).
    expect(fixture.find((candidate) => candidate.id === "babbage")?.status).toBe("detected");
  });

  it("filters the Confirmed tab by search", async () => {
    const tauri = await import("../../../lib/tauri");
    const { user } = renderView();
    await screen.findByText("Ada Lovelace");
    await openTab(user, /Confirmed/);
    await screen.findByText("Analytical Engine");

    await user.type(screen.getByRole("searchbox", { name: "Find a name" }), "Engine");

    // The debounced search narrows the Confirmed request to the match.
    await waitFor(() => {
      const calls = vi.mocked(tauri.queryEntities).mock.calls;
      const last = calls[calls.length - 1][0];
      expect(last.status).toBe("established");
      expect(last.query).toBe("Engine");
    });
    expect(screen.getByText("Analytical Engine")).toBeInTheDocument();

    const searchbox = screen.getByRole("searchbox", { name: "Find a name" });
    await user.clear(searchbox);
    await user.type(searchbox, "zzz");
    expect(await screen.findByText("No confirmed entities yet")).toBeInTheDocument();
  });

  it("archives selected entities from the Confirmed tab", async () => {
    const tauri = await import("../../../lib/tauri");
    const { user } = renderView();
    await screen.findByText("Ada Lovelace");

    // Give Confirmed a second row through the real confirm flow.
    await user.click(screen.getByRole("checkbox", { name: "Select Ada Lovelace" }));
    await user.click(screen.getByRole("button", { name: "Confirm selected" }));
    await openTab(user, /Confirmed/);
    await screen.findByText("Analytical Engine");
    await screen.findByText("Ada Lovelace");

    await user.click(screen.getByRole("checkbox", { name: "Select Analytical Engine" }));
    // Confirm makes no sense for already-confirmed rows: only archiving.
    expect(screen.queryByRole("button", { name: "Confirm selected" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("checkbox", { name: "Select all" }));
    await user.click(screen.getByRole("button", { name: "Archive selected" }));

    const applied = vi.mocked(tauri.archiveEntities).mock.calls.find(([req]) => !req.dry_run);
    expect([...(applied?.[0].ids ?? [])].sort()).toEqual(["ada", "engine"]);
    expect(await screen.findByText("No confirmed entities yet")).toBeInTheDocument();

    await openTab(user, /Archived/);
    await screen.findByText("Analytical Engine");
    await screen.findByText("Ada Lovelace");
  });

  it("archives all matching on Confirmed with the active filter read back", async () => {
    const tauri = await import("../../../lib/tauri");
    const { user } = renderView();
    await screen.findByText("Ada Lovelace");
    await openTab(user, /Confirmed/);
    await screen.findByText("Analytical Engine");

    await user.click(screen.getByRole("button", { name: "Concept" }));
    await screen.findByText("Analytical Engine");

    await user.click(screen.getByRole("button", { name: "Archive all matching" }));

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Archive 1 confirmed entity?")).toBeInTheDocument();
    expect(within(dialog).getByText(/Concept/)).toBeInTheDocument();
    // The Engine holds 5 memories, so the dialog warns archiving takes them along.
    expect(within(dialog).getByText("Includes")).toBeInTheDocument();

    const dryRuns = vi.mocked(tauri.archiveEntities).mock.calls.filter(([req]) => req.dry_run);
    expect(dryRuns.length).toBeGreaterThan(0);
    for (const [req] of dryRuns) {
      expect(req.filter?.status).toBe("established");
      expect(req.filter?.entity_type).toBe("concept");
    }

    await user.click(within(dialog).getByRole("button", { name: "Archive" }));
    const applied = vi.mocked(tauri.archiveEntities).mock.calls.find(([req]) => !req.dry_run);
    expect(applied?.[0].filter?.status).toBe("established");
    expect(applied?.[0].filter?.entity_type).toBe("concept");

    expect(await screen.findByText("No confirmed entities yet")).toBeInTheDocument();
    await openTab(user, /Archived/);
    await screen.findByText("Analytical Engine");
  });

  it("filters the Archived tab by search", async () => {
    const tauri = await import("../../../lib/tauri");
    const { user } = renderView();
    await screen.findByText("Ada Lovelace");
    await openTab(user, /Archived/);
    await screen.findByText("Countess of Lovelace");

    await user.type(screen.getByRole("searchbox", { name: "Find a name" }), "Countess");

    await waitFor(() => {
      const calls = vi.mocked(tauri.queryEntities).mock.calls;
      const last = calls[calls.length - 1][0];
      expect(last.status).toBe("archived");
      expect(last.query).toBe("Countess");
    });
    expect(screen.getByText("Countess of Lovelace")).toBeInTheDocument();

    const searchbox = screen.getByRole("searchbox", { name: "Find a name" });
    await user.clear(searchbox);
    await user.type(searchbox, "zzz");
    expect(await screen.findByText("No archived entities")).toBeInTheDocument();
  });

  it("restores only what the Archived matchline counted when a filter is on", async () => {
    fixture = [
      ...fixture,
      entity({
        id: "difference",
        name: "Difference Engine",
        entity_type: "concept",
        status: "archived",
        memory_count: 0,
      }),
    ];
    const tauri = await import("../../../lib/tauri");
    const { user } = renderView();
    await screen.findByText("Ada Lovelace");
    await openTab(user, /Archived/);
    await screen.findByText("Countess of Lovelace");
    await screen.findByText("Difference Engine");

    await user.click(screen.getByRole("button", { name: "Concept" }));
    expect(await screen.findByText("1 archived entity matches")).toBeInTheDocument();

    // The button says what it will do, and does exactly that: the person
    // filtered out of the list must survive the restore.
    await user.click(await screen.findByRole("button", { name: "Restore all matching" }));

    const applied = vi.mocked(tauri.restoreEntities).mock.calls.find(([req]) => !req.dry_run);
    expect(applied?.[0].filter?.status).toBe("archived");
    expect(applied?.[0].filter?.entity_type).toBe("concept");
    expect(applied?.[0].ids).toBeUndefined();

    await waitFor(() =>
      expect(fixture.find((candidate) => candidate.id === "difference")?.status).toBe("detected"),
    );
    expect(fixture.find((candidate) => candidate.id === "countess")?.status).toBe("archived");
  });

  it("will not restore on a count the search box is about to change", async () => {
    fixture = [
      ...fixture,
      entity({
        id: "difference",
        name: "Difference Engine",
        entity_type: "concept",
        status: "archived",
        memory_count: 0,
      }),
    ];
    const tauri = await import("../../../lib/tauri");
    const { user } = renderView();
    await screen.findByText("Ada Lovelace");
    await openTab(user, /Archived/);
    await screen.findByText("Countess of Lovelace");

    await user.type(screen.getByRole("searchbox", { name: "Find a name" }), "Countess");
    expect(await screen.findByText("1 archived entity matches")).toBeInTheDocument();

    // Clearing the box does not reach the count for 300 ms, and the new count
    // only lands when its request returns. Until both have happened the button
    // is unavailable, rather than restoring everything archived while the
    // screen still says one entity matches.
    let releaseList: () => void = () => {};
    const realQuery = vi.mocked(tauri.queryEntities).getMockImplementation()!;
    vi.mocked(tauri.queryEntities).mockImplementation(async (filter) => {
      if (filter.status === "archived" && filter.query === undefined && filter.limit !== 1) {
        await new Promise<void>((resolve) => {
          releaseList = resolve;
        });
      }
      return realQuery(filter);
    });

    await user.clear(screen.getByRole("searchbox", { name: "Find a name" }));
    expect(screen.getByRole("button", { name: "Restore all matching" })).toBeDisabled();

    // The debounce has now moved the cleared search into the filters, but the
    // list showing the new count is still in flight.
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Restore all" })).toBeDisabled(),
    );
    expect(vi.mocked(tauri.restoreEntities)).not.toHaveBeenCalled();

    releaseList();

    await screen.findByText("2 archived entities");
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Restore all" })).toBeEnabled(),
    );
    await user.click(screen.getByRole("button", { name: "Restore all" }));
    const applied = vi.mocked(tauri.restoreEntities).mock.calls.find(([req]) => !req.dry_run);
    expect(applied?.[0].filter?.query).toBeUndefined();
  });

  it("will not restore on a count a failed reload left stale", async () => {
    const tauri = await import("../../../lib/tauri");
    const { user } = renderView();
    await screen.findByText("Ada Lovelace");
    await openTab(user, /Archived/);
    await screen.findByText("Countess of Lovelace");

    // The list for the typed search never arrives, so the count beside the
    // button still describes the unfiltered tab. The button must not act on
    // it.
    const realQuery = vi.mocked(tauri.queryEntities).getMockImplementation()!;
    vi.mocked(tauri.queryEntities).mockImplementation(async (filter) => {
      if (filter.query !== undefined) throw new Error("list unavailable");
      return realQuery(filter);
    });

    await user.type(screen.getByRole("searchbox", { name: "Find a name" }), "Countess");

    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Restore all matching" })).toBeDisabled(),
    );
    expect(vi.mocked(tauri.restoreEntities)).not.toHaveBeenCalled();
  });

  it("keeps the filters but clears the selection when switching tabs", async () => {
    const { user } = renderView();
    await screen.findByText("Ada Lovelace");

    await user.click(screen.getByRole("button", { name: "Person" }));
    await user.click(screen.getByRole("checkbox", { name: "Select Ada Lovelace" }));
    expect(screen.getByRole("button", { name: "Archive selected" })).toBeInTheDocument();

    await openTab(user, /Confirmed/);
    // The Person chip persists (the Concept Engine is filtered out) while the
    // Detected selection does not follow.
    expect(screen.getByRole("button", { name: "Person" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.queryByRole("button", { name: "Archive selected" })).not.toBeInTheDocument();
    expect(await screen.findByText("No confirmed entities yet")).toBeInTheDocument();

    await openTab(user, /Detected/);
    expect(screen.getByRole("button", { name: "Person" })).toHaveAttribute("aria-pressed", "true");
    await screen.findByText("Ada Lovelace");
    expect(screen.queryByRole("button", { name: "Archive selected" })).not.toBeInTheDocument();
  });

  it("moves between tabs with the arrow keys", async () => {
    const { user } = renderView();
    await screen.findByText("Ada Lovelace");

    screen.getByRole("tab", { name: /Detected/ }).focus();
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: /Archived/ })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: /Archived/ })).toHaveFocus();
    expect(await screen.findByText("Countess of Lovelace")).toBeInTheDocument();

    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: /Confirmed/ })).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{End}");
    expect(screen.getByRole("tab", { name: /Archived/ })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tabpanel", { name: /Archived/ })).toBeInTheDocument();
  });

  it("paginates the Detected tab 100 rows at a time with Load more", async () => {
    fixture = Array.from({ length: 130 }, (_, index) =>
      entity({ id: `d${index}`, name: `Detected Entity ${index}`, status: "detected" }),
    );
    const tauri = await import("../../../lib/tauri");
    vi.mocked(tauri.queryEntities).mockImplementation(async (filter) => {
      const all = fixture.filter((candidate) => matchesFilter(candidate, filter));
      const offset = filter.offset ?? 0;
      const limit = filter.limit ?? 100;
      return { entities: all.slice(offset, offset + limit), total: all.length };
    });

    const { user } = renderView();
    await screen.findByText("Detected Entity 0");
    expect(screen.queryByText("Detected Entity 100")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Load more" }));

    expect(await screen.findByText("Detected Entity 100")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Load more" })).not.toBeInTheDocument();
    // Renders a full 100-row page twice; ~1.5s locally, timed out at the 5s
    // default on the shared CI runner.
  }, 20_000);

  it("renders cards by default on Detected with initials, context, type, count, and the dashed variant", async () => {
    window.localStorage.removeItem("wenlan-entities-view-mode");
    renderView();

    expect(await screen.findByTestId("entities-cards")).toBeInTheDocument();
    expect(screen.queryByRole("table")).not.toBeInTheDocument();

    const adaCard = screen.getByTestId("entity-card-ada");
    expect(adaCard).toHaveClass("asset-card--detected");
    expect(within(adaCard).getByText("AL")).toBeInTheDocument();
    expect(within(adaCard).getByText("Detected in 0 memories. Confirm to keep it.")).toBeInTheDocument();
    expect(within(adaCard).getByText("Person")).toBeInTheDocument();
    expect(within(adaCard).getByText("0 memories")).toBeInTheDocument();
    // No dossier on Detected: the title is plain text, not a button.
    expect(adaCard.querySelector(".asset-card-open")).toBeNull();
    expect(adaCard.querySelector("span.asset-card-title")).not.toBeNull();
  });

  it("drives the selection bar from a card checkbox", async () => {
    window.localStorage.removeItem("wenlan-entities-view-mode");
    const { user } = renderView();
    await screen.findByTestId("entities-cards");

    await user.click(screen.getByRole("checkbox", { name: "Select Ada Lovelace" }));
    expect(screen.getByRole("button", { name: "Confirm selected" })).toBeInTheDocument();
  });

  it("confirms a detected entity from its card", async () => {
    window.localStorage.removeItem("wenlan-entities-view-mode");
    const tauri = await import("../../../lib/tauri");
    const { user } = renderView();
    await screen.findByTestId("entities-cards");

    const adaCard = screen.getByTestId("entity-card-ada");
    await user.click(within(adaCard).getByRole("button", { name: "Confirm" }));

    expect(tauri.confirmEntity).toHaveBeenCalledWith("ada", true);
  });

  it("opens the dossier from a Confirmed card", async () => {
    window.localStorage.removeItem("wenlan-entities-view-mode");
    const { user, onEntityClick } = renderView();
    await screen.findByTestId("entities-cards");
    await openTab(user, /Confirmed/);

    const engineCard = await screen.findByTestId("entity-card-engine");
    await user.click(within(engineCard).getByRole("button", { name: "Open Analytical Engine" }));
    expect(onEntityClick).toHaveBeenCalledWith("engine");
  });

  it("switches to rows from the toggle and persists the preference", async () => {
    window.localStorage.removeItem("wenlan-entities-view-mode");
    const { user } = renderView();
    await screen.findByTestId("entities-cards");

    await user.click(screen.getByRole("button", { name: "Rows" }));

    expect(await screen.findByRole("table")).toBeInTheDocument();
    expect(screen.queryByTestId("entities-cards")).not.toBeInTheDocument();
    expect(window.localStorage.getItem("wenlan-entities-view-mode")).toBe("rows");
    expect(screen.getByRole("button", { name: "Rows" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Cards" })).toHaveAttribute("aria-pressed", "false");
  });

  it("shows Restore and Delete permanently on an archived card", async () => {
    window.localStorage.removeItem("wenlan-entities-view-mode");
    const { user } = renderView();
    await screen.findByTestId("entities-cards");
    await openTab(user, /Archived/);

    const card = await screen.findByTestId("entity-card-countess");
    expect(within(card).getByRole("button", { name: "Restore" })).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: /^Delete .* permanently$/ })).toBeInTheDocument();
    expect(within(card).getByText(/^Archived /)).toBeInTheDocument();
    // No dossier on Archived either: the title is plain text.
    expect(card.querySelector(".asset-card-open")).toBeNull();
  });
});
