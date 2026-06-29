// SPDX-License-Identifier: AGPL-3.0-only
import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

vi.mock("../../lib/tauri", () => ({
  getSpace: vi.fn(),
  listMemoriesRich: vi.fn(),
  listEntities: vi.fn(),
  listConcepts: vi.fn().mockResolvedValue([]),
  getNurtureCards: vi.fn().mockResolvedValue([]),
  setStability: vi.fn(),
  updateSpace: vi.fn(),
  deleteSpace: vi.fn(),
  confirmSpace: vi.fn(),
  updateMemory: vi.fn(),
  deleteFileChunks: vi.fn(),
  getVersionChain: vi.fn().mockResolvedValue([]),
  FACET_COLORS: {},
  STABILITY_TIERS: {},
  getPendingRevision: vi.fn().mockResolvedValue(null),
  acceptPendingRevision: vi.fn(),
  dismissPendingRevision: vi.fn(),
}));

import {
  deleteSpace,
  getSpace,
  listMemoriesRich,
  listEntities,
} from "../../lib/tauri";
import SpaceDetail from "./SpaceDetail";

const mockGetSpace = vi.mocked(getSpace);
const mockListMemoriesRich = vi.mocked(listMemoriesRich);
const mockListEntities = vi.mocked(listEntities);
const mockDeleteSpace = vi.mocked(deleteSpace);

function renderWithQuery(ui: React.ReactElement) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return render(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

const baseSpace = {
  id: "s1",
  name: "Origin",
  description: null,
  suggested: false,
  starred: false,
  sort_order: 0,
  memory_count: 47,
  entity_count: 12,
  created_at: 1000,
  updated_at: 2000,
};

const confirmedMemories = [
  {
    source_id: "m1",
    title: "Prefers TDD workflow",
    content: "TDD workflow details",
    summary: null,
    memory_type: "preference",
    domain: "origin",
    source_agent: "claude-code",
    confidence: 0.9,
    confirmed: true,
    pinned: false,
    supersedes: null,
    last_modified: 1000,
    chunk_count: 1,
    access_count: 5,
    is_recap: false,
  },
  {
    source_id: "m2",
    title: "Uses Rust for backend",
    content: "Rust backend details",
    summary: null,
    memory_type: "fact",
    domain: "origin",
    source_agent: "claude-code",
    confidence: 0.8,
    confirmed: true,
    pinned: false,
    supersedes: null,
    last_modified: 2000,
    chunk_count: 1,
    access_count: 3,
    is_recap: false,
  },
];

describe("SpaceDetail context header", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetSpace.mockResolvedValue(baseSpace);
    mockListMemoriesRich.mockResolvedValue(confirmedMemories);
    mockListEntities.mockResolvedValue([]);
  });

  it("does not render context section", async () => {
    renderWithQuery(
      <SpaceDetail
        spaceName="Origin"
        onBack={() => {}}
        onSelectMemory={() => {}}
        onSelectPage={() => {}}
        onEntityClick={() => {}}
      />,
    );

    // Wait for component to load
    await screen.findByText("47 memories");
    // Context section should not exist
    expect(screen.queryByText("Context")).not.toBeInTheDocument();
  });

  it("groups edit and delete buttons together", async () => {
    renderWithQuery(
      <SpaceDetail
        spaceName="Origin"
        onBack={() => {}}
        onSelectMemory={() => {}}
        onSelectPage={() => {}}
        onEntityClick={() => {}}
      />,
    );

    const editBtn = await screen.findByTitle("Edit description");
    const deleteBtn = screen.getByTitle("Delete space");
    // Both buttons share the same parent container
    expect(editBtn.parentElement).toBe(deleteBtn.parentElement);
  });

  it("shows memory count", async () => {
    renderWithQuery(
      <SpaceDetail
        spaceName="Origin"
        onBack={() => {}}
        onSelectMemory={() => {}}
        onSelectPage={() => {}}
        onEntityClick={() => {}}
      />,
    );

    expect(
      await screen.findByText("47 memories"),
    ).toBeInTheDocument();
  });

  it("shows entity count when present", async () => {
    renderWithQuery(
      <SpaceDetail
        spaceName="Origin"
        onBack={() => {}}
        onSelectMemory={() => {}}
        onSelectPage={() => {}}
        onEntityClick={() => {}}
      />,
    );

    expect(
      await screen.findByText("12 entities"),
    ).toBeInTheDocument();
  });

  it("deletes a space with only the daemon-supported keep behavior", async () => {
    renderWithQuery(
      <SpaceDetail
        spaceName="Origin"
        onBack={() => {}}
        onSelectMemory={() => {}}
        onSelectPage={() => {}}
        onEntityClick={() => {}}
      />,
    );

    fireEvent.click(await screen.findByTitle("Delete space"));

    expect(screen.queryByText("Delete")).not.toBeInTheDocument();
    expect(screen.queryByText(/Keep 47 memories/)).not.toBeInTheDocument();
    expect(
      screen.getByText(/Memories keep their current space tag/),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Confirm delete space" }));

    await waitFor(() => {
      const calls = mockDeleteSpace.mock.calls;
      expect(calls[calls.length - 1]).toEqual(["Origin"]);
    });
  });

  it("discards a suggested space without unsupported memoryAction arguments", async () => {
    mockGetSpace.mockResolvedValue({ ...baseSpace, suggested: true });

    renderWithQuery(
      <SpaceDetail
        spaceName="Origin"
        onBack={() => {}}
        onSelectMemory={() => {}}
        onSelectPage={() => {}}
        onEntityClick={() => {}}
      />,
    );

    fireEvent.click(await screen.findByText("Discard"));

    await waitFor(() => {
      const calls = mockDeleteSpace.mock.calls;
      expect(calls[calls.length - 1]).toEqual(["Origin"]);
    });
  });
});
