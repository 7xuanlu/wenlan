// SPDX-License-Identifier: AGPL-3.0-only
import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import PageDetail from "./PageDetail";

const tauriMocks = vi.hoisted(() => ({
  getPage: vi.fn(),
  getPageSources: vi.fn(),
  listRegisteredSources: vi.fn(),
  getPageLinks: vi.fn(),
  getPageRevisions: vi.fn(),
  listPages: vi.fn(),
  updatePage: vi.fn(),
  deletePage: vi.fn(),
  clipboardWrite: vi.fn(),
  exportPageToObsidian: vi.fn(),
}));

vi.mock("../../lib/tauri", () => ({
  ...tauriMocks,
  FACET_COLORS: {},
  STABILITY_TIERS: {},
}));

const LINKED_PAGE = {
  id: "page-1",
  title: "Link Test Page",
  summary: null,
  content:
    "Intro sentence.\n\nThis page references [[Resolved Link]] and [[Missing Link]].\n\nIt also cites [memory](#memory:mem-1).",
  entity_id: null,
  domain: "testing",
  source_memory_ids: [],
  version: 1,
  status: "active",
  created_at: "2026-06-26T00:00:00+00:00",
  last_compiled: "2026-06-26T00:00:00+00:00",
  last_modified: "2026-06-26T00:00:00+00:00",
};

function renderWithQuery(ui: React.ReactElement, client = new QueryClient({ defaultOptions: { queries: { retry: false } } })) {
  return {
    client,
    user: userEvent.setup(),
    ...render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>),
  };
}

