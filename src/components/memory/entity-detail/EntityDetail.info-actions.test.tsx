// SPDX-License-Identifier: AGPL-3.0-only
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { i18n } from "../../../i18n";
import type { EntityDetail as Detail, RelationWithEntity } from "../../../lib/tauri";
import EntityDetail from "../EntityDetail";

vi.mock("../../../lib/tauri", () => ({
  getEntityDetail: vi.fn(), search: vi.fn().mockResolvedValue([]), FACET_COLORS: {},
  archiveEntities: vi.fn(), restoreEntities: vi.fn(), confirmEntity: vi.fn(),
  deleteEntity: vi.fn(), addObservation: vi.fn().mockResolvedValue("obs-new"),
  updateObservation: vi.fn(), deleteObservation: vi.fn(),
}));
vi.mock("../FocusGraph", () => ({ default: () => <div /> }));

import {
  archiveEntities, confirmEntity, deleteEntity, getEntityDetail, search, updateObservation,
} from "../../../lib/tauri";

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

function renderTopic(
  onRegisterFlush?: (flush: (() => Promise<boolean>) | null) => void,
  onMemoryClick?: (sourceId: string) => void,
) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 }, mutations: { retry: false } },
  });
  const rendered = render(
    <QueryClientProvider client={queryClient}>
      <EntityDetail entityId="topic-a" onBack={vi.fn()} onEntityClick={vi.fn()}
        onRegisterFlush={onRegisterFlush} onMemoryClick={onMemoryClick} />
    </QueryClientProvider>,
  );
  return { ...rendered, queryClient, user: userEvent.setup() };
}

async function openInfo() {
  await screen.findByRole("heading", { name: "Topic A" });
  expect(screen.getByRole("button", { name: "Info" })).toHaveAttribute("aria-expanded", "true");
  return screen.getByRole("button", { name: "Keep topic" });
}

