// SPDX-License-Identifier: AGPL-3.0-only
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, within, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { FirstUseGuide } from "../FirstUseGuide";
import type { ImportBatchStatus, ImportPhase, Page } from "../../../lib/tauri";

vi.mock("../../../lib/tauri", () => ({
  getActiveImportBatches: vi.fn(),
  getResolvedRouting: vi.fn(),
  getImportBatchStatus: vi.fn(),
  listPages: vi.fn(),
  clipboardWrite: vi.fn(),
}));

import {
  getActiveImportBatches,
  getImportBatchStatus,
  getResolvedRouting,
  listPages,
} from "../../../lib/tauri";

const mockedBatches = vi.mocked(getActiveImportBatches);
const mockedBatchStatus = vi.mocked(getImportBatchStatus);
const mockedListPages = vi.mocked(listPages);

function entry(
  phase: ImportPhase,
  state: "pending" | "running" | "complete" | "failed",
  done: number,
  total: number,
  failed = 0,
) {
  return { phase, state, done, total, failed };
}

function makeBatch(overrides: Partial<ImportBatchStatus> = {}): ImportBatchStatus {
  return {
    batch_id: "batch-1",
    source: "chatgpt",
    started_at: 1_700_000_000,
    updated_at: 1_700_000_100,
    chunks_received: 1,
    memories_imported: 4,
    memories_skipped: 1,
    entities_detected: 2,
    entities_established: 1,
    pages_distilled: 0,
    phases: [
      entry("ingest", "complete", 4, 4),
      entry("store", "complete", 4, 4),
      entry("detect", "running", 2, 4),
      entry("enrich", "pending", 0, 0),
      entry("link", "pending", 0, 0),
      entry("distill", "pending", 0, 0),
    ],
    complete: false,
    space: null,
    ...overrides,
  } as ImportBatchStatus;
}

function makePage(overrides: Partial<Page> = {}): Page {
  return {
    id: "page-real-1",
    title: "A real knowledge page",
    summary: null,
    content: "body",
    entity_id: null,
    domain: null,
    space: null,
    source_memory_ids: [],
    version: 1,
    status: "active",
    creation_kind: "distilled",
    review_status: null,
    created_at: "2026-01-01",
    last_compiled: "2026-01-02",
    last_modified: "2026-01-02",
    ...overrides,
  } as Page;
}

function renderGuide(props = {}) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <FirstUseGuide
        onBack={vi.fn()}
        onImport={vi.fn()}
        onSources={vi.fn()}
        onConnect={vi.fn()}
        onOpenIntelligence={vi.fn()}
        onOpenPage={vi.fn()}
        {...props}
      />
    </QueryClientProvider>,
  );
}

function enterLive() {
  fireEvent.click(screen.getByRole("button", { name: "See my knowledge" }));
}

beforeEach(() => {
  vi.clearAllMocks();
  mockedBatchStatus.mockReset();
  vi.mocked(getResolvedRouting).mockResolvedValue({ everyday: { source: "on_device", model: "qwen3-4b", mode: "pinned", pin: "on_device" }, synthesis: { source: "on_device", model: "qwen3-4b", mode: "pinned", pin: "on_device" }, pool: { anthropic: { configured: false, everyday_model: null, synthesis_model: null }, external: null, on_device: { selected: "qwen3-4b", loaded: true } } });
  mockedBatches.mockResolvedValue({ batches: [] });
  mockedListPages.mockResolvedValue([]);
});

afterEach(() => {
  vi.useRealTimers();
});

describe("FirstUseGuide navigation", () => {
  it("stays query-silent until the live view is entered", async () => {
    renderGuide();
    expect(mockedListPages).not.toHaveBeenCalled();
    expect(mockedBatches).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Try an example" }));
    expect(screen.getByTestId("first-use-sample")).toBeInTheDocument();
    expect(mockedListPages).not.toHaveBeenCalled();
    expect(mockedBatches).not.toHaveBeenCalled();
  });

  it("routes the own-data chooser to the real import, sources, and connect targets", () => {
    const onImport = vi.fn();
    const onSources = vi.fn();
    const onConnect = vi.fn();
    renderGuide({ onImport, onSources, onConnect });
    fireEvent.click(screen.getByRole("button", { name: "Bring my data" }));
    fireEvent.click(screen.getByRole("button", { name: "Open import" }));
    fireEvent.click(screen.getByRole("button", { name: "Open sources" }));
    fireEvent.click(screen.getByRole("button", { name: "Set up a tool" }));
    expect(onImport).toHaveBeenCalledTimes(1);
    expect(onSources).toHaveBeenCalledTimes(1);
    expect(onConnect).toHaveBeenCalledTimes(1);
  });
});

