// SPDX-License-Identifier: AGPL-3.0-only
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { i18n } from "../../../i18n";
import type { EntityDetail as EntityDetailRecord } from "../../../lib/tauri";
import EntityDetail from "../EntityDetail";

vi.mock("../../../lib/tauri", () => ({
  getEntityDetail: vi.fn(), updateObservation: vi.fn(), deleteObservation: vi.fn(),
  addObservation: vi.fn().mockResolvedValue("obs-new"), confirmEntity: vi.fn(),
  deleteEntity: vi.fn(), search: vi.fn().mockResolvedValue([]), FACET_COLORS: {},
}));
import { deleteEntity, getEntityDetail, updateObservation } from "../../../lib/tauri";

const detail: EntityDetailRecord = {
  entity: {
    id: "entity-ada", name: "Ada Lovelace", entity_type: "person", domain: "computing",
    space: null, source_agent: "research-agent", confidence: 0.87, confirmed: false,
    created_at: 1_700_000_000, updated_at: 1_700_086_400, memory_count: 0,
    status: "detected", established_by: null,
  },
  observations: [{ id: "obs-1", entity_id: "entity-ada", content: "Original observation",
    source_agent: "research-agent", confidence: 0.8, confirmed: false, created_at: 1_700_000_100 }],
  relations: [],
};

function renderEntity(onBack = vi.fn(), onRegisterFlush?: (flush: (() => Promise<boolean>) | null) => void) {
  const queryClient = new QueryClient({ defaultOptions: {
    queries: { retry: false, gcTime: 0, refetchOnWindowFocus: false }, mutations: { retry: false },
  } });
  return {
    onBack, queryClient, user: userEvent.setup(), ...render(
      <QueryClientProvider client={queryClient}>
        <EntityDetail entityId="entity-ada" onBack={onBack} onEntityClick={vi.fn()}
          onMemoryClick={vi.fn()} onRegisterFlush={onRegisterFlush} />
      </QueryClientProvider>,
    ),
  };
}

const contentName = "Edit information — Original observation";
const textboxName = "Edit information";
async function runFlush(flush: (() => Promise<boolean>) | null) {
  let saved = false;
  await act(async () => { saved = await flush?.() ?? false; });
  return saved;
}

describe("EntityDetail autosave errors", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await i18n.changeLanguage("en");
    vi.mocked(getEntityDetail).mockResolvedValue(detail);
    vi.mocked(updateObservation).mockResolvedValue(undefined);
  });

  it("keeps a failed autosave draft focused and retries without closing the editor", async () => {
    vi.mocked(updateObservation).mockRejectedValueOnce(new Error("write failed"));
    let flush: (() => Promise<boolean>) | null = null;
    const { user } = renderEntity(undefined, (value) => { flush = value; });
    await user.click(await screen.findByRole("button", { name: contentName }));
    const input = screen.getByRole("textbox", { name: textboxName });
    await user.clear(input);
    await user.type(input, "Draft survives");
    expect(await runFlush(flush)).toBe(false);
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't save. Try again.");
    expect(screen.getByRole("textbox", { name: textboxName })).toHaveValue("Draft survives");
    expect(input).toHaveFocus();
    expect(await runFlush(flush)).toBe(false);
    expect(updateObservation).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "Retry save" }));
    await waitFor(() => expect(updateObservation).toHaveBeenCalledTimes(2));
    expect(screen.getByRole("textbox", { name: textboxName })).toHaveValue("Draft survives");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("keeps the editor open after a successful retry and closes on Escape after saving", async () => {
    vi.mocked(updateObservation).mockRejectedValueOnce(new Error("write failed"));
    let flush: (() => Promise<boolean>) | null = null;
    const { user } = renderEntity(undefined, (value) => { flush = value; });
    await user.click(await screen.findByRole("button", { name: contentName }));
    const input = screen.getByRole("textbox", { name: textboxName });
    await user.type(input, " revised");
    expect(await runFlush(flush)).toBe(false);
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retry save" }));
    await waitFor(() => expect(updateObservation).toHaveBeenCalledTimes(2));
    expect(screen.getByRole("textbox", { name: textboxName })).toHaveValue("Original observation revised");
    await user.type(input, "{Escape}");
    expect(screen.queryByRole("textbox", { name: textboxName })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Edit information — Original observation revised" })).toBeInTheDocument();
  });

  it("clears the save error when the draft is restored to the acknowledged value", async () => {
    vi.mocked(updateObservation).mockRejectedValueOnce(new Error("write failed"));
    let flush: (() => Promise<boolean>) | null = null;
    const { user } = renderEntity(undefined, (value) => { flush = value; });
    await user.click(await screen.findByRole("button", { name: contentName }));
    const input = screen.getByRole("textbox", { name: textboxName });
    await user.type(input, " draft");
    expect(await runFlush(flush)).toBe(false);
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    await user.clear(input);
    await user.type(input, "Original observation");
    expect(await runFlush(flush)).toBe(true);
    await user.type(input, "{Escape}");
    expect(updateObservation).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: textboxName })).not.toBeInTheDocument();
  });

  it("keeps the entity visible and does not navigate back when deletion fails", async () => {
    vi.mocked(deleteEntity).mockRejectedValueOnce(new Error("delete failed"));
    const { onBack, user } = renderEntity();
    await screen.findByRole("heading", { name: "Ada Lovelace" });
    await user.click(screen.getByRole("button", { name: "Topic actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Delete topic" }));
    await user.click(screen.getByRole("button", { name: /^Delete$/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't save. Try again.");
    expect(screen.getByRole("heading", { name: "Ada Lovelace" })).toBeInTheDocument();
    expect(onBack).not.toHaveBeenCalled();
  });

  it("evicts entity-specific caches and refreshes entity indexes after deletion", async () => {
    vi.mocked(deleteEntity).mockResolvedValueOnce(undefined);
    const { queryClient, user } = renderEntity();
    const removeSpy = vi.spyOn(queryClient, "removeQueries");
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
    await screen.findByRole("heading", { name: "Ada Lovelace" });
    await user.click(screen.getByRole("button", { name: "Topic actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Delete topic" }));
    await user.click(screen.getByRole("button", { name: /^Delete$/ }));
    await waitFor(() => expect(deleteEntity).toHaveBeenCalledWith("entity-ada"));
    expect(removeSpy).toHaveBeenCalledWith({ queryKey: ["entityDetail", "entity-ada"] });
    expect(removeSpy).toHaveBeenCalledWith({ queryKey: ["entity-linked-memories", "entity-ada"] });
    for (const queryKey of ["entities", "space-entities", "constellation-entities", "constellation-relations", "connections-entities", "searchEntities"]) {
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: [queryKey] });
    }
  });
});
