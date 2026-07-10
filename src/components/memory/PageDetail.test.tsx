// SPDX-License-Identifier: AGPL-3.0-only
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
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
  exportPageToObsidian: vi.fn().mockResolvedValue({ path: "/path/to/file.md" }),
  listRegisteredSources: vi.fn().mockResolvedValue([
    { id: "obsidian-vault", source_type: "obsidian", path: "/Users/test/vault", status: "Active", last_sync: null, file_count: 10, memory_count: 20 },
  ]),
  getPageLinks: vi.fn().mockResolvedValue({ outbound: [], inbound: [] }),
  listOrphanLinks: vi.fn().mockResolvedValue({ min_count: 2, orphan_labels: [] }),
  listPages: vi.fn().mockResolvedValue([]),
  redistillPage: vi.fn().mockResolvedValue({ status: "ok", updated: true }),
  updatePage: vi.fn().mockResolvedValue(undefined),
  deletePage: vi.fn().mockResolvedValue(undefined),
  FACET_COLORS: {},
  STABILITY_TIERS: {},
  getPendingRevision: vi.fn().mockResolvedValue(null),
  acceptPendingRevision: vi.fn(),
  dismissPendingRevision: vi.fn(),
}));

function renderWithQuery(
  ui: React.ReactElement,
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } }),
) {
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

  it("re-distills the current page and keeps skipped daemon hints visible", async () => {
    const { redistillPage } = await import("../../lib/tauri");
    const hint = "page re-distill needs an LLM in the daemon";
    (redistillPage as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      status: "skipped",
      updated: false,
      hint,
    });
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
    const { user } = renderWithQuery(<PageDetail {...defaultProps} />, queryClient);

    await screen.findByText("libSQL Architecture");
    await user.click(screen.getByTitle("Re-distill page"));

    expect(redistillPage).toHaveBeenCalledWith("concept_abc");
    expect(await screen.findByText(hint)).toBeTruthy();
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["page", "concept_abc"] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["page-revisions", "concept_abc"] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["page-sources", "concept_abc"] });
    expect(defaultProps.onBack).not.toHaveBeenCalled();
  });

  it("confirms before re-distilling a user-edited page", async () => {
    const { getPage, redistillPage } = await import("../../lib/tauri");
    (getPage as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      id: "concept_abc",
      title: "Edited Page",
      summary: null,
      content: "Edited page prose.",
      entity_id: null,
      domain: null,
      source_memory_ids: [],
      version: 4,
      status: "active",
      created_at: "2026-04-01T00:00:00+00:00",
      last_compiled: "2026-04-07T12:00:00+00:00",
      last_modified: "2026-04-08T12:00:00+00:00",
      user_edited: true,
    });
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    const { user } = renderWithQuery(<PageDetail {...defaultProps} />);

    await screen.findByText("Edited Page");
    await user.click(screen.getByTitle("Re-distill page"));

    expect(confirmSpy).toHaveBeenCalledWith(
      "Re-distill this edited page? The current version stays in page history for recovery.",
    );
    expect(redistillPage).not.toHaveBeenCalled();
    confirmSpy.mockRestore();
  });

  it("clears re-distill notices when navigating between pages", async () => {
    const { getPage, redistillPage } = await import("../../lib/tauri");
    const hint = "page re-distill needs an LLM in the daemon";
    (redistillPage as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      status: "skipped",
      updated: false,
      hint,
    });
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { user, rerender } = renderWithQuery(<PageDetail {...defaultProps} />, queryClient);

    await screen.findByText("libSQL Architecture");
    await user.click(screen.getByTitle("Re-distill page"));
    expect(await screen.findByText(hint)).toBeTruthy();

    (getPage as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      id: "concept_next",
      title: "Next Page",
      summary: null,
      content: "Next page content.",
      entity_id: null,
      domain: null,
      source_memory_ids: [],
      version: 1,
      status: "active",
      created_at: "2026-04-09T00:00:00+00:00",
      last_compiled: "2026-04-09T12:00:00+00:00",
      last_modified: "2026-04-09T12:00:00+00:00",
    });
    rerender(
      <QueryClientProvider client={queryClient}>
        <PageDetail {...defaultProps} pageId="concept_next" />
      </QueryClientProvider>,
    );

    await waitFor(() => expect(screen.queryByText(hint)).toBeNull());
  });

  it("renders back button as SVG arrow", async () => {
    const { container } = renderWithQuery(<PageDetail {...defaultProps} />);
    await screen.findByText("libSQL Architecture");
    const backBtn = container.querySelector("button svg");
    expect(backBtn).toBeTruthy();
  });

  it("renders source memories section with count", async () => {
    renderWithQuery(<PageDetail {...defaultProps} />);
    await screen.findByText("libSQL Architecture");
    expect(screen.getByText(/2 sources/)).toBeInTheDocument();
  });

  it("uses the page-detail grammar class (matches MemoryDetail's dossier pattern)", async () => {
    const { container } = renderWithQuery(<PageDetail {...defaultProps} />);
    await screen.findByText("libSQL Architecture");
    const topDiv = container.firstElementChild as HTMLElement;
    expect(topDiv.className).toContain("page-detail");
    expect(topDiv.className).not.toContain("h-screen");
  });

  it("shows Page Links from daemon page links without listPages inference", async () => {
    const { getPageLinks, listPages: mockList } = await import("../../lib/tauri");
    (getPageLinks as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      outbound: [{ label: "Entity Graph", target_page_id: "concept_eg" }],
      inbound: [],
    });
    renderWithQuery(<PageDetail {...defaultProps} />);
    expect(await screen.findByLabelText("Related pages")).toBeTruthy();
    const entityEls = await screen.findAllByText("Entity Graph");
    const cardSpan = entityEls.find((el) => el.tagName === "SPAN");
    expect(cardSpan).toBeTruthy();
    expect(mockList).not.toHaveBeenCalled();
  });

  it("hides Page Links when the daemon returns no links", async () => {
    const { listPages: mockList } = await import("../../lib/tauri");
    renderWithQuery(<PageDetail {...defaultProps} />);
    await screen.findByText("libSQL Architecture");
    expect(screen.queryByLabelText("Related pages")).toBeNull();
    expect(screen.getByText(/Page info/i)).toBeInTheDocument();
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
    expect(screen.queryByLabelText("Related pages")).toBeNull();
    expect(screen.getByText(/Page info/i)).toBeInTheDocument();
  });

  it("renders one evidence card per source memory after fetch", async () => {
    const { user } = renderWithQuery(<PageDetail {...defaultProps} />);
    await screen.findByText("libSQL Architecture");
    await user.click(screen.getByText(/Page info/i));
    expect(screen.getAllByTestId("page-info-source-row")).toHaveLength(2);
  });

  it("clicking an evidence card calls onMemoryClick with the right source_id", async () => {
    const { user } = renderWithQuery(<PageDetail {...defaultProps} />);
    await screen.findByText("libSQL Architecture");
    await user.click(screen.getByText(/Page info/i));
    const row = screen
      .getByText("libSQL stores vectors")
      .closest('[data-testid="page-info-source-row"]')!;
    await user.click(row);
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
