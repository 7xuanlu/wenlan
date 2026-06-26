// SPDX-License-Identifier: AGPL-3.0-only
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import MemoryDetail from "./MemoryDetail";
import * as tauri from "../../lib/tauri";

vi.mock("../../lib/tauri");

const memory: tauri.MemoryItem = {
  source_id: "mem-1",
  title: "Memory",
  content: "A memory",
  summary: null,
  memory_type: "fact",
  domain: null,
  source_agent: null,
  confidence: null,
  confirmed: false,
  pinned: false,
  supersedes: null,
  last_modified: 1,
  chunk_count: 1,
};

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

describe("MemoryDetail enrichment status", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauri.getMemoryDetail).mockResolvedValue(memory);
    vi.mocked(tauri.listSpaces).mockResolvedValue([]);
    vi.mocked(tauri.listEntities).mockResolvedValue([]);
    vi.mocked(tauri.listAllTags).mockResolvedValue({
      tags: [],
      document_tags: {},
      categories: [],
      document_categories: {},
    });
    vi.mocked(tauri.search).mockResolvedValue([]);
    vi.mocked(tauri.getEnrichmentStatus).mockRejectedValue(new Error("old daemon"));
    vi.mocked(tauri.getVersionChain).mockResolvedValue([]);
    vi.mocked(tauri.getPendingRevision).mockResolvedValue(null);
    vi.mocked(tauri.acceptPendingRevision).mockResolvedValue({
      target_source_id: "mem-1",
      revision_source_id: "rev-1",
      wrote: true,
    });
    vi.mocked(tauri.getMemoryRevisions).mockResolvedValue({
      current_source_id: "mem-1",
      chain_depth: 1,
      entries: [],
    });
  });

  it("shows daemon enrichment status without blocking the memory body", async () => {
    vi.mocked(tauri.getEnrichmentStatus).mockResolvedValue({
      source_id: "mem-1",
      summary: "complete",
      steps: [{ step: "classify", status: "done", error: null, attempts: 1 }],
    });

    render(
      <MemoryDetail
        sourceId="mem-1"
        onBack={vi.fn()}
        onNavigateEntity={vi.fn()}
        onNavigateMemory={vi.fn()}
      />,
      { wrapper },
    );

    expect(await screen.findByText("A memory")).toBeInTheDocument();
    await waitFor(() => {
      expect(screen.getByText(/enrichment/i)).toBeInTheDocument();
      expect(screen.getByText(/complete/i)).toBeInTheDocument();
    });
  });

  it("keeps rendering the memory when enrichment status route is unavailable", async () => {
    vi.mocked(tauri.getEnrichmentStatus).mockRejectedValue(new Error("404"));

    render(
      <MemoryDetail
        sourceId="mem-1"
        onBack={vi.fn()}
        onNavigateEntity={vi.fn()}
        onNavigateMemory={vi.fn()}
      />,
      { wrapper },
    );

    expect(await screen.findByText("A memory")).toBeInTheDocument();
    expect(screen.queryByText(/enrichment/i)).toBeNull();
  });

  it("renders daemon memory revision history", async () => {
    vi.mocked(tauri.getEnrichmentStatus).mockRejectedValue(new Error("old daemon"));
    vi.mocked(tauri.getMemoryRevisions).mockResolvedValue({
      current_source_id: "mem-1",
      chain_depth: 2,
      entries: [
        {
          source_id: "mem-1",
          depth: 0,
          title: "Current",
          content_preview: "Current version",
          last_modified: 10,
          source_agent: "claude-code",
          supersede_mode: "protected_revision",
          delta_summary: "Clarified wording",
        },
      ],
    });

    render(
      <MemoryDetail
        sourceId="mem-1"
        onBack={vi.fn()}
        onNavigateEntity={vi.fn()}
        onNavigateMemory={vi.fn()}
      />,
      { wrapper },
    );

    expect(await screen.findByText(/revision history/i)).toBeInTheDocument();
    expect(screen.getByText(/clarified wording/i)).toBeInTheDocument();
  });

  it("keeps rendering the memory when memory revisions route is unavailable", async () => {
    vi.mocked(tauri.getEnrichmentStatus).mockRejectedValue(new Error("old daemon"));
    vi.mocked(tauri.getMemoryRevisions).mockRejectedValue(new Error("404"));

    render(
      <MemoryDetail
        sourceId="mem-1"
        onBack={vi.fn()}
        onNavigateEntity={vi.fn()}
        onNavigateMemory={vi.fn()}
      />,
      { wrapper },
    );

    expect(await screen.findByText("A memory")).toBeInTheDocument();
    expect(screen.queryByText(/revision history/i)).toBeNull();
  });

  it("refreshes daemon memory revision history after accepting a pending revision", async () => {
    const user = userEvent.setup();
    vi.mocked(tauri.getMemoryDetail).mockResolvedValue({
      ...memory,
      memory_type: "identity",
      confirmed: true,
    });
    vi.mocked(tauri.getPendingRevision).mockResolvedValue({
      source_id: "rev-1",
      content: "Proposed protected update",
      source_agent: "claude-code",
    });

    render(
      <MemoryDetail
        sourceId="mem-1"
        onBack={vi.fn()}
        onNavigateEntity={vi.fn()}
        onNavigateMemory={vi.fn()}
      />,
      { wrapper },
    );

    expect(await screen.findByText(/proposed protected update/i)).toBeInTheDocument();
    expect(tauri.getMemoryRevisions).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole("button", { name: /accept/i }));

    await waitFor(() => {
      expect(tauri.acceptPendingRevision).toHaveBeenCalledWith("mem-1");
      expect(tauri.getMemoryRevisions).toHaveBeenCalledTimes(2);
    });
  });

  it("uses daemon memory revision history instead of the legacy version panel", async () => {
    vi.mocked(tauri.getMemoryDetail).mockResolvedValue({
      ...memory,
      supersedes: "mem-old",
    });
    vi.mocked(tauri.getVersionChain).mockResolvedValue([
      {
        source_id: "mem-old",
        title: "Legacy version",
        content: "Old content",
        memory_type: "fact",
        confirmed: true,
        supersedes: null,
        last_modified: 5,
      },
    ]);
    vi.mocked(tauri.getMemoryRevisions).mockResolvedValue({
      current_source_id: "mem-1",
      chain_depth: 2,
      entries: [
        {
          source_id: "mem-1",
          depth: 0,
          title: "Current",
          content_preview: "Current version",
          last_modified: 10,
          delta_summary: "Clarified wording",
        },
      ],
    });

    render(
      <MemoryDetail
        sourceId="mem-1"
        onBack={vi.fn()}
        onNavigateEntity={vi.fn()}
        onNavigateMemory={vi.fn()}
      />,
      { wrapper },
    );

    expect(await screen.findByText(/revision history/i)).toBeInTheDocument();
    expect(screen.getByText(/clarified wording/i)).toBeInTheDocument();
    await waitFor(() => {
      expect(tauri.getVersionChain).toHaveBeenCalledWith("mem-1");
    });
    expect(screen.queryByText(/version history/i)).toBeNull();
    expect(screen.queryByText(/legacy version/i)).toBeNull();
  });
});
