// SPDX-License-Identifier: AGPL-3.0-only
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { i18n } from "../../../i18n";
import type { EntityDetail as Detail } from "../../../lib/tauri";
import EntityDetail from "../EntityDetail";

vi.mock("../../../lib/tauri", () => ({
  getEntityDetail: vi.fn(), search: vi.fn().mockResolvedValue([]), FACET_COLORS: {},
  archiveEntities: vi.fn(), restoreEntities: vi.fn(), confirmEntity: vi.fn(),
  deleteEntity: vi.fn(), addObservation: vi.fn(), updateObservation: vi.fn(),
  deleteObservation: vi.fn(), confirmObservation: vi.fn(),
}));
vi.mock("../FocusGraph", () => ({ default: () => <div /> }));

import {
  archiveEntities, restoreEntities, confirmEntity, getEntityDetail, search, updateObservation,
} from "../../../lib/tauri";

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
  const onBack = vi.fn();
  const onEntityClick = vi.fn();
  const renderDetail = (entityId: string) => (
    <QueryClientProvider client={queryClient}>
      <EntityDetail entityId={entityId} onBack={onBack} onEntityClick={onEntityClick} />
    </QueryClientProvider>
  );
  const rendered = render(renderDetail("topic-a"));
  return { ...rendered, queryClient, onBack, user: userEvent.setup(),
    switchTopic: (id: string) => rendered.rerender(renderDetail(id)) };
}

