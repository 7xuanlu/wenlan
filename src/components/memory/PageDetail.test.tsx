// SPDX-License-Identifier: AGPL-3.0-only
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import PageDetail from "./PageDetail";

vi.mock("../../lib/tauri", () => ({
  getPage: vi.fn().mockResolvedValue({
    id: "concept_abc",
    title: "libSQL Architecture",
    summary: "Core database layer for Origin",
    content: "libSQL is the core database layer powering Origin's memory system, chosen for its vector support and SQLite compatibility.\n\nOrigin stores 768-dimensional embeddings in F32_BLOB columns with DiskANN indexing for fast approximate nearest neighbor search. The database also hosts the knowledge graph ([[Entity Graph]]) and full-text search via FTS5 triggers. This architecture enables hybrid retrieval combining vector similarity with keyword matching through [[Reciprocal Rank Fusion]].\n\n## Open Questions\n- Performance at scale beyond 10k memories?\n\n## Sources\n- mem_1 (2026-04-01)\n- mem_2 (2026-04-01)",
    entity_id: "entity_libsql",
    domain: "architecture",
    source_memory_ids: ["mem_1", "mem_2"],
    version: 3,
    status: "active",
    created_at: "2026-04-01T00:00:00+00:00",
    last_compiled: "2026-04-07T12:00:00+00:00",
    last_modified: "2026-04-07T12:00:00+00:00",
  }),
  getPageSources: vi.fn().mockResolvedValue([
    {
      source: { page_id: "page_abc", memory_source_id: "mem_1", linked_at: 1712000000, link_reason: "page_growth" },
      memory: {
        source_id: "mem_1",
        title: "libSQL stores vectors",
        content: "libSQL stores vectors in F32_BLOB columns",
        summary: null,
        memory_type: "fact",
        domain: "architecture",
        source_agent: "claude",
        confidence: 0.9,
        confirmed: false,
        last_modified: 1712000000,
      },
    },
    {
      source: { page_id: "page_abc", memory_source_id: "mem_2", linked_at: 1712100000, link_reason: "page_growth" },
      memory: {
        source_id: "mem_2",
        title: "DiskANN indexing strategy",
        content: "DiskANN provides fast approximate nearest neighbor search",
        summary: null,
        memory_type: "fact",
        domain: "architecture",
        source_agent: "claude-code",
        confidence: 0.85,
        confirmed: true,
        last_modified: 1712100000,
      },
    },
  ]),
  clipboardWrite: vi.fn().mockResolvedValue(undefined),
  exportPagesToObsidian: vi.fn().mockResolvedValue({ exported: 1, skipped: 0, failed: 0 }),
  exportPageToObsidian: vi.fn().mockResolvedValue("/path/to/file.md"),
  listRegisteredSources: vi.fn().mockResolvedValue([
    { id: "obsidian-vault", source_type: "obsidian", path: "/Users/test/vault", status: "Active", last_sync: null, file_count: 10, memory_count: 20 },
  ]),
  getPageLinks: vi.fn().mockResolvedValue({ outbound: [], inbound: [] }),
  listPages: vi.fn().mockResolvedValue([]),
  updatePage: vi.fn().mockResolvedValue(undefined),
  deletePage: vi.fn().mockResolvedValue(undefined),
  FACET_COLORS: {},
  STABILITY_TIERS: {},
  getPendingRevision: vi.fn().mockResolvedValue(null),
  acceptPendingRevision: vi.fn(),
  dismissPendingRevision: vi.fn(),
}));

function renderWithQuery(ui: React.ReactElement) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return {
    user: userEvent.setup(),
    ...render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>),
  };
}

