import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ImportView, chunkImportText } from "./ImportView";

vi.mock("../../lib/tauri", () => ({
  importMemories: vi.fn(),
  getImportBatchStatus: vi.fn(),
  clipboardWrite: vi.fn(),
  IMPORT_CHUNK_SIZE: 500,
}));

import { importMemories, getImportBatchStatus } from "../../lib/tauri";

function renderImport(props = {}) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <ImportView
        onBack={vi.fn()}
        onComplete={vi.fn()}
        {...props}
      />
    </QueryClientProvider>,
  );
}

type PhaseEntry = {
  phase: string;
  state: "pending" | "running" | "complete" | "failed";
  done: number;
  total: number;
  failed?: number;
};

function makeBatch(overrides: Record<string, unknown> = {}) {
  return {
    batch_id: "batch-test-1",
    source: "chatgpt",
    started_at: 1_700_000_000,
    updated_at: 1_700_000_100,
    chunks_received: 1,
    memories_imported: 3,
    memories_skipped: 1,
    entities_detected: 4,
    entities_established: 2,
    pages_distilled: 7,
    phases: [
      { phase: "ingest", state: "complete", done: 3, total: 3, failed: 0 },
      { phase: "store", state: "complete", done: 3, total: 3, failed: 0 },
      { phase: "detect", state: "running", done: 5, total: 12, failed: 0 },
      { phase: "enrich", state: "pending", done: 0, total: 0, failed: 0 },
      { phase: "link", state: "pending", done: 0, total: 0, failed: 0 },
      { phase: "distill", state: "running", done: 3, total: 0, failed: 0 },
    ] as PhaseEntry[],
    complete: false,
    space: null,
    ...overrides,
  };
}

function chunkResult(overrides: Record<string, unknown> = {}) {
  return {
    imported: 3,
    skipped: 1,
    breakdown: { fact: 3 },
    entities_created: 0,
    observations_added: 0,
    relations_created: 0,
    batch_id: "batch-test-1",
    ...overrides,
  };
}

function startImport(text = "Memory 1") {
  const textarea = screen.getByPlaceholderText(/paste your memories/i);
  fireEvent.change(textarea, { target: { value: text } });
  fireEvent.click(screen.getByText("Import"));
}