describe("Topic detail lifecycle", () => {
  beforeEach(async () => {
    vi.resetAllMocks();
    await i18n.changeLanguage("en");
    vi.mocked(getEntityDetail).mockResolvedValue(topic);
    vi.mocked(search).mockResolvedValue([]);
  });

  it("archives and restores a detected topic without exposing confirmation controls", async () => {
    let current = topic;
    vi.mocked(getEntityDetail).mockImplementation(async () => current);
    vi.mocked(archiveEntities).mockImplementation(async () => {
      current = { ...topic, entity: { ...topic.entity, status: "archived" } };
      return { count: 1, entity_ids: ["topic-a"], dry_run: false };
    });
    vi.mocked(restoreEntities).mockImplementation(async () => {
      current = topic;
      return { count: 1, entity_ids: ["topic-a"], dry_run: false };
    });
    const { user, onBack, queryClient } = renderTopic();
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");
    await screen.findByRole("heading", { name: "Topic A" });
    expect(confirmEntity).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Topic actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Archive" }));
    await user.click(screen.getByRole("button", { name: "Topic actions" }));
    const restore = await screen.findByRole("menuitem", { name: "Restore" });
    await waitFor(() => expect(restore).toBeEnabled());
    expect(archiveEntities).toHaveBeenCalledExactlyOnceWith({ ids: ["topic-a"], dry_run: false });
    expect(onBack).not.toHaveBeenCalled();
    expect(screen.queryByRole("menuitemcheckbox")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Add information" })).not.toBeInTheDocument();
    expect(screen.getByText("Original topic note")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Edit information —/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Mark as confirmed/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Delete information —/ })).not.toBeInTheDocument();
    expect(screen.getByText("archived")).toHaveClass("entity-archived-notice");
    for (const key of ["entityDetail", "entities", "knowledge-graph", "pages", "recent-concepts", "searchEntities"]) {
      expect(invalidate).toHaveBeenCalledWith({ queryKey: [key] });
    }
    await user.click(restore);
    await user.click(screen.getByRole("button", { name: "Topic actions" }));
    await waitFor(() => expect(screen.getByRole("menuitem", { name: "Archive" })).toBeEnabled());
    expect(restoreEntities).toHaveBeenCalledExactlyOnceWith({ ids: ["topic-a"], dry_run: false });
    expect(screen.queryByRole("button", { name: "Edit" })).not.toBeInTheDocument();
    expect(screen.getByText("Original topic note")).toBeInTheDocument();
    expect(screen.queryByRole("menuitemcheckbox")).not.toBeInTheDocument();
    expect(confirmEntity).not.toHaveBeenCalled();
    expect(onBack).not.toHaveBeenCalled();
  });

  it("shows archive errors on the topic and blocks duplicate or conflicting pending actions", async () => {
    let reject: (error: Error) => void = () => {};
    vi.mocked(archiveEntities).mockImplementationOnce(() => new Promise((_, fail) => { reject = fail; }));
    const { user, onBack } = renderTopic();
    await user.click(await screen.findByRole("button", { name: "Topic actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Archive" }));
    await user.click(screen.getByRole("button", { name: "Topic actions" }));
    const archive = screen.getByRole("menuitem", { name: "Archive" });
    await waitFor(() => expect(archive).toBeDisabled());
    await user.click(archive);
    expect(archiveEntities).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menuitemcheckbox")).not.toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Delete topic" })).toBeDisabled();
    expect(screen.getByText("Original topic note")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Edit information" })).not.toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    reject(new Error("offline"));
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't save. Try again.");
    await user.click(screen.getByRole("button", { name: "Topic actions" }));
    expect(screen.getByRole("menuitem", { name: "Archive" })).toBeEnabled();
    expect(screen.getByRole("heading", { name: "Topic A" })).toBeInTheDocument();
    expect(onBack).not.toHaveBeenCalled();
  });

  it("restores an archived confirmed topic without exposing confirmation metadata", async () => {
    let current: Detail = { ...topic, entity: { ...topic.entity, confirmed: true, status: "archived" } };
    vi.mocked(getEntityDetail).mockImplementation(async () => current);
    vi.mocked(restoreEntities).mockImplementation(async () => {
      current = { ...current, entity: { ...current.entity, status: "established" } };
      return { count: 1, entity_ids: ["topic-a"], dry_run: false };
    });
    const { user } = renderTopic();
    await user.click(await screen.findByRole("button", { name: "Topic actions" }));
    const restore = screen.getByRole("menuitem", { name: "Restore" });
    expect(screen.queryByText("Confirmed")).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitemcheckbox")).not.toBeInTheDocument();
    expect(screen.getByText("archived")).toHaveClass("entity-archived-notice");
    await user.click(restore);
    await user.click(screen.getByRole("button", { name: "Topic actions" }));
    await waitFor(() => expect(screen.getByRole("menuitem", { name: "Archive" })).toBeEnabled());
    expect(screen.queryByText("Confirmed")).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitemcheckbox")).not.toBeInTheDocument();
    expect(confirmEntity).not.toHaveBeenCalled();
  });

  it("blocks archive while a note edit is being saved", async () => {
    let resolve: () => void = () => {};
    vi.mocked(updateObservation).mockImplementationOnce(() => new Promise<void>((done) => { resolve = done; }));
    const { user } = renderTopic();
    await user.click(await screen.findByRole("button", { name: "Edit information — Original topic note" }));
    const input = screen.getByRole("textbox", { name: "Edit information" });
    await user.type(input, " updated");
    await waitFor(() => expect(updateObservation).toHaveBeenCalledExactlyOnceWith("note-a", "Original topic note updated"));
    await user.click(screen.getByRole("button", { name: "Topic actions" }));
    await waitFor(() => expect(screen.getByRole("menuitem", { name: "Archive" })).toBeDisabled());
    resolve();
    await waitFor(() => expect(screen.getByRole("menuitem", { name: "Archive" })).toBeEnabled());
  });

  it("shows restore failure, then retries while keeping topic confirmation absent", async () => {
    const archived: Detail = { ...topic, entity: { ...topic.entity, confirmed: true, status: "archived" } };
    let current = archived;
    vi.mocked(getEntityDetail).mockImplementation(async () => current);
    vi.mocked(restoreEntities)
      .mockRejectedValueOnce(new Error("offline"))
      .mockImplementationOnce(async () => {
        current = topic;
        return { count: 1, entity_ids: ["topic-a"], dry_run: false };
      });
    const { user, queryClient } = renderTopic();
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");
    await user.click(await screen.findByRole("button", { name: "Topic actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Restore" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't save. Try again.");
    expect(screen.getByText("archived")).toHaveClass("entity-archived-notice");
    expect(confirmEntity).not.toHaveBeenCalled();
    expect(screen.queryByRole("menuitemcheckbox")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Topic actions" }));
    const retry = screen.getByRole("menuitem", { name: "Restore" });
    expect(retry).toBeEnabled();
    await user.click(retry);
    await user.click(screen.getByRole("button", { name: "Topic actions" }));
    await waitFor(() => expect(screen.getByRole("menuitem", { name: "Archive" })).toBeEnabled());
    expect(restoreEntities).toHaveBeenCalledTimes(2);
    expect(confirmEntity).not.toHaveBeenCalled();
    expect(screen.queryByRole("menuitemcheckbox")).not.toBeInTheDocument();
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ["entities"] });
  });

  it("blocks repeated archive actions and conflicting deletion while archive is pending", async () => {
    let resolve: () => void = () => {};
    let current = topic;
    vi.mocked(getEntityDetail).mockImplementation(async () => current);
    vi.mocked(archiveEntities).mockImplementationOnce(() => new Promise((done) => {
      resolve = () => {
        current = { ...topic, entity: { ...topic.entity, status: "archived" } };
        done({ count: 1, entity_ids: ["topic-a"], dry_run: false });
      };
    }));
    const { user } = renderTopic();
    await user.click(await screen.findByRole("button", { name: "Topic actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Archive" }));
    await user.click(screen.getByRole("button", { name: "Topic actions" }));
    const archive = screen.getByRole("menuitem", { name: "Archive" });
    expect(archive).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Delete topic" })).toBeDisabled();
    const menu = screen.getByRole("menu", { name: "Topic actions" });
    expect(menu).toHaveFocus();
    await user.keyboard("{ArrowDown}{ArrowUp}");
    expect(menu).toHaveFocus();
    expect(archiveEntities).toHaveBeenCalledExactlyOnceWith({ ids: ["topic-a"], dry_run: false });
    expect(screen.queryByRole("menuitemcheckbox")).not.toBeInTheDocument();
    expect(confirmEntity).not.toHaveBeenCalled();
    resolve();
    await waitFor(() => expect(screen.getByRole("menuitem", { name: "Restore" })).toBeEnabled());
  });

  it("resets delete intent, graph, note drafts and errors on an unkeyed topic switch", async () => {
    const second = { ...topic, entity: { ...topic.entity, id: "topic-b", name: "Topic B" } };
    vi.mocked(getEntityDetail).mockImplementation(async (id) => id === "topic-a" ? topic : second);
    vi.mocked(archiveEntities).mockRejectedValueOnce(new Error("offline"));
    const { user, switchTopic } = renderTopic();
    await user.click(await screen.findByRole("button", { name: "Topic actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Archive" }));
    await screen.findByRole("alert");
    await user.click(screen.getByRole("button", { name: "Edit information — Original topic note" }));
    await user.type(screen.getByRole("textbox", { name: "Edit information" }), " discarded");
    await user.click(screen.getByRole("button", { name: "Topic actions" }));
    await user.click(screen.getByRole("menuitem", { name: "Delete topic" }));
    await user.click(screen.getByRole("button", { name: "Info" }));
    await user.click(screen.getByRole("button", { name: "Full screen" }));
    switchTopic("topic-b");
    await screen.findByRole("heading", { name: "Topic B" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "Edit information" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Delete$/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Topic actions" }));
    expect(screen.getByRole("menuitem", { name: "Delete topic" })).toBeEnabled();
  });
});
