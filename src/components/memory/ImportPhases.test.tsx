import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, act, within } from "@testing-library/react";
import {
  ImportDetailPanel,
  ImportPhaseList,
  ImportStatusPill,
  summarizeImportBatches,
  useImportBatchStatus,
} from "./ImportPhases";
import type { ImportBatchStatus, ImportPhase } from "../../lib/tauri";

vi.mock("../../lib/tauri", () => ({
  getImportBatchStatus: vi.fn(),
}));

import { getImportBatchStatus } from "../../lib/tauri";

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
    memories_imported: 10,
    memories_skipped: 2,
    entities_detected: 4,
    entities_established: 1,
    pages_distilled: 3,
    phases: [
      entry("ingest", "complete", 10, 10),
      entry("store", "complete", 10, 10),
      entry("detect", "running", 5, 12),
      entry("enrich", "pending", 0, 0),
      entry("link", "pending", 0, 0),
      entry("distill", "running", 3, 0),
    ],
    complete: false,
    space: null,
    ...overrides,
  } as ImportBatchStatus;
}

function Probe({ batchId }: { batchId: string | null }) {
  const status = useImportBatchStatus(batchId);
  return <div data-testid="probe">{status ? status.batch_id : "none"}</div>;
}

describe("summarizeImportBatches", () => {
  it("adds counts across batches and keeps one pill", () => {
    const summary = summarizeImportBatches([
      makeBatch({ memories_imported: 10, pages_distilled: 3 }),
      makeBatch({ batch_id: "batch-2", memories_imported: 5, pages_distilled: 1 }),
    ]);
    expect(summary.memoriesImported).toBe(15);
    expect(summary.pagesDistilled).toBe(4);
    expect(summary.complete).toBe(false);
    expect(summary.runningPhases).toContain("detect");
  });

  it("reads failed while any batch reports a failed phase", () => {
    const failed = makeBatch({
      phases: [entry("detect", "failed", 2, 3, 1)],
    });
    const summary = summarizeImportBatches([makeBatch(), failed]);
    expect(summary.failedPhases).toEqual(["detect"]);
    const detect = summary.phases.find((p) => p.phase === "detect")!;
    expect(detect.state).toBe("failed");
    expect(detect.failed).toBe(1);
  });

  it("is complete only when every batch settles every phase", () => {
    const done = makeBatch({
      complete: true,
      phases: [
        entry("ingest", "complete", 10, 10),
        entry("store", "complete", 10, 10),
        entry("detect", "complete", 12, 12),
        entry("enrich", "complete", 10, 10),
        entry("link", "complete", 10, 10),
        entry("distill", "complete", 3, 0),
      ],
    });
    expect(summarizeImportBatches([done]).complete).toBe(true);
    expect(summarizeImportBatches([done, makeBatch()]).complete).toBe(false);
  });
});

describe("ImportPhaseList", () => {
  it("renders all six phases with real counts", () => {
    render(<ImportPhaseList phases={makeBatch().phases} />);
    expect(screen.getByText("Receiving memories")).toBeInTheDocument();
    expect(screen.getByText("Storing memories")).toBeInTheDocument();
    expect(screen.getByText("Detecting entities")).toBeInTheDocument();
    expect(screen.getByText("Enriching memories")).toBeInTheDocument();
    expect(screen.getByText("Linking memories")).toBeInTheDocument();
    expect(screen.getByText("Distilling pages")).toBeInTheDocument();
    expect(screen.getByText("5 of 12")).toBeInTheDocument();
  });

  it("reads a phase with no rows as waiting, never 0%", () => {
    render(<ImportPhaseList phases={[]} />);
    expect(screen.getAllByText("Waiting").length).toBeGreaterThan(0);
    expect(screen.queryByText(/0%/)).not.toBeInTheDocument();
  });

  it("renders distill as a count with no bar", () => {
    render(<ImportPhaseList phases={makeBatch().phases} />);
    expect(screen.getByText("3 pages so far")).toBeInTheDocument();
    expect(
      within(screen.getByTestId("import-phase-distill")).queryByRole("progressbar"),
    ).not.toBeInTheDocument();
  });
});