describe("FirstUseGuide live view", () => {
  it("shows the empty state with real actions when the library is empty", async () => {
    const onImport = vi.fn();
    const onOpenIntelligence = vi.fn();
    renderGuide({ onImport, onOpenIntelligence });
    enterLive();
    expect(await screen.findByText("Nothing here yet")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Import memories" }));
    fireEvent.click(screen.getByRole("button", { name: "Set up intelligence" }));
    expect(onImport).toHaveBeenCalledTimes(1);
    expect(onOpenIntelligence).toHaveBeenCalledTimes(1);
  });

  it("reports a page-list failure as failure with retry, not as empty success", async () => {
    mockedListPages.mockRejectedValueOnce(new Error("down"));
    renderGuide();
    enterLive();
    expect(
      await screen.findByText("Wenlan couldn't read this right now."),
    ).toBeInTheDocument();
    expect(screen.queryByText("Nothing here yet")).not.toBeInTheDocument();
    mockedListPages.mockResolvedValue([makePage()]);
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("Your knowledge pages")).toBeInTheDocument();
  });

  it("reports an active-batch failure as failure, not as progress", async () => {
    mockedBatches.mockRejectedValueOnce(new Error("down"));
    mockedListPages.mockResolvedValue([makePage()]);
    renderGuide();
    enterLive();
    expect(
      await screen.findByText("Wenlan couldn't read this right now."),
    ).toBeInTheDocument();
    expect(screen.queryByText("Import progress")).not.toBeInTheDocument();
  });

  it("shows a completed import returned by its status endpoint after active polling drops it", async () => {
    mockedBatchStatus.mockResolvedValue(makeBatch({
      complete: true,
      phases: [
        entry("ingest", "complete", 4, 4),
        entry("store", "complete", 4, 4),
        entry("detect", "complete", 4, 4),
        entry("enrich", "complete", 4, 4),
        entry("link", "complete", 4, 4),
        entry("distill", "complete", 0, 0),
      ],
    }));
    mockedBatches.mockResolvedValue({ batches: [] });
    renderGuide({ initialView: "live", batchId: "batch-1" });
    expect(await screen.findByText("Import progress")).toBeInTheDocument();
    expect(screen.getByText("Background work finished.")).toBeInTheDocument();
    expect(screen.getByText(/No knowledge page was created in this batch/)).toBeInTheDocument();
    expect(mockedBatchStatus).toHaveBeenCalledWith("batch-1");
  });

  it("keeps the targeted batch through completion, then stops its terminal poll", async () => {
    vi.useFakeTimers();
    mockedBatches.mockResolvedValue({ batches: [] });
    mockedBatchStatus
      .mockResolvedValueOnce(makeBatch())
      .mockResolvedValue(makeBatch({
        complete: true,
        phases: [
          entry("ingest", "complete", 4, 4),
          entry("store", "complete", 4, 4),
          entry("detect", "complete", 4, 4),
          entry("enrich", "complete", 4, 4),
          entry("link", "complete", 4, 4),
          entry("distill", "complete", 0, 0),
        ],
      }));
    renderGuide({ initialView: "live", batchId: "batch-1" });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(screen.getByTestId("import-phase-detect")).toHaveAttribute("data-state", "running");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(5_000);
    });
    await act(async () => {});
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(screen.getByText("Background work finished.")).toBeInTheDocument();
    const settledCalls = mockedBatchStatus.mock.calls.length;
    await act(async () => {
      await vi.advanceTimersByTimeAsync(15_000);
    });
    expect(mockedBatchStatus).toHaveBeenCalledTimes(settledCalls);
  });

  it("renders observed running phases without inventing a finished result", async () => {
    mockedBatches.mockResolvedValue({ batches: [makeBatch()] });
    renderGuide();
    enterLive();
    expect(await screen.findByText("Import progress")).toBeInTheDocument();
    expect(screen.getByTestId("import-phase-detect")).toHaveAttribute(
      "data-state",
      "running",
    );
    expect(
      screen.queryByText(/No knowledge page was created in this batch/),
    ).not.toBeInTheDocument();
    expect(screen.queryByText("Background work finished.")).not.toBeInTheDocument();
  });

  it("names every source when summarizing concurrent import batches", async () => {
    mockedBatches.mockResolvedValue({ batches: [
      makeBatch(), makeBatch({ batch_id: "batch-2", source: "claude" }),
    ] });
    renderGuide();
    enterLive();
    expect(await screen.findByText(/8 memories imported from ChatGPT · Claude/)).toBeVisible();
  });

  it("shows failed status on a settled batch with failures and zero pages", async () => {
    const onOpenIntelligence = vi.fn();
    mockedBatches.mockResolvedValue({ batches: [] });
    mockedBatchStatus.mockResolvedValue(makeBatch({
      complete: true,
      phases: [
        entry("ingest", "complete", 4, 4),
        entry("store", "complete", 4, 4),
        entry("detect", "failed", 2, 4, 2),
        entry("enrich", "complete", 4, 4),
        entry("link", "complete", 4, 4),
        entry("distill", "complete", 0, 0),
      ],
    }));
    renderGuide({ onOpenIntelligence, initialView: "live", batchId: "batch-1" });
    expect(await screen.findByText("Import progress")).toBeInTheDocument();
    expect(
      await screen.findByText(/Some items could not be processed/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/No knowledge page was created in this batch/),
    ).toBeInTheDocument();
    expect(screen.getByTestId("import-phase-detect")).toHaveAttribute(
      "data-state",
      "failed",
    );
    fireEvent.click(screen.getByRole("button", { name: "Set up intelligence" }));
    expect(onOpenIntelligence).toHaveBeenCalledOnce();
  });

  it("does not double-count a targeted batch that is also in the active list", async () => {
    const batch = makeBatch();
    mockedBatches.mockResolvedValue({ batches: [batch] });
    mockedBatchStatus.mockResolvedValue(batch);
    renderGuide({ initialView: "live", batchId: "batch-1" });
    expect(await screen.findByText("Import progress")).toBeInTheDocument();
    expect(screen.getByText(/4 memories imported from ChatGPT/)).toBeInTheDocument();
    expect(screen.queryByText(/8 memories imported from ChatGPT/)).not.toBeInTheDocument();
  });

  it("offers real knowledge pages and hides entity shadow pages", async () => {
    const onOpenPage = vi.fn();
    mockedListPages.mockResolvedValue([
      makePage({
        id: "shadow-1",
        title: "Shadow entity page",
        creation_kind: "entity",
      }),
      makePage({ id: "page-real-7", title: "Weekend in Tainan" }),
    ]);
    renderGuide({ onOpenPage });
    enterLive();
    expect(await screen.findByText("Your knowledge pages")).toBeInTheDocument();
    expect(screen.queryByText("Shadow entity page")).not.toBeInTheDocument();
    const live = screen.getByTestId("first-use-live");
    fireEvent.click(within(live).getByText("Weekend in Tainan"));
    expect(onOpenPage).toHaveBeenCalledTimes(1);
    expect(onOpenPage).toHaveBeenCalledWith("page-real-7");
  });

  it("enters the live view directly when initialView is live", async () => {
    renderGuide({ initialView: "live" });
    expect(screen.getByTestId("first-use-live")).toBeInTheDocument();
    expect(await screen.findByText("Nothing here yet")).toBeInTheDocument();
  });

  it("uses the active endpoint for a direct live view without a targeted batch", async () => {
    renderGuide({ initialView: "live" });
    expect(await screen.findByText("Nothing here yet")).toBeInTheDocument();
    expect(mockedBatchStatus).not.toHaveBeenCalled();
    expect(mockedBatches).toHaveBeenCalled();
  });

  it("keeps polling pages and batches while the live view is mounted", async () => {
    vi.useFakeTimers();
    renderGuide();
    enterLive();
    await act(async () => {});
    expect(mockedListPages.mock.calls.length).toBeGreaterThanOrEqual(1);
    expect(mockedBatches.mock.calls.length).toBeGreaterThanOrEqual(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(30_000);
    });
    // Pages re-read on their own cadence so a newly distilled page appears
    // without a manual refresh; batches keep their faster poll.
    expect(mockedListPages.mock.calls.length).toBeGreaterThanOrEqual(2);
    expect(mockedBatches.mock.calls.length).toBeGreaterThanOrEqual(2);
  });
});


describe("live model availability", () => {
  it("shows a setup action instead of promising progress when imported data has no AI route", async () => {
    vi.mocked(getResolvedRouting).mockResolvedValue({
      everyday: { source: "basic", model: null, mode: "unconfigured", pin: null },
      synthesis: { source: "none", model: null, mode: "unconfigured", pin: null },
      pool: { anthropic: { configured: false, everyday_model: null, synthesis_model: null }, external: null, on_device: null },
    });
    mockedBatches.mockResolvedValue({ batches: [makeBatch()] });
    const onOpenIntelligence = vi.fn();
    renderGuide({ initialView: "live", onOpenIntelligence });
    expect(await screen.findByText("Your data is saved. Set up an available AI model to continue organizing it into pages.")).toBeInTheDocument();
    expect(screen.queryByText(/Your data is queued for background work/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Set up intelligence" }));
    expect(onOpenIntelligence).toHaveBeenCalledOnce();
  });
});