describe("Topic Info keep action", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    window.matchMedia = vi.fn().mockImplementation((query: string) => ({
      matches: query === "(min-width: 1100px)",
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn().mockReturnValue(false),
    }));
    await i18n.changeLanguage("en");
    vi.mocked(getEntityDetail).mockResolvedValue(topic);
    vi.mocked(search).mockResolvedValue([]);
    vi.mocked(confirmEntity).mockResolvedValue(undefined);
    vi.mocked(updateObservation).mockResolvedValue(undefined);
  });

  afterEach(() => {
    window.matchMedia = originalMatchMedia;
  });

  it("keeps and releases the topic while refreshing entity caches after each change", async () => {
    let current = topic;
    vi.mocked(getEntityDetail).mockImplementation(async () => current);
    vi.mocked(confirmEntity).mockImplementation(async (_id, confirmed) => {
      current = { ...current, entity: { ...current.entity, confirmed } };
    });
    const { queryClient, user } = renderTopic();
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");
    const keep = await openInfo();

    await user.click(keep);
    const release = await screen.findByRole("button", { name: "Stop keeping topic" });
    expect(release).toHaveAttribute("data-active", "true");
    expect(confirmEntity).toHaveBeenNthCalledWith(1, "topic-a", true);
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ["entityDetail"] });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ["entities"] });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ["knowledge-graph"] });

    await user.click(release);
    const keepAgain = await screen.findByRole("button", { name: "Keep topic" });
    expect(keepAgain).toHaveAttribute("data-active", "false");
    expect(confirmEntity).toHaveBeenNthCalledWith(2, "topic-a", false);
    expect(invalidate.mock.calls.filter(([query]) => JSON.stringify(query) === JSON.stringify({ queryKey: ["entityDetail"] }))).toHaveLength(2);
  });

  it("shows a failed keep action and allows retry without changing the confirmed state", async () => {
    let current = topic;
    vi.mocked(getEntityDetail).mockImplementation(async () => current);
    vi.mocked(confirmEntity)
      .mockRejectedValueOnce(new Error("offline"))
      .mockImplementationOnce(async (_id, confirmed) => {
        current = { ...current, entity: { ...current.entity, confirmed } };
      });
    const { user } = renderTopic();
    const keep = await openInfo();

    await user.click(keep);
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn’t save this change. Try again.");
    expect(screen.getByRole("button", { name: "Keep topic" })).toHaveAttribute("data-active", "false");
    expect(screen.getByRole("button", { name: "Keep topic" })).toBeEnabled();
    expect(confirmEntity).toHaveBeenCalledExactlyOnceWith("topic-a", true);

    await user.click(screen.getByRole("button", { name: "Keep topic" }));
    expect(await screen.findByRole("button", { name: "Stop keeping topic" })).toHaveAttribute("data-active", "true");
    expect(confirmEntity).toHaveBeenCalledTimes(2);
    expect(confirmEntity).toHaveBeenLastCalledWith("topic-a", true);
  });

  it("flushes a dirty observation before keeping the topic", async () => {
    let resolveUpdate: (() => void) | undefined;
    vi.mocked(updateObservation).mockImplementationOnce(() => new Promise<void>((resolve) => {
      resolveUpdate = resolve;
    }));
    const { user } = renderTopic();
    const keep = await openInfo();
    await user.click(screen.getByRole("button", { name: "Edit information — Original topic note" }));
    const input = screen.getByRole("textbox", { name: "Edit information" });
    fireEvent.change(input, { target: { value: "Updated before keep" } });
    fireEvent.blur(input);
    await waitFor(() => expect(updateObservation).toHaveBeenCalledExactlyOnceWith("note-a", "Updated before keep"));
    expect(keep).toBeDisabled();
    expect(confirmEntity).not.toHaveBeenCalled();

    resolveUpdate?.();
    await waitFor(() => expect(keep).toBeEnabled());
    await user.click(keep);
    await waitFor(() => expect(confirmEntity).toHaveBeenCalledExactlyOnceWith("topic-a", true));
  });

  it("does not keep the topic when flushing the observation draft fails", async () => {
    vi.mocked(updateObservation).mockRejectedValueOnce(new Error("write failed"));
    const { user } = renderTopic();
    const keep = await openInfo();
    await user.click(screen.getByRole("button", { name: "Edit information — Original topic note" }));
    const input = screen.getByRole("textbox", { name: "Edit information" });
    fireEvent.change(input, { target: { value: "Unsaved observation" } });

    fireEvent.blur(input);
    await waitFor(() => expect(updateObservation).toHaveBeenCalledExactlyOnceWith("note-a", "Unsaved observation"));
    await waitFor(() => expect(keep).toBeEnabled());
    await user.click(keep);

    expect(confirmEntity).not.toHaveBeenCalled();
    expect(keep).toHaveAttribute("data-active", "false");
    expect(input).toHaveValue("Unsaved observation");
  });

  it("blocks duplicate keep, archive, delete, and observation edits while keep is pending", async () => {
    let current = topic;
    vi.mocked(getEntityDetail).mockImplementation(async () => current);
    let resolveConfirm: (() => void) | undefined;
    vi.mocked(confirmEntity).mockImplementationOnce((_id, confirmed) => new Promise<void>((resolve) => {
      resolveConfirm = () => {
        current = { ...current, entity: { ...current.entity, confirmed } };
        resolve();
      };
    }));
    const { user } = renderTopic();
    const keep = await openInfo();
    await user.click(keep);
    await waitFor(() => expect(keep).toBeDisabled());
    await user.click(keep);
    expect(confirmEntity).toHaveBeenCalledExactlyOnceWith("topic-a", true);
    expect(screen.getByRole("button", { name: "Edit information — Original topic note" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Topic actions" }));
    expect(screen.getByRole("menuitem", { name: "Archive" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Delete topic" })).toBeDisabled();

    resolveConfirm?.();
    await waitFor(() => expect(screen.getByRole("button", { name: "Stop keeping topic" })).toBeEnabled());
    expect(archiveEntities).not.toHaveBeenCalled();
    expect(deleteEntity).not.toHaveBeenCalled();
  });

  it("disables the keep action for archived topics and explains how to change it", async () => {
    vi.mocked(getEntityDetail).mockResolvedValue({
      ...topic,
      entity: { ...topic.entity, confirmed: true, status: "archived" },
    });
    renderTopic();
    await screen.findByRole("heading", { name: "Topic A" });
    expect(screen.getByRole("button", { name: "Info" })).toHaveAttribute("aria-expanded", "true");
    const release = screen.getByRole("button", { name: "Stop keeping topic" });
    expect(release).toBeDisabled();
    expect(release).toHaveAccessibleDescription("Restore this topic before changing whether it is kept.");
    expect(confirmEntity).not.toHaveBeenCalled();
  });

  it("orders Topic Info sections and keeps approximate memory navigation without badges", async () => {
    const relation: RelationWithEntity = {
      id: "relation-topic-b",
      relation_type: "related_to",
      direction: "outgoing",
      entity_id: "topic-b",
      entity_name: "Topic B",
      entity_type: "concept",
      source_agent: "fixture",
      created_at: 1_700_000_100,
    };
    vi.mocked(getEntityDetail).mockResolvedValue({ ...topic, relations: [relation] });
    vi.mocked(search).mockResolvedValue([{
      id: "memory-related",
      content: "A possible related memory",
      source: "fixture",
      source_id: "memory-source",
      title: "Fixture",
      url: null,
      chunk_index: 0,
      last_modified: 1_700_000_200,
      score: 0.8,
      memory_type: "raw",
      entity_id: "topic-a",
      is_archived: true,
    }]);
    const onMemoryClick = vi.fn();
    const { user } = renderTopic(undefined, onMemoryClick);
    await openInfo();

    const details = screen.getByRole("heading", { name: i18n.t("detailInfo.details") });
    const connections = screen.getByRole("heading", { name: i18n.t("entityDetail.connectionsTitle") });
    const related = await screen.findByRole("heading", { name: i18n.t("detailInfo.relatedMemories") });
    const keep = screen.getByRole("button", { name: "Keep topic" });
    expect(keep.compareDocumentPosition(details) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(details.compareDocumentPosition(connections) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(connections.compareDocumentPosition(related) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByText(i18n.t("detailInfo.relatedMemoriesHint"))).toBeInTheDocument();

    const memoryRow = screen.getByRole("button", { name: /A possible related memory/ });
    expect(memoryRow).toHaveTextContent(i18n.t("entityDetail.archived"));
    expect(memoryRow.querySelector(".memory-detail-related-facet")).toBeNull();
    expect(memoryRow.querySelector(".memory-detail-related-chevron")).toBeNull();
    expect(screen.queryByText("raw")).not.toBeInTheDocument();
    await user.click(memoryRow);
    expect(onMemoryClick).toHaveBeenCalledExactlyOnceWith("memory-source");
  });
});
