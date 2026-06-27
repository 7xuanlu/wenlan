// SPDX-License-Identifier: AGPL-3.0-only
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import PageDetail from "../PageDetail";
import * as tauri from "../../../lib/tauri";

vi.mock("../../../lib/tauri");

const MOCK_PAGE: tauri.Page = {
  id: "c1",
  title: "Test Page",
  content: "Some page content",
  summary: "A test summary",
  domain: "testing",
  entity_id: null,
  version: 1,
  status: "active",
  created_at: new Date().toISOString(),
  last_compiled: new Date().toISOString(),
  last_modified: new Date().toISOString(),
  source_memory_ids: [],
};

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

describe("PageDetail export", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauri.getPage).mockResolvedValue(MOCK_PAGE);
    vi.mocked(tauri.getPageLinks).mockResolvedValue({ outbound: [], inbound: [] });
    vi.mocked(tauri.listPages).mockResolvedValue([MOCK_PAGE]);
  });

  it("disables export button when no obsidian sources exist", async () => {
    vi.mocked(tauri.listRegisteredSources).mockResolvedValue([]);

    render(
      <PageDetail
        pageId="c1"
        onBack={vi.fn()}
        onMemoryClick={vi.fn()}
      />,
      { wrapper },
    );

    await waitFor(() => {
      expect(screen.getByTitle(/add an obsidian source/i)).toBeInTheDocument();
    });

    const btn = screen.getByTitle(/add an obsidian source/i);
    expect(btn).toBeDisabled();
  });

  it("exports directly when exactly one obsidian source exists", async () => {
    vi.mocked(tauri.listRegisteredSources).mockResolvedValue([
      {
        id: "obsidian-vault",
        source_type: "obsidian",
        path: "/Users/test/vault",
        status: "Active",
        last_sync: null,
        file_count: 10,
        memory_count: 20,
      },
    ]);
    vi.mocked(tauri.exportPageToObsidian).mockResolvedValue(
      "/Users/test/vault/Wenlan/pages/Test Page.md",
    );

    render(
      <PageDetail
        pageId="c1"
        onBack={vi.fn()}
        onMemoryClick={vi.fn()}
      />,
      { wrapper },
    );

    await waitFor(() => {
      expect(screen.getByTitle("Export to Obsidian")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTitle("Export to Obsidian"));

    await waitFor(() => {
      expect(tauri.exportPageToObsidian).toHaveBeenCalledWith(
        "c1",
        "/Users/test/vault/Wenlan/pages",
      );
    });
  });

  it("shows popover menu when 2+ obsidian sources exist", async () => {
    vi.mocked(tauri.listRegisteredSources).mockResolvedValue([
      {
        id: "obsidian-vault-1",
        source_type: "obsidian",
        path: "/Users/test/vault-one",
        status: "Active",
        last_sync: null,
        file_count: 10,
        memory_count: 20,
      },
      {
        id: "obsidian-vault-2",
        source_type: "obsidian",
        path: "/Users/test/vault-two",
        status: "Active",
        last_sync: null,
        file_count: 5,
        memory_count: 10,
      },
    ]);

    render(
      <PageDetail
        pageId="c1"
        onBack={vi.fn()}
        onMemoryClick={vi.fn()}
      />,
      { wrapper },
    );

    await waitFor(() => {
      expect(screen.getByTitle("Export to Obsidian")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByTitle("Export to Obsidian"));

    await waitFor(() => {
      expect(screen.getByText("vault-one")).toBeInTheDocument();
      expect(screen.getByText("vault-two")).toBeInTheDocument();
    });
  });

  it("exports to selected vault from popover menu", async () => {
    vi.mocked(tauri.listRegisteredSources).mockResolvedValue([
      {
        id: "obsidian-vault-1",
        source_type: "obsidian",
        path: "/Users/test/vault-one",
        status: "Active",
        last_sync: null,
        file_count: 10,
        memory_count: 20,
      },
      {
        id: "obsidian-vault-2",
        source_type: "obsidian",
        path: "/Users/test/vault-two",
        status: "Active",
        last_sync: null,
        file_count: 5,
        memory_count: 10,
      },
    ]);
    vi.mocked(tauri.exportPageToObsidian).mockResolvedValue(
      "/Users/test/vault-two/Wenlan/pages/Test Page.md",
    );

    render(
      <PageDetail
        pageId="c1"
        onBack={vi.fn()}
        onMemoryClick={vi.fn()}
      />,
      { wrapper },
    );

    await waitFor(() => {
      expect(screen.getByTitle("Export to Obsidian")).toBeInTheDocument();
    });

    // Open popover
    fireEvent.click(screen.getByTitle("Export to Obsidian"));

    await waitFor(() => {
      expect(screen.getByText("vault-two")).toBeInTheDocument();
    });

    // Click the second vault
    fireEvent.click(screen.getByText("vault-two"));

    await waitFor(() => {
      expect(tauri.exportPageToObsidian).toHaveBeenCalledWith(
        "c1",
        "/Users/test/vault-two/Wenlan/pages",
      );
    });
  });
});
