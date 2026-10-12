// SPDX-License-Identifier: AGPL-3.0-only
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18n } from "../../../i18n";
import type { EntityDetail as Detail } from "../../../lib/tauri";
import EntityDetail from "../EntityDetail";

vi.mock("../../../lib/tauri", () => ({
  getEntityDetail: vi.fn(), search: vi.fn().mockResolvedValue([]), FACET_COLORS: {},
  archiveEntities: vi.fn(), restoreEntities: vi.fn(), confirmEntity: vi.fn(), changeEntityType: vi.fn(),
  deleteEntity: vi.fn(), addObservation: vi.fn().mockResolvedValue("obs-new"),
  updateObservation: vi.fn(), deleteObservation: vi.fn(),
}));
vi.mock("../FocusGraph", () => ({ default: () => <div /> }));

import { changeEntityType, getEntityDetail, search, updateObservation } from "../../../lib/tauri";

const originalMatchMedia = window.matchMedia;
const topic: Detail = {
  entity: {
    id: "topic-a", name: "Topic A", entity_type: "concept", domain: null,
    space: null, source_agent: null, confidence: null, confirmed: false,
    created_at: 1_700_000_000, updated_at: 1_700_000_000,
    memory_count: 1, status: "detected", established_by: null,
  },
  observations: [{
    id: "note-a", entity_id: "topic-a", content: "Original topic note",
    source_agent: null, confidence: null, confirmed: false, created_at: 1_700_000_000,
  }],
  relations: [],
};

function renderTopic() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 }, mutations: { retry: false } },
  });
  return {
    ...render(<QueryClientProvider client={queryClient}>
      <EntityDetail entityId="topic-a" onBack={vi.fn()} onEntityClick={vi.fn()} />
    </QueryClientProvider>),
    queryClient,
    user: userEvent.setup(),
  };
}

describe("Topic Info type control", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    window.matchMedia = vi.fn().mockImplementation((query: string) => ({
      matches: query === "(min-width: 1100px)", media: query, onchange: null,
      addEventListener: vi.fn(), removeEventListener: vi.fn(), addListener: vi.fn(), removeListener: vi.fn(),
      dispatchEvent: vi.fn().mockReturnValue(false),
    }));
    await i18n.changeLanguage("en");
    vi.mocked(getEntityDetail).mockResolvedValue(topic);
    vi.mocked(search).mockResolvedValue([]);
    vi.mocked(changeEntityType).mockResolvedValue(undefined);
    vi.mocked(updateObservation).mockResolvedValue(undefined);
  });

  afterEach(() => { window.matchMedia = originalMatchMedia; });

  it("saves a type change and refreshes topic indexes", async () => {
    let current = topic;
    vi.mocked(getEntityDetail).mockImplementation(async () => current);
    vi.mocked(changeEntityType).mockImplementation(async (_id, entityType) => {
      current = { ...current, entity: { ...current.entity, entity_type: entityType } };
    });
    const { user, queryClient } = renderTopic();
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");
    await screen.findByRole("heading", { name: "Topic A" });
    const type = screen.getByRole("combobox", { name: "Type" });

    await user.selectOptions(type, "person");

    await waitFor(() => expect(changeEntityType).toHaveBeenCalledExactlyOnceWith("topic-a", "person"));
    await waitFor(() => expect(type).toHaveValue("person"));
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ["entityDetail"] });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ["entities"] });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ["knowledge-graph"] });
  });

  it("keeps a failed requested type selected and lets the user retry", async () => {
    let current = topic;
    vi.mocked(getEntityDetail).mockImplementation(async () => current);
    vi.mocked(changeEntityType)
      .mockRejectedValueOnce(new Error("offline"))
      .mockImplementationOnce(async (_id, entityType) => {
        current = { ...current, entity: { ...current.entity, entity_type: entityType } };
      });
    const { user } = renderTopic();
    await screen.findByRole("heading", { name: "Topic A" });
    const type = screen.getByRole("combobox", { name: "Type" });

    await user.selectOptions(type, "person");
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't save the type.");
    expect(type).toHaveValue("person");
    expect(changeEntityType).toHaveBeenCalledExactlyOnceWith("topic-a", "person");

    await user.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(changeEntityType).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(type).toHaveValue("person"));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("locks topic actions and observation editing while the type is saving", async () => {
    let current = topic;
    vi.mocked(getEntityDetail).mockImplementation(async () => current);
    let resolveChange: (() => void) | undefined;
    vi.mocked(changeEntityType).mockImplementationOnce(async (_id, entityType) => new Promise<void>((resolve) => {
      resolveChange = () => {
        current = { ...current, entity: { ...current.entity, entity_type: entityType } };
        resolve();
      };
    }));
    const { user } = renderTopic();
    await screen.findByRole("heading", { name: "Topic A" });
    const type = screen.getByRole("combobox", { name: "Type" });
    await user.selectOptions(type, "person");

    await waitFor(() => expect(changeEntityType).toHaveBeenCalledExactlyOnceWith("topic-a", "person"));
    expect(type).toBeDisabled();
    expect(screen.getByRole("button", { name: "Keep topic" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Edit information — Original topic note" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Topic actions" }));
    expect(screen.getByRole("menuitem", { name: "Archive" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Delete topic" })).toBeDisabled();

    resolveChange?.();
    await waitFor(() => expect(type).toBeEnabled());
    await waitFor(() => expect(type).toHaveValue("person"));
  });

  it("keeps unknown legacy values visible and archived topics read-only", async () => {
    vi.mocked(getEntityDetail).mockResolvedValue({
      ...topic, entity: { ...topic.entity, entity_type: "legacy_kind", status: "archived" },
    });
    renderTopic();
    await screen.findByRole("heading", { name: "Topic A" });
    const type = screen.getByRole("combobox", { name: "Type" });
    expect(type).toHaveValue("legacy_kind");
    expect(type.querySelector('option[value="legacy_kind"]')).toHaveTextContent("Legacy Kind (current)");
    expect(type).toBeDisabled();
    expect(changeEntityType).not.toHaveBeenCalled();
  });

  it("does not change type when the current observation draft cannot be saved", async () => {
    vi.mocked(updateObservation).mockRejectedValueOnce(new Error("offline"));
    const { user } = renderTopic();
    await screen.findByRole("heading", { name: "Topic A" });
    await user.click(screen.getByRole("button", { name: "Edit information — Original topic note" }));
    const note = screen.getByRole("textbox", { name: "Edit information" });
    fireEvent.change(note, { target: { value: "A changed note" } });
    fireEvent.blur(note);
    await waitFor(() => expect(updateObservation).toHaveBeenCalledExactlyOnceWith("note-a", "A changed note"));

    await waitFor(() => expect(screen.getByRole("button", { name: "Retry save" })).toBeEnabled());
    const type = screen.getByRole("combobox", { name: "Type" });
    await user.selectOptions(type, "person");
    await waitFor(() => expect(type).toHaveValue("concept"));
    expect(changeEntityType).not.toHaveBeenCalled();
  });
});
