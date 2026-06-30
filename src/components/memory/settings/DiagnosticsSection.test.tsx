// SPDX-License-Identifier: AGPL-3.0-only
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import DiagnosticsSection from "./DiagnosticsSection";
import { getPipelineStatus } from "../../../lib/tauri";

vi.mock("../../../lib/tauri", () => ({
  getPipelineStatus: vi.fn(),
}));

function renderDiagnostics() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  }
  return render(<DiagnosticsSection />, { wrapper: Wrapper });
}

describe("DiagnosticsSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(getPipelineStatus).mockResolvedValue({
      enrichment: { classified: 9, raw: 2 },
      entity_linking: { linked: 7, unlinked: 3 },
      refinement_queue: [{ action: "merge", status: "pending", count: 4 }],
      recaps: 5,
      types: { fact: 6, preference: 1 },
      quality: { trusted: 8, low: 1 },
    });
  });

  it("renders the pipeline snapshot fields", async () => {
    renderDiagnostics();

    expect(await screen.findByText("Pipeline Snapshot")).toBeInTheDocument();
    expect(await screen.findByText("classified")).toBeInTheDocument();
    expect(screen.getByText("9")).toBeInTheDocument();
    expect(screen.getByText("Entity linking")).toBeInTheDocument();
    expect(screen.getByText("70% linked")).toBeInTheDocument();
    expect(screen.getByText("Refinery queue")).toBeInTheDocument();
    expect(screen.getByText("merge")).toBeInTheDocument();
    expect(screen.getByText("pending")).toBeInTheDocument();
    expect(screen.getByText("Recaps")).toBeInTheDocument();
    expect(screen.getByText("5")).toBeInTheDocument();
    expect(screen.getByText("fact")).toBeInTheDocument();
    expect(screen.getByText("trusted")).toBeInTheDocument();
  });

  it("renders the refinery queue empty state", async () => {
    vi.mocked(getPipelineStatus).mockResolvedValue({
      enrichment: {},
      entity_linking: { linked: 0, unlinked: 0 },
      refinement_queue: [],
      recaps: 0,
      types: {},
      quality: {},
    });

    renderDiagnostics();

    expect(await screen.findByText("Refinery queue")).toBeInTheDocument();
    expect(screen.getByText("No pending refinery work.")).toBeInTheDocument();
  });

  it("shows a scoped old-daemon message when the route is missing", async () => {
    vi.mocked(getPipelineStatus).mockRejectedValue(
      new Error("HTTP GET /api/debug/pipeline returned 404: not found"),
    );

    renderDiagnostics();

    expect(await screen.findByText("Diagnostics require a newer daemon")).toBeInTheDocument();
    expect(screen.queryByText("Run maintenance")).not.toBeInTheDocument();
  });

  it("does not expose the manual steep maintenance action", async () => {
    renderDiagnostics();

    await waitFor(() => expect(getPipelineStatus).toHaveBeenCalled());
    expect(screen.queryByText("Run maintenance")).not.toBeInTheDocument();
    expect(screen.queryByText("Steep")).not.toBeInTheDocument();
  });
});
