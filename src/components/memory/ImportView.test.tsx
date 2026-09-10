import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, within, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { i18n } from "../../i18n";
import { ImportView, chunkImportText } from "./ImportView";
import { EXPORT_PROMPT, IMPORT_TYPE_TAGS } from "./importCopy";

vi.mock("../../lib/tauri", () => ({
  importMemories: vi.fn(),
  getImportBatchStatus: vi.fn(),
  clipboardWrite: vi.fn(),
  IMPORT_CHUNK_SIZE: 500,
}));

import { importMemories, getImportBatchStatus, clipboardWrite } from "../../lib/tauri";

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

  afterEach(async () => {
    vi.unstubAllGlobals();
    await i18n.changeLanguage("en");
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
      expect(screen.getByText("3 related pages")).toBeInTheDocument();
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
    expect(screen.getByText("2 entities confirmed")).toBeInTheDocument();
    expect(screen.getByText("7 related pages")).toBeInTheDocument();
    // Honest that background work continues — no final total that is not final.
    expect(screen.getByText(/Saved and ready to use. Organization progress/)).toBeInTheDocument();
    expect(screen.queryByText(/idle and has enough resources/)).not.toBeInTheDocument();
    expect(screen.queryByText(/keep climbing/)).not.toBeInTheDocument();
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
    expect(screen.queryByText(/Saved and ready to use. Organization progress/)).not.toBeInTheDocument();
  });

  it("shows type breakdown badges in summary", async () => {
    (importMemories as ReturnType<typeof vi.fn>).mockResolvedValue(
      chunkResult({ imported: 3, skipped: 0, breakdown: { identity: 1, fact: 2 } }),
    );

    renderImport();
    startImport("a\nb\nc");

    await waitFor(() => {
      expect(screen.getByText("Identity")).toBeInTheDocument();
      expect(screen.getByText("Fact")).toBeInTheDocument();
    });
    // Machine identifiers never leak into the staged summary.
    expect(screen.queryByText("identity")).not.toBeInTheDocument();
    expect(screen.queryByText("fact")).not.toBeInTheDocument();
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

  it.each([
    {
      language: "en",
      title: "Import Memories",
      importAction: "Import",
      upload: "Upload file",
      paste: "Paste output",
      copy: "Copy prompt",
      other: "Other",
      exportPrompt: "Export prompt",
      skip: "Skip",
      intro: "paste into ChatGPT",
    },
    {
      language: "zh-Hans",
      title: "导入记忆",
      importAction: "导入",
      upload: "上传文件",
      paste: "粘贴输出",
      copy: "复制提示词",
      other: "其他",
      exportPrompt: "导出提示词",
      skip: "跳过",
      intro: "粘贴到 ChatGPT",
    },
    {
      language: "zh-Hant",
      title: "匯入記憶",
      importAction: "匯入",
      upload: "上傳檔案",
      paste: "貼上輸出",
      copy: "複製提示詞",
      other: "其他",
      exportPrompt: "匯出提示詞",
      skip: "略過",
      intro: "貼到 ChatGPT",
    },
  ])(
    "localizes input chrome in $language while brands stay literal",
    async ({ language, title, importAction, upload, paste, copy, other, exportPrompt, skip, intro }) => {
      await i18n.changeLanguage(language);
      renderImport({ onSkip: vi.fn() });

      expect(screen.getByText(title)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: importAction })).toBeInTheDocument();
      expect(screen.getByText(upload)).toBeInTheDocument();
      expect(screen.getByText(paste)).toBeInTheDocument();
      expect(screen.getByText(copy)).toBeInTheDocument();
      expect(screen.getByText(exportPrompt)).toBeInTheDocument();
      expect(screen.getByText(skip)).toBeInTheDocument();
      // Brand names stay literal; only "Other" is localized.
      expect(screen.getByText("ChatGPT")).toBeInTheDocument();
      expect(screen.getByText("Claude")).toBeInTheDocument();
      expect(screen.getByText(other)).toBeInTheDocument();
      expect(screen.getByText(new RegExp(intro))).toBeInTheDocument();
    },
  );

  it("keeps protocol tags literal in the copied prompt and signals success only after resolution", async () => {
    (clipboardWrite as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

    renderImport();
    fireEvent.click(screen.getByText("Copy prompt"));

    await waitFor(() => {
      expect(clipboardWrite).toHaveBeenCalledTimes(1);
    });
    const copied = (clipboardWrite as ReturnType<typeof vi.fn>).mock.calls[0][0] as string;
    expect(copied).toBe(EXPORT_PROMPT);
    for (const tag of IMPORT_TYPE_TAGS) {
      expect(copied).toContain(tag);
    }
    await waitFor(() => {
      expect(screen.getByText("Copied!")).toBeInTheDocument();
    });
  });

  it.each([
    {
      language: "zh-Hans",
      copy: "复制提示词",
      copiedLabel: "已复制！",
      prose: "每一行都必须",
    },
    {
      language: "zh-Hant",
      copy: "複製提示詞",
      copiedLabel: "已複製！",
      prose: "每一行都必須",
    },
  ])(
    "copies a localized $language prompt preserving every literal tag",
    async ({ language, copy, copiedLabel, prose }) => {
      await i18n.changeLanguage(language);
      (clipboardWrite as ReturnType<typeof vi.fn>).mockResolvedValue(undefined);

      renderImport();
      // The shown instructions are localized too, not a giant English block.
      expect(screen.getByText(new RegExp(prose))).toBeInTheDocument();
      fireEvent.click(screen.getByText(copy));

      await waitFor(() => {
        expect(clipboardWrite).toHaveBeenCalledTimes(1);
      });
      const copiedText = (clipboardWrite as ReturnType<typeof vi.fn>).mock.calls[0][0] as string;
      expect(copiedText).toContain(prose);
      expect(copiedText).not.toBe(EXPORT_PROMPT);
      for (const tag of IMPORT_TYPE_TAGS) {
        expect(copiedText).toContain(tag);
      }
      await waitFor(() => {
        expect(screen.getByText(copiedLabel)).toBeInTheDocument();
      });
    },
  );

  it("keeps protocol tags untranslated in localized instructions", async () => {
    await i18n.changeLanguage("zh-Hant");
    renderImport();

    fireEvent.click(screen.getByText("其他"));
    const panel = screen.getByText(/貼上事實或記憶清單/);
    expect(panel.textContent).toContain("[identity]");
    expect(panel.textContent).toContain("[fact]");
    expect(panel.textContent).not.toContain("[身份]");
    expect(panel.textContent).not.toContain("[事實]");
  });

  it("shows localized feedback when the clipboard rejects", async () => {
    (clipboardWrite as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error("denied"));

    renderImport();
    fireEvent.click(screen.getByText("Copy prompt"));

    await waitFor(() => {
      expect(screen.getByText(/Couldn't copy/i)).toBeInTheDocument();
    });
    expect(screen.queryByText("Copied!")).not.toBeInTheDocument();
    vi.mocked(clipboardWrite).mockResolvedValueOnce(undefined);
    fireEvent.click(screen.getByText("Copy prompt"));
    expect(await screen.findByText("Copied!")).toBeInTheDocument();
    expect(screen.queryByText(/Couldn't copy/i)).not.toBeInTheDocument();
  });

  it("shows a localized read failure when FileReader errors", async () => {
    class FailingReader {
      onload: ((ev: unknown) => void) | null = null;
      onerror: (() => void) | null = null;
      onabort: (() => void) | null = null;
      readAsText() {
        this.onerror?.();
      }
    }
    vi.stubGlobal("FileReader", FailingReader);

    renderImport();
    const input = screen.getByLabelText("Upload file") as HTMLInputElement;
    fireEvent.change(input, {
      target: { files: [new File(["x"], "a.txt", { type: "text/plain" })] },
    });

    expect(
      await screen.findByText("Couldn't read that file. Try again."),
    ).toBeInTheDocument();
  });

  it("ignores a stale file read after typing newer text", async () => {
    const readers: Array<{
      onload: ((ev: { target: unknown }) => void) | null;
      result: unknown;
    }> = [];
    class ManualReader {
      onload: ((ev: { target: unknown }) => void) | null = null;
      onerror: (() => void) | null = null;
      onabort: (() => void) | null = null;
      result: unknown = null;
      readAsText() {
        readers.push(this);
      }
    }
    vi.stubGlobal("FileReader", ManualReader);

    renderImport();
    const textarea = screen.getByPlaceholderText(/paste your memories/i);
    const input = screen.getByLabelText("Upload file") as HTMLInputElement;
    fireEvent.change(input, {
      target: { files: [new File(["stale"], "first.txt", { type: "text/plain" })] },
    });
    fireEvent.change(textarea, { target: { value: "newer typed text" } });

    const first = readers[0]!;
    first.result = "STALE FILE TEXT";
    act(() => {
      first.onload!({ target: first });
    });

    expect(textarea).toHaveValue("newer typed text");
  });

  it("keeps the second file when two reads race", async () => {
    const readers: Array<{
      onload: ((ev: { target: unknown }) => void) | null;
      result: unknown;
    }> = [];
    class ManualReader {
      onload: ((ev: { target: unknown }) => void) | null = null;
      onerror: (() => void) | null = null;
      onabort: (() => void) | null = null;
      result: unknown = null;
      readAsText() {
        readers.push(this);
      }
    }
    vi.stubGlobal("FileReader", ManualReader);

    renderImport();
    const textarea = screen.getByPlaceholderText(/paste your memories/i);
    const input = screen.getByLabelText("Upload file") as HTMLInputElement;
    fireEvent.change(input, {
      target: { files: [new File(["a"], "first.txt", { type: "text/plain" })] },
    });
    fireEvent.change(input, {
      target: { files: [new File(["b"], "second.txt", { type: "text/plain" })] },
    });

    // The second read lands first, then the stale first read completes late.
    const [first, second] = readers as [
      { onload: ((ev: { target: unknown }) => void) | null; result: unknown },
      { onload: ((ev: { target: unknown }) => void) | null; result: unknown },
    ];
    second.result = "SECOND FILE TEXT";
    act(() => {
      second.onload!({ target: second });
    });
    expect(textarea).toHaveValue("SECOND FILE TEXT");
    first.result = "FIRST FILE TEXT";
    act(() => {
      first.onload!({ target: first });
    });
    expect(textarea).toHaveValue("SECOND FILE TEXT");
  });

  it("preserves backend detail behind a localized heading on import failure", async () => {
    (importMemories as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
      new Error("daemon exploded: disk full"),
    );

    renderImport();
    startImport("Memory");

    await waitFor(() => {
      expect(screen.getByText(/Import failed/)).toBeInTheDocument();
    });
    expect(screen.getByText(/daemon exploded: disk full/)).toBeInTheDocument();
    // Should go back to input form after error
    expect(screen.getByPlaceholderText(/paste your memories/i)).toBeInTheDocument();
  });

  it("localizes the import failure heading in zh-Hant", async () => {
    await i18n.changeLanguage("zh-Hant");
    (importMemories as ReturnType<typeof vi.fn>).mockRejectedValueOnce(new Error("boom"));

    renderImport();
    fireEvent.change(screen.getByLabelText("貼上輸出"), { target: { value: "Memory" } });
    fireEvent.click(screen.getByRole("button", { name: "匯入" }));

    expect(await screen.findByText(/匯入失敗/)).toBeInTheDocument();
    expect(screen.getByText(/boom/)).toBeInTheDocument();
  });

  it("renders unclassified as a human label in the summary", async () => {
    (importMemories as ReturnType<typeof vi.fn>).mockResolvedValue(
      chunkResult({ imported: 2, skipped: 0, breakdown: { unclassified: 2 } }),
    );

    renderImport();
    startImport("a\nb");

    await waitFor(() => {
      expect(screen.getByText("Unclassified")).toBeInTheDocument();
    });
    expect(screen.queryByText("unclassified")).not.toBeInTheDocument();
  });

  it("localizes summary categories in zh-Hant", async () => {
    await i18n.changeLanguage("zh-Hant");
    (importMemories as ReturnType<typeof vi.fn>).mockResolvedValue(
      chunkResult({ imported: 2, skipped: 0, breakdown: { unclassified: 1, fact: 1 } }),
    );

    renderImport();
    fireEvent.change(screen.getByLabelText("貼上輸出"), { target: { value: "a\nb" } });
    fireEvent.click(screen.getByRole("button", { name: "匯入" }));

    expect(await screen.findByText("待分類")).toBeInTheDocument();
    expect(screen.getByText("事實")).toBeInTheDocument();
    expect(screen.queryByText("unclassified")).not.toBeInTheDocument();
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
