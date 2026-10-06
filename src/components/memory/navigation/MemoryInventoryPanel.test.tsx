// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ComponentProps } from "react";
import type { MemoryItem } from "../../../lib/tauri";
import { MemoryInventoryPanel } from "./MemoryInventoryPanel";

const { listMemoriesMock, getMemoryMock } = vi.hoisted(() => ({
  listMemoriesMock: vi.fn(),
  getMemoryMock: vi.fn(),
}));
vi.mock("../../../lib/tauri", () => ({
  listMemoriesRich: listMemoriesMock,
  getMemoryDetail: getMemoryMock,
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    i18n: { language: "en" },
    t: (key: string, options?: { title?: string }) => {
      if (key === "knowledgeContext.openMemory") return `Open ${options?.title}`;
      return ({
        "knowledgeContext.memoryList": "Memory list",
        "knowledgeContext.recentMemories": "Recent memories",
        "knowledgeContext.memoryHint": "Memories are saved fragments.",
        "knowledgeContext.filterMemories": "Filter memories",
        "knowledgeContext.loadingMemories": "Loading memories…",
        "knowledgeContext.memoryLoadError": "Memories couldn't be loaded.",
        "knowledgeContext.noMemories": "No memories yet",
        "knowledgeContext.noMatches": "No matching memories",
        "memoryDetail.untitledMemory": "Untitled memory",
        "memoryDetail.notFoundTitle": "Memory not found",
        "pageDetail.retry": "Try again",
      } as Record<string, string>)[key] ?? key;
    },
  }),
}));

function memory(sourceId: string, title: string, content = ""): MemoryItem {
  return {
    source_id: sourceId, title, content, summary: null, memory_type: "fact",
    domain: null, source_agent: null, confidence: null, confirmed: false,
    pinned: false, supersedes: null, last_modified: 1_700_000_000, chunk_count: 1,
  };
}

function renderPanel(
  props: Partial<ComponentProps<typeof MemoryInventoryPanel>> = {},
  queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } }),
) {
  return {
    queryClient,
    ...render(
      <QueryClientProvider client={queryClient}>
        <MemoryInventoryPanel onOpenMemory={() => {}} {...props} />
      </QueryClientProvider>,
    ),
  };
}

describe("MemoryInventoryPanel", () => {
  beforeEach(() => {
    listMemoriesMock.mockReset().mockResolvedValue([]);
    getMemoryMock.mockReset().mockResolvedValue(null);
  });

  it("reads only the bounded recent list and opens content-first rows by source ID", async () => {
    listMemoriesMock.mockResolvedValue([
      memory("a", "Decision title", "First saved line\nLater line"),
      memory("b", "Title fallback"),
      memory("c", ""),
    ]);
    const onOpenMemory = vi.fn();
    const user = userEvent.setup();
    const { container } = renderPanel({ currentMemoryId: "a", onOpenMemory });

    const current = await screen.findByRole("button", { name: "Open Decision title", current: "page" });
    expect(screen.getByRole("heading", { name: "Recent memories" })).toBeInTheDocument();
    expect(current.querySelector(".notes-memory-preview")).toHaveTextContent("First saved line");
    expect(current.querySelector(".notes-memory-title")).toHaveTextContent("Decision title");
    expect(screen.queryByText("Later line")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open Title fallback" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open Untitled memory" })).toBeInTheDocument();
    expect(container).not.toHaveTextContent("1700000000");
    expect(listMemoriesMock).toHaveBeenCalledWith(undefined, undefined, undefined, 200);
    expect(getMemoryMock).not.toHaveBeenCalled();
    await user.click(current);
    expect(onOpenMemory).toHaveBeenCalledWith("a");
  });

  it("filters loaded title and content locally and distinguishes no matches", async () => {
    listMemoriesMock.mockResolvedValue([
      memory("a", "Decision", "First line\nSearchable second line"),
      memory("b", "Project plan", "Different content"),
    ]);
    const user = userEvent.setup();
    renderPanel();
    await screen.findByRole("button", { name: "Open Decision" });
    const filter = screen.getByRole("searchbox", { name: "Filter memories" });
    await user.type(filter, "SEARCHABLE");
    expect(screen.getByRole("button", { name: "Open Decision" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open Project plan" })).not.toBeInTheDocument();
    await user.clear(filter);
    await user.type(filter, "project");
    expect(screen.getByRole("button", { name: "Open Project plan" })).toBeInTheDocument();
    await user.type(filter, " unavailable");
    expect(screen.getByText("No matching memories")).toBeInTheDocument();
    expect(listMemoriesMock).toHaveBeenCalledTimes(1);
  });

  it("shows loading and empty states without implying a complete library", async () => {
    let resolveList!: (memories: MemoryItem[]) => void;
    listMemoriesMock.mockReturnValue(new Promise<MemoryItem[]>((resolve) => { resolveList = resolve; }));
    renderPanel();
    expect(screen.getByRole("status")).toHaveTextContent("Loading memories…");
    expect(screen.queryByText("No memories yet")).not.toBeInTheDocument();
    resolveList([]);
    expect(await screen.findByText("No memories yet")).toBeInTheDocument();
  });

  it("reuses the memories cache and supplements the selected older memory at the top", async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } });
    queryClient.setQueryData(["memories"], [memory("recent", "Recent title", "Recent fragment")]);
    getMemoryMock.mockResolvedValue(memory("older", "Older title", "Older fragment"));
    renderPanel({ currentMemoryId: "older" }, queryClient);
    const selected = await screen.findByRole("button", { name: "Open Older title", current: "page" });
    expect(screen.getAllByRole("button")[0]).toBe(selected);
    expect(screen.getByRole("button", { name: "Open Recent title" })).toBeInTheDocument();
    expect(getMemoryMock).toHaveBeenCalledWith("older");
    expect(listMemoriesMock).not.toHaveBeenCalled();
    expect(queryClient.getQueryData<MemoryItem[]>(["memories"])).toHaveLength(1);
  });

  it("retries a failed recent-list read", async () => {
    listMemoriesMock.mockRejectedValueOnce(new Error("unavailable")).mockResolvedValue([memory("a", "Recovered")]);
    const user = userEvent.setup();
    renderPanel();
    expect(await screen.findByRole("alert")).toHaveTextContent("Memories couldn't be loaded.");
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("button", { name: "Open Recovered" })).toBeInTheDocument();
  });

  it("keeps recent rows available while retrying a selected-memory supplement", async () => {
    listMemoriesMock.mockResolvedValue([memory("recent", "Recent title")]);
    getMemoryMock.mockRejectedValueOnce(new Error("unavailable")).mockResolvedValue(memory("older", "Older title"));
    const user = userEvent.setup();
    renderPanel({ currentMemoryId: "older" });
    expect(await screen.findByRole("alert")).toHaveTextContent("Memories couldn't be loaded.");
    expect(screen.getByRole("button", { name: "Open Recent title" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Open Older title", current: "page" })).toBeInTheDocument());
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