describe("PageDetail page links", () => {
  const defaultProps = {
    pageId: "page-1",
    onBack: vi.fn(),
    onMemoryClick: vi.fn(),
    onPageClick: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    tauriMocks.getPage.mockResolvedValue(LINKED_PAGE);
    tauriMocks.getPageSources.mockResolvedValue([]);
    tauriMocks.listRegisteredSources.mockResolvedValue([]);
    tauriMocks.getPageLinks.mockResolvedValue({ outbound: [], inbound: [] });
    tauriMocks.getPageRevisions.mockResolvedValue({
      page_id: "page-1",
      current_version: 1,
      user_edited: false,
      stale_reason: null,
      entries: [],
    });
    tauriMocks.listPages.mockResolvedValue([]);
    tauriMocks.updatePage.mockResolvedValue(undefined);
    tauriMocks.deletePage.mockResolvedValue(undefined);
    tauriMocks.clipboardWrite.mockResolvedValue(undefined);
    tauriMocks.exportPageToObsidian.mockResolvedValue("/tmp/page.md");
  });

  it("uses daemon page links for related links and wikilink navigation", async () => {
    tauriMocks.getPageLinks.mockResolvedValue({
      outbound: [
        { label: "Resolved Link", target_page_id: "page-2" },
        { label: "Missing Link", target_page_id: null },
      ],
      inbound: [{ source_page_id: "page-3", label: "Inbound Mention" }],
    });

    const { user } = renderWithQuery(<PageDetail {...defaultProps} />);

    expect(await screen.findByText("Link Test Page")).toBeInTheDocument();
    await waitFor(() => {
      expect(tauriMocks.getPageLinks).toHaveBeenCalledWith("page-1");
    });
    expect(tauriMocks.listPages).not.toHaveBeenCalled();

    const contentLink = await screen.findByRole("link", { name: "Resolved Link" });
    await user.click(contentLink);
    expect(defaultProps.onPageClick).toHaveBeenCalledWith("page-2");

    const linksSection = await screen.findByLabelText("Page links");
    const resolvedButton = within(linksSection).getByRole("button", { name: /Resolved Link/ });
    await user.click(resolvedButton);
    expect(defaultProps.onPageClick).toHaveBeenCalledWith("page-2");

    expect(within(linksSection).getByText("Missing Link")).toBeInTheDocument();
    expect(within(linksSection).queryByRole("button", { name: /Missing Link/ })).toBeNull();
    expect(within(linksSection).getByText("Inbound Mention")).toBeInTheDocument();
  });

  it("invalidates page links after saving edited content", async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { user } = renderWithQuery(<PageDetail {...defaultProps} />, client);

    expect(await screen.findByText("Link Test Page")).toBeInTheDocument();
    await user.click(screen.getByTitle("Edit page"));
    const editor = screen.getByRole("textbox");
    fireEvent.change(editor, {
      target: { value: "Intro sentence.\n\nThis page now links [[New Link]]." },
    });
    await user.click(screen.getByRole("button", { name: /Save/ }));

    await waitFor(() => {
      expect(tauriMocks.updatePage).toHaveBeenCalledWith(
        "page-1",
        "Intro sentence.\n\nThis page now links [[New Link]].",
      );
    });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["page-links", "page-1"] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["page-revisions", "page-1"] });
  });

  it("shows source identity for duplicate inbound link labels", async () => {
    tauriMocks.getPageLinks.mockResolvedValue({
      outbound: [],
      inbound: [
        { source_page_id: "source-page-a", label: "Shared Mention" },
        { source_page_id: "source-page-b", label: "Shared Mention" },
      ],
    });

    renderWithQuery(<PageDetail {...defaultProps} />);

    const linksSection = await screen.findByLabelText("Page links");
    expect(within(linksSection).getAllByText("Shared Mention")).toHaveLength(2);
    expect(within(linksSection).getByText("from source-page-a")).toBeInTheDocument();
    expect(within(linksSection).getByText("from source-page-b")).toBeInTheDocument();
  });

  it("uses daemon target labels for alias and heading wikilinks", async () => {
    tauriMocks.getPage.mockResolvedValueOnce({
      ...LINKED_PAGE,
      content:
        "Intro sentence.\n\nAlias [[Resolved Link|Alias Text]], heading [[Resolved Link#Section]], unresolved [[Missing Link|Missing Alias]].",
    });
    tauriMocks.getPageLinks.mockResolvedValue({
      outbound: [{ label: "Resolved Link", target_page_id: "resolved-page" }],
      inbound: [],
    });

    const { user } = renderWithQuery(<PageDetail {...defaultProps} />);

    expect(await screen.findByText("Link Test Page")).toBeInTheDocument();
    const aliasLink = await screen.findByRole("link", { name: "Alias Text" });
    const headingLink = await screen.findByRole("link", { name: "Resolved Link" });
    expect(screen.getByText(/Missing Alias/)).toBeInTheDocument();
    expect(screen.queryByText("Missing Link|Missing Alias")).toBeNull();

    await user.click(aliasLink);
    await user.click(headingLink);
    expect(defaultProps.onPageClick).toHaveBeenCalledWith("resolved-page");
    expect(defaultProps.onPageClick).toHaveBeenCalledTimes(2);
  });

  it("keeps the page visible and hides links when the daemon route fails", async () => {
    tauriMocks.getPageLinks.mockRejectedValue(new Error("HTTP GET /api/pages/page-1/links returned 404"));

    renderWithQuery(<PageDetail {...defaultProps} />);

    expect(await screen.findByText("Link Test Page")).toBeInTheDocument();
    expect(await screen.findByText(/This page references/)).toBeInTheDocument();
    await waitFor(() => {
      expect(tauriMocks.getPageLinks).toHaveBeenCalledWith("page-1");
    });
    expect(tauriMocks.listPages).not.toHaveBeenCalled();
    expect(screen.queryByLabelText("Page links")).toBeNull();
  });

  it("renders daemon page revision history", async () => {
    tauriMocks.getPageRevisions.mockResolvedValue({
      page_id: "page-1",
      current_version: 2,
      user_edited: false,
      stale_reason: null,
      entries: [
        {
          version: 2,
          at: Math.floor(Date.now() / 1000),
          edited_by: "distill",
          delta_summary: "Added backlinks",
          incoming_source_ids: ["mem-1"],
        },
      ],
    });

    renderWithQuery(<PageDetail {...defaultProps} />);

    expect(await screen.findByText(/revision history/i)).toBeInTheDocument();
    expect(screen.getByText(/added backlinks/i)).toBeInTheDocument();
    expect(within(screen.getByLabelText("Revision history")).getByText("just now")).toBeInTheDocument();
  });

  it("keeps rendering the page when page revisions route is unavailable", async () => {
    tauriMocks.getPageRevisions.mockRejectedValue(new Error("404"));

    renderWithQuery(<PageDetail {...defaultProps} />);

    expect(await screen.findByText("Link Test Page")).toBeInTheDocument();
    expect(screen.queryByText(/revision history/i)).toBeNull();
  });
});