describe("ImportStatusPill", () => {
  it("renders nothing when no batch is active", () => {
    const { container } = render(<ImportStatusPill batches={[]} onOpen={vi.fn()} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("summarises the running phase and opens the panel on click", () => {
    const onOpen = vi.fn();
    render(<ImportStatusPill batches={[makeBatch()]} onOpen={onOpen} />);
    const pill = screen.getByTestId("import-status-pill");
    expect(pill).toHaveTextContent("Importing memories…");
    expect(pill).toHaveTextContent("Detecting entities");
    fireEvent.click(pill);
    expect(onOpen).toHaveBeenCalledTimes(1);
  });

  it("flags a failed batch instead of claiming progress", () => {
    render(
      <ImportStatusPill
        batches={[makeBatch({ phases: [entry("detect", "failed", 2, 3, 1)] })]}
        onOpen={vi.fn()}
      />,
    );
    expect(screen.getByTestId("import-status-pill")).toHaveTextContent("Import needs attention");
  });
});

describe("ImportDetailPanel", () => {
  it("shows per-phase progress with a Back control", () => {
    const onBack = vi.fn();
    render(<ImportDetailPanel batches={[makeBatch()]} onBack={onBack} />);
    expect(screen.getByTestId("import-detail-panel")).toBeInTheDocument();
    expect(screen.getByText("Import progress")).toBeInTheDocument();
    expect(screen.getByText("5 of 12")).toBeInTheDocument();
    expect(screen.getByText("3 pages so far")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("import-detail-back"));
    expect(onBack).toHaveBeenCalledTimes(1);
  });
});

describe("useImportBatchStatus", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("polls immediately, keeps polling, and stops on complete", async () => {
    (getImportBatchStatus as ReturnType<typeof vi.fn>).mockResolvedValue(makeBatch());

    render(<Probe batchId="batch-1" />);
    // Immediate read, no interval tick needed.
    expect(getImportBatchStatus).toHaveBeenCalledTimes(1);
    expect(getImportBatchStatus).toHaveBeenCalledWith("batch-1");
    await act(async () => {});
    expect(screen.getByTestId("probe")).toHaveTextContent("batch-1");

    await act(async () => {
      vi.advanceTimersByTime(1_500);
    });
    expect(getImportBatchStatus).toHaveBeenCalledTimes(2);

    // The daemon says complete: the interval is cleared, not left running.
    (getImportBatchStatus as ReturnType<typeof vi.fn>).mockResolvedValue(
      makeBatch({ complete: true }),
    );
    await act(async () => {
      vi.advanceTimersByTime(1_500);
    });
    const settledCalls = (getImportBatchStatus as ReturnType<typeof vi.fn>).mock.calls.length;
    await act(async () => {
      vi.advanceTimersByTime(30_000);
    });
    expect(getImportBatchStatus).toHaveBeenCalledTimes(settledCalls);
  });

  it("leaves no interval running after unmount", async () => {
    (getImportBatchStatus as ReturnType<typeof vi.fn>).mockResolvedValue(makeBatch());

    const { unmount } = render(<Probe batchId="batch-1" />);
    await act(async () => {});
    const callsAtUnmount = (getImportBatchStatus as ReturnType<typeof vi.fn>).mock.calls.length;
    unmount();
    await act(async () => {
      vi.advanceTimersByTime(30_000);
    });
    expect(getImportBatchStatus).toHaveBeenCalledTimes(callsAtUnmount);
  });

  it("does nothing without a batch id", async () => {
    render(<Probe batchId={null} />);
    await act(async () => {
      vi.advanceTimersByTime(30_000);
    });
    expect(getImportBatchStatus).not.toHaveBeenCalled();
    expect(screen.getByTestId("probe")).toHaveTextContent("none");
  });
});