describe("ImportView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal("crypto", { randomUUID: vi.fn(() => "batch-test-1") });
    (getImportBatchStatus as ReturnType<typeof vi.fn>).mockResolvedValue(
      makeBatch({ complete: true }),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("renders input form by default", () => {
    renderImport();
    expect(screen.getByText("Import Memories")).toBeInTheDocument();
    expect(screen.getByPlaceholderText(/paste your memories/i)).toBeInTheDocument();
    expect(screen.getByText("ChatGPT")).toBeInTheDocument();
    expect(screen.getByText("Claude")).toBeInTheDocument();
  });

  it("disables import button when textarea is empty", () => {
    renderImport();
    const button = screen.getByText("Import");
    expect(button).toBeDisabled();
  });

  it("enables import button when textarea has content", () => {
    renderImport();
    const textarea = screen.getByPlaceholderText(/paste your memories/i);
    fireEvent.change(textarea, { target: { value: "User is an engineer" } });
    const button = screen.getByText("Import");
    expect(button).not.toBeDisabled();
  });

  it("splits n memories into ceil(n/500) calls sharing one batch id", async () => {
    (importMemories as ReturnType<typeof vi.fn>).mockImplementation(
      (_source: string, content: string) => Promise.resolve(
        chunkResult({
          imported: content.split("\n").length,
          skipped: 0,
          breakdown: { fact: content.split("\n").length },
        }),
      ),
    );
    // The live status agrees with the uploads, as the daemon's would.
    (getImportBatchStatus as ReturnType<typeof vi.fn>).mockResolvedValue(
      makeBatch({ complete: true, memories_imported: 1200, memories_skipped: 0 }),
    );

    renderImport();
    const lines = Array.from({ length: 1200 }, (_, i) => `Memory ${i}`).join("\n");
    startImport(lines);

    await waitFor(() => {
      expect(importMemories).toHaveBeenCalledTimes(3);
    });
    const calls = (importMemories as ReturnType<typeof vi.fn>).mock.calls;
    expect(new Set(calls.map((c) => c[3].batchId)).size).toBe(1);
    expect(calls[0][3]).toEqual({ batchId: "batch-test-1", chunkIndex: 0, chunkTotal: 3 });
    expect(calls[1][3]).toEqual({ batchId: "batch-test-1", chunkIndex: 1, chunkTotal: 3 });
    expect(calls[2][3]).toEqual({ batchId: "batch-test-1", chunkIndex: 2, chunkTotal: 3 });
    // The summary aggregates every chunk.
    await waitFor(() => {
      expect(screen.getByText(/1200 memories imported/i)).toBeInTheDocument();
    });
  });

  it("sends a single chunk for a small import", async () => {
    (importMemories as ReturnType<typeof vi.fn>).mockResolvedValue(chunkResult());

    renderImport();
    startImport("Just one memory");

    await waitFor(() => {
      expect(importMemories).toHaveBeenCalledTimes(1);
    });
    expect(importMemories).toHaveBeenCalledWith(
      "chatgpt",
      "Just one memory",
      undefined,
      { batchId: "batch-test-1", chunkIndex: 0, chunkTotal: 1 },
    );
  });

  it("renders per-phase counts mid-flight, never a timer bar", async () => {
    let resolveImport!: (value: unknown) => void;
    (importMemories as ReturnType<typeof vi.fn>).mockImplementation(
      () => new Promise((resolve) => { resolveImport = resolve; }),
    );
    (getImportBatchStatus as ReturnType<typeof vi.fn>).mockResolvedValue(makeBatch());

    renderImport();
    startImport("Memory 1");

    // Real row counts from the daemon…
    await waitFor(() => {
      expect(screen.getByText("5 of 12")).toBeInTheDocument();
    });
    // …a phase with no rows yet reads as waiting, not 0%…
    expect(screen.getAllByText("Waiting").length).toBeGreaterThan(0);
    expect(screen.queryByText(/0%/)).not.toBeInTheDocument();
    // …and there is no elapsed-time progress anywhere.
    expect(screen.queryByText(/Processing your memories/)).not.toBeInTheDocument();

    resolveImport(chunkResult());
  });

  it("counts memories in the progress heading, not upload chunks", async () => {
    // `chunkImportText` returns chunks, so a 1,200-line paste is 3 of them.
    // Reading its length here made the heading say "3 memories".
    let resolveImport!: (value: unknown) => void;
    (importMemories as ReturnType<typeof vi.fn>).mockImplementation(
      () => new Promise((resolve) => { resolveImport = resolve; }),
    );
    (getImportBatchStatus as ReturnType<typeof vi.fn>).mockResolvedValue(makeBatch());

    renderImport();
    startImport(Array.from({ length: 1200 }, (_, i) => `Memory ${i}`).join("\n"));

    await waitFor(() => {
      expect(screen.getByText(/1,?200 memories/)).toBeInTheDocument();
    });
    expect(screen.queryByText(/^3 memories$/)).not.toBeInTheDocument();

    resolveImport(chunkResult());
  });

  it("renders a failed phase as failed", async () => {
    let resolveImport!: (value: unknown) => void;
    (importMemories as ReturnType<typeof vi.fn>).mockImplementation(
      () => new Promise((resolve) => { resolveImport = resolve; }),
    );
    const failed = makeBatch({
      phases: [
        { phase: "ingest", state: "complete", done: 3, total: 3, failed: 0 },
        { phase: "store", state: "complete", done: 3, total: 3, failed: 0 },
        { phase: "detect", state: "failed", done: 2, total: 3, failed: 1 },
        { phase: "enrich", state: "pending", done: 0, total: 0, failed: 0 },
        { phase: "link", state: "pending", done: 0, total: 0, failed: 0 },
        { phase: "distill", state: "pending", done: 0, total: 0, failed: 0 },
      ] as PhaseEntry[],
    });
    (getImportBatchStatus as ReturnType<typeof vi.fn>).mockResolvedValue(failed);

    renderImport();
    startImport("Memory 1");

    await waitFor(() => {
      expect(screen.getByTestId("import-phase-detect")).toHaveAttribute("data-state", "failed");
    });
    const row = screen.getByTestId("import-phase-detect");
    expect(within(row).getByText("Failed")).toBeInTheDocument();
    expect(within(row).getByText("1 failed")).toBeInTheDocument();

    resolveImport(chunkResult());
  });

  it("renders distill as a live count with no bar", async () => {
    let resolveImport!: (value: unknown) => void;
    (importMemories as ReturnType<typeof vi.fn>).mockImplementation(
      () => new Promise((resolve) => { resolveImport = resolve; }),
    );
    (getImportBatchStatus as ReturnType<typeof vi.fn>).mockResolvedValue(makeBatch());

    renderImport();
    startImport("Memory 1");

    await waitFor(() => {
      expect(screen.getByText("3 pages so far")).toBeInTheDocument();
    });
    expect(
      within(screen.getByTestId("import-phase-distill")).queryByRole("progressbar"),
    ).not.toBeInTheDocument();
    // …while a phase with a known total does draw a bar.
    expect(
      within(screen.getByTestId("import-phase-detect")).getByRole("progressbar"),
    ).toBeInTheDocument();

    resolveImport(chunkResult());
  });

  it("shows summary figures and names the phases still running", async () => {
    (importMemories as ReturnType<typeof vi.fn>).mockResolvedValue(chunkResult());
    (getImportBatchStatus as ReturnType<typeof vi.fn>).mockResolvedValue(makeBatch());

    renderImport();
    startImport("Memory 1\nMemory 2\nMemory 3");

    await waitFor(() => {
      expect(screen.getByText(/3 memories imported/i)).toBeInTheDocument();
    });
    expect(screen.getByText(/1 skipped/i)).toBeInTheDocument();
    expect(screen.getByText("4 detected entities")).toBeInTheDocument();
    expect(screen.getByText("2 entities established")).toBeInTheDocument();
    expect(screen.getByText("7 pages distilled")).toBeInTheDocument();
    // Honest that background work continues — no final total that is not final.
    expect(screen.getByText(/Still working:/)).toBeInTheDocument();
    expect(screen.getByText(/Detecting entities/)).toBeInTheDocument();
    expect(screen.getByText(/keep climbing/)).toBeInTheDocument();
  });

  it("says background work finished once every phase settles", async () => {
    (importMemories as ReturnType<typeof vi.fn>).mockResolvedValue(chunkResult());
    (getImportBatchStatus as ReturnType<typeof vi.fn>).mockResolvedValue(
      makeBatch({ complete: true }),
    );

    renderImport();
    startImport("Memory 1");

    await waitFor(() => {
      expect(screen.getByText("Background work finished.")).toBeInTheDocument();
    });
    expect(screen.queryByText(/Still working:/)).not.toBeInTheDocument();
  });

  it("shows type breakdown badges in summary", async () => {
    (importMemories as ReturnType<typeof vi.fn>).mockResolvedValue(
      chunkResult({ imported: 3, skipped: 0, breakdown: { identity: 1, fact: 2 } }),
    );

    renderImport();
    startImport("a\nb\nc");

    await waitFor(() => {
      expect(screen.getByText("identity")).toBeInTheDocument();
      expect(screen.getByText("fact")).toBeInTheDocument();
    });
  });

  it("shows error on import failure", async () => {
    (importMemories as ReturnType<typeof vi.fn>).mockRejectedValue("Import failed: too large");

    renderImport();
    startImport("Memory");

    await waitFor(() => {
      expect(screen.getByText(/Import failed/i)).toBeInTheDocument();
    });
    // Should go back to input form after error
    expect(screen.getByPlaceholderText(/paste your memories/i)).toBeInTheDocument();
  });

  it("calls onBack when back button clicked", () => {
    const onBack = vi.fn();
    renderImport({ onBack });
    // Back button is the first button (arrow icon, no text)
    const buttons = screen.getAllByRole("button");
    fireEvent.click(buttons[0]);
    expect(onBack).toHaveBeenCalled();
  });

  it("calls onComplete when View memories clicked", async () => {
    const onComplete = vi.fn();
    (importMemories as ReturnType<typeof vi.fn>).mockResolvedValue(chunkResult());

    renderImport({ onComplete });
    startImport("Memory");

    await waitFor(() => screen.getByText("View memories"));
    fireEvent.click(screen.getByText("View memories"));
    expect(onComplete).toHaveBeenCalledWith("chatgpt", expect.objectContaining({ imported: expect.any(Number) }));
  });

  it("resets to input form when Import more clicked", async () => {
    (importMemories as ReturnType<typeof vi.fn>).mockResolvedValue(chunkResult());

    renderImport();
    startImport("Memory");

    await waitFor(() => screen.getByText("Import more"));
    fireEvent.click(screen.getByText("Import more"));

    // Should be back to input form with empty textarea
    expect(screen.getByText("Import Memories")).toBeInTheDocument();
    expect(screen.getByPlaceholderText(/paste your memories/i)).toHaveValue("");
  });

  it("switches source and shows correct help text", () => {
    renderImport();

    // Default is ChatGPT — shows export prompt with ChatGPT instruction
    expect(screen.getByText(/paste into ChatGPT/i)).toBeInTheDocument();

    // Switch to Claude
    fireEvent.click(screen.getByText("Claude"));
    expect(screen.getByText(/paste into Claude/i)).toBeInTheDocument();

    // Switch to Other
    fireEvent.click(screen.getByText("Other"));
    expect(screen.getByText(/Paste any list/)).toBeInTheDocument();
  });

  it("passes selected source to importMemories", async () => {
    (importMemories as ReturnType<typeof vi.fn>).mockResolvedValue(chunkResult());

    renderImport();
    fireEvent.click(screen.getByText("Claude"));
    startImport("Memory");

    await waitFor(() => {
      expect(importMemories).toHaveBeenCalledWith(
        "claude",
        "Memory",
        undefined,
        { batchId: "batch-test-1", chunkIndex: 0, chunkTotal: 1 },
      );
    });
  });
});

describe("chunkImportText", () => {
  it("returns one chunk for a small import", () => {
    expect(chunkImportText("a\nb\nc")).toEqual(["a\nb\nc"]);
  });

  it("skips empty lines", () => {
    expect(chunkImportText("a\n\n  \nb")).toEqual(["a\nb"]);
  });

  it("splits at the chunk boundary", () => {
    const lines = Array.from({ length: 1200 }, (_, i) => `m${i}`);
    const chunks = chunkImportText(lines.join("\n"));
    expect(chunks).toHaveLength(3);
    expect(chunks[0]!.split("\n")).toHaveLength(500);
    expect(chunks[2]!.split("\n")).toHaveLength(200);
  });
});