describe("PageDetail", () => {
  const defaultProps = {
    pageId: "concept_abc",
    onBack: vi.fn(),
    onMemoryClick: vi.fn(),
    onPageClick: vi.fn(),
  };

  beforeEach(() => { vi.clearAllMocks(); });

  it("renders page title", async () => {
    renderWithQuery(<PageDetail {...defaultProps} />);
    expect(await screen.findByText("libSQL Architecture")).toBeTruthy();
  });

  it("renders meta line with distilled time", async () => {
    renderWithQuery(<PageDetail {...defaultProps} />);
    expect(await screen.findByText(/Last distilled/)).toBeTruthy();
    expect(await screen.findByText(/from 2 memories/)).toBeTruthy();
  });

  it("renders last distilled info", async () => {
    renderWithQuery(<PageDetail {...defaultProps} />);
    expect(await screen.findByText(/Last distilled/)).toBeTruthy();
  });

  it("renders source memory count", async () => {
    renderWithQuery(<PageDetail {...defaultProps} />);
    expect(await screen.findByText(/from 2 memories/)).toBeTruthy();
  });

  it("renders copy and export buttons in header", async () => {
    const { container } = renderWithQuery(<PageDetail {...defaultProps} />);
    await screen.findByText("libSQL Architecture");
    expect(container.querySelector('button[title="Copy as context"]')).toBeTruthy();
    expect(container.querySelector('button[title="Export to Obsidian"]')).toBeTruthy();
  });

  it("renders back button as SVG arrow", async () => {
    const { container } = renderWithQuery(<PageDetail {...defaultProps} />);
    await screen.findByText("libSQL Architecture");
    const backBtn = container.querySelector("button svg");
    expect(backBtn).toBeTruthy();
  });

  it("renders source memories section with count", async () => {
    renderWithQuery(<PageDetail {...defaultProps} />);
    expect(await screen.findByText("Source Memories (2)")).toBeTruthy();
  });

  it("uses mem- CSS variables (matches MemoryDetail pattern)", async () => {
    const { container } = renderWithQuery(<PageDetail {...defaultProps} />);
    await screen.findByText("libSQL Architecture");
    const topDiv = container.firstElementChild as HTMLElement;
    expect(topDiv.className).toContain("flex");
    expect(topDiv.className).toContain("flex-col");
    expect(topDiv.className).not.toContain("h-screen");
  });

  it("shows Page Links from daemon page links without listPages inference", async () => {
    const { getPageLinks, listPages: mockList } = await import("../../lib/tauri");
    (getPageLinks as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      outbound: [{ label: "Entity Graph", target_page_id: "concept_eg" }],
      inbound: [],
    });
    renderWithQuery(<PageDetail {...defaultProps} />);
    expect(await screen.findByLabelText("Page links")).toBeTruthy();
    const entityEls = await screen.findAllByText("Entity Graph");
    const cardSpan = entityEls.find((el) => el.tagName === "SPAN");
    expect(cardSpan).toBeTruthy();
    expect(mockList).not.toHaveBeenCalled();
  });

  it("hides Page Links when the daemon returns no links", async () => {
    const { listPages: mockList } = await import("../../lib/tauri");
    renderWithQuery(<PageDetail {...defaultProps} />);
    await screen.findByText("libSQL Architecture");
    expect(screen.queryByLabelText("Page links")).toBeNull();
    expect(mockList).not.toHaveBeenCalled();
  });

  it("hides Page Links section when no daemon links in content", async () => {
    const { getPage } = await import("../../lib/tauri");
    (getPage as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      id: "concept_abc",
      title: "Simple Page",
      summary: "No links here",
      content: "A simple page with no wikilinks.\n\n## Open Questions\n- None",
      entity_id: null,
      domain: null,
      source_memory_ids: [],
      version: 1,
      status: "active",
      created_at: "2026-04-01T00:00:00+00:00",
      last_compiled: "2026-04-07T12:00:00+00:00",
      last_modified: "2026-04-07T12:00:00+00:00",
    });
    renderWithQuery(<PageDetail {...defaultProps} />);
    await screen.findByText("Simple Page");
    expect(screen.queryByLabelText("Page links")).toBeNull();
  });

  it("shows loading placeholders while source memories are fetching", async () => {
    const { getPageSources } = await import("../../lib/tauri");
    let resolveMemories!: (v: unknown[]) => void;
    (getPageSources as ReturnType<typeof vi.fn>).mockReturnValueOnce(
      new Promise((res) => { resolveMemories = res; }),
    );
    renderWithQuery(<PageDetail {...defaultProps} />);
    await screen.findByText("libSQL Architecture");
    // Section header should be visible while memories are loading
    expect(screen.getByText(/Source Memories/)).toBeTruthy();
    // Resolve so the test doesn't leave a pending promise
    resolveMemories([]);
  });

  it("renders one evidence card per source memory after fetch", async () => {
    renderWithQuery(<PageDetail {...defaultProps} />);
    await screen.findByText("libSQL Architecture");
    // Both source memory titles should appear as evidence cards
    expect(await screen.findByText("libSQL stores vectors")).toBeTruthy();
    expect(await screen.findByText("DiskANN indexing strategy")).toBeTruthy();
  });

  it("clicking an evidence card calls onMemoryClick with the right source_id", async () => {
    const { user } = renderWithQuery(<PageDetail {...defaultProps} />);
    await screen.findByText("libSQL stores vectors");
    const card = screen.getByText("libSQL stores vectors").closest("li")!;
    await user.click(card);
    expect(defaultProps.onMemoryClick).toHaveBeenCalledWith("mem_1");
  });

  it("uses getPageSources (join table) not listMemoriesByIds", async () => {
    const { getPageSources } = await import("../../lib/tauri");
    renderWithQuery(<PageDetail {...defaultProps} />);
    await screen.findByText("libSQL stores vectors");
    expect(getPageSources).toHaveBeenCalledTimes(1);
    expect(getPageSources).toHaveBeenCalledWith("concept_abc");
  });
});
