import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act, fireEvent } from "@testing-library/react";
import { open } from "@tauri-apps/plugin-dialog";
import { i18n } from "../../../i18n";

const mockSendNotification = vi.fn();

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-notification", () => ({
  sendNotification: (...args: unknown[]) => mockSendNotification(...args),
  isPermissionGranted: vi.fn(() => Promise.resolve(true)),
  requestPermission: vi.fn(() => Promise.resolve("granted")),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

const mockImportChatExport = vi.fn();
const mockSaveTempFile = vi.fn();
const mockListPendingImports = vi.fn();

vi.mock("../../../lib/tauri", () => {
  const labels: Record<string, string> = {
    parsing: "Reading archive",
    stage_a: "Importing conversations",
    stage_b: "Classifying and extracting entities",
    done: "Complete",
    error: "Failed",
  };
  return {
    importChatExport: (...args: unknown[]) => mockImportChatExport(...args),
    saveTempFile: (...args: unknown[]) => mockSaveTempFile(...args),
    listPendingImports: (...args: unknown[]) => mockListPendingImports(...args),
    importStageLabel: (stage: string) => labels[stage] ?? stage,
    IMPORT_STAGE_LABELS: labels,
  };
});

import { ImportFlow } from "../ImportFlow";

describe("ImportFlow", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    await i18n.changeLanguage("en");
    // Default: no pending imports
    mockListPendingImports.mockResolvedValue([]);
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("renders DropZone in idle state", async () => {
    const { getByTestId } = render(<ImportFlow />);
    expect(getByTestId("chat-import-drop-zone")).toBeTruthy();
  });

  it("shows the drop prompt text", async () => {
    const { getByText } = render(<ImportFlow />);
    expect(getByText("Drop export ZIP here")).toBeTruthy();
  });

  it("shows DropZone always (not replaced during import)", async () => {
    mockListPendingImports.mockResolvedValue([
      { id: "imp_1", vendor: "chatgpt", stage: "stage_b", total_conversations: 77 },
    ]);
    const { getByTestId } = render(<ImportFlow />);
    await act(async () => { await Promise.resolve(); });
    // DropZone is always present regardless of import state
    expect(getByTestId("chat-import-drop-zone")).toBeTruthy();
  });

  it("polls daemon for pending imports on mount", async () => {
    mockListPendingImports.mockResolvedValue([]);
    render(<ImportFlow />);
    await act(async () => { await vi.advanceTimersByTimeAsync(100); });
    expect(mockListPendingImports).toHaveBeenCalled();
  });

  it("shows idle (no strip) when no pending imports", async () => {
    mockListPendingImports.mockResolvedValue([]);
    const { queryByText } = render(<ImportFlow />);
    // Wait for the effect to run
    await vi.advanceTimersByTimeAsync(100);
    expect(queryByText(/Refining/)).toBeNull();
    expect(queryByText(/Importing/)).toBeNull();
  });

  it("keeps a dismissed refining strip hidden across polls that keep reporting the same import", async () => {
    mockListPendingImports.mockResolvedValue([
      { id: "imp_1", vendor: "chatgpt", stage: "stage_b", total_conversations: 77 },
    ]);
    const { getByRole, queryByRole } = render(<ImportFlow />);
    await act(async () => { await vi.advanceTimersByTimeAsync(100); });
    expect(getByRole("button", { name: "Dismiss" })).toBeTruthy();

    await act(async () => {
      getByRole("button", { name: "Dismiss" }).click();
    });
    expect(queryByRole("button", { name: "Dismiss" })).toBeNull();

    // Next poll still reports the same pending import (same id) — the strip
    // must not resurrect itself.
    await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
    expect(queryByRole("button", { name: "Dismiss" })).toBeNull();
  });

  it("marks the dismiss icon aria-hidden while the dismiss button keeps its own accessible name", async () => {
    const mockOpen = open as ReturnType<typeof vi.fn>;
    mockOpen.mockResolvedValue("/tmp/export.zip");
    mockImportChatExport.mockRejectedValue(new Error("boom"));

    const { getByRole, container } = render(<ImportFlow />);
    const chooseFile = getByRole("button", { name: /choose file/i });
    await act(async () => {
      chooseFile.click();
      await vi.advanceTimersByTimeAsync(50);
    });

    const dismissButton = getByRole("button", { name: "Dismiss" });
    expect(dismissButton).toHaveAttribute("aria-label", "Dismiss");
    container.querySelectorAll("svg").forEach((svg) => {
      expect(svg).toHaveAttribute("aria-hidden", "true");
    });
  });

  it("marks the success status icon aria-hidden", async () => {
    const mockOpen = open as ReturnType<typeof vi.fn>;
    mockOpen.mockResolvedValue("/tmp/export.zip");
    mockImportChatExport.mockResolvedValue({
      import_id: "imp_1",
      vendor: "chatgpt",
      conversations_total: 3,
      conversations_new: 3,
      conversations_skipped_existing: 0,
      memories_stored: 5,
    });

    const { getByRole, getByText, container } = render(<ImportFlow />);
    const chooseFile = getByRole("button", { name: /choose file/i });
    await act(async () => {
      chooseFile.click();
      await vi.advanceTimersByTimeAsync(50);
    });

    expect(getByText(/imported/i)).toBeInTheDocument();
    const icons = container.querySelectorAll("svg");
    expect(icons.length).toBeGreaterThan(0);
    icons.forEach((svg) => expect(svg).toHaveAttribute("aria-hidden", "true"));
  });

  it("reports busy while the daemon accepts an import and blocks duplicates", async () => {
    const mockOpen = open as ReturnType<typeof vi.fn>;
    mockOpen.mockResolvedValue("/tmp/export.zip");
    let resolveImport!: (value: unknown) => void;
    mockImportChatExport.mockImplementation(
      () => new Promise((resolve) => { resolveImport = resolve; }),
    );
    const onBusyChange = vi.fn();

    render(<ImportFlow onBusyChange={onBusyChange} />);
    const chooseFile = screen.getByRole("button", { name: /choose file/i });
    await act(async () => {
      chooseFile.click();
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(onBusyChange).toHaveBeenLastCalledWith(true);
    expect(chooseFile).toBeDisabled();
    expect(screen.getByText(/importing conversations/i)).toBeInTheDocument();

    resolveImport({
      import_id: "imp_1",
      vendor: "chatgpt",
      conversations_total: 1,
      conversations_new: 1,
      conversations_skipped_existing: 0,
      memories_stored: 1,
    });
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(onBusyChange).toHaveBeenLastCalledWith(false);
    expect(chooseFile).toBeEnabled();
  });

  it("surfaces File.arrayBuffer failures so the user can retry", async () => {
    const file = new File(["zip bytes"], "export.zip", { type: "application/zip" });
    Object.defineProperty(file, "arrayBuffer", {
      value: vi.fn().mockRejectedValue(new Error("read failed")),
    });
    const { getByTestId } = render(<ImportFlow />);
    fireEvent.drop(getByTestId("chat-import-drop-zone"), {
      dataTransfer: { files: [file], items: [], types: ["Files"] },
    });

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(screen.getByText(/read failed/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /choose file/i })).toBeEnabled();
  });

  it("surfaces saveTempFile failures so the user can retry", async () => {
    mockSaveTempFile.mockRejectedValue(new Error("temporary file failed"));
    const file = new File(["zip bytes"], "export.zip", { type: "application/zip" });
    const { getByTestId } = render(<ImportFlow />);
    fireEvent.drop(getByTestId("chat-import-drop-zone"), {
      dataTransfer: { files: [file], items: [], types: ["Files"] },
    });

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(screen.getByText(/temporary file failed/i)).toBeInTheDocument();
    expect(mockImportChatExport).not.toHaveBeenCalled();
  });

  it.each([
    ["en", "Drop export ZIP here"],
    ["zh-Hans", "将导出 ZIP 拖到这里"],
    ["zh-Hant", "將匯出 ZIP 拖到這裡"],
  ] as const)("keeps the existing DropZone copy in %s", async (locale, expected) => {
    await i18n.changeLanguage(locale);
    render(<ImportFlow />);
    expect(screen.getByText(expected)).toBeInTheDocument();
  });

  it("does not announce refinement success after an error row", async () => {
    mockListPendingImports
      .mockResolvedValueOnce([
        { id: "imp_1", vendor: "chatgpt", stage: "error", total_conversations: 1 },
      ])
      .mockResolvedValueOnce([]);
    render(<ImportFlow />);
    await act(async () => { await vi.advanceTimersByTimeAsync(100); });
    await act(async () => { await vi.advanceTimersByTimeAsync(5000); });

    expect(mockSendNotification).not.toHaveBeenCalledWith(
      expect.objectContaining({ body: "Import refinement complete" }),
    );
  });

  it("does not announce refinement success after a failed status query", async () => {
    mockListPendingImports
      .mockResolvedValueOnce([
        { id: "imp_1", vendor: "chatgpt", stage: "stage_b", total_conversations: 1 },
      ])
      .mockRejectedValueOnce(new Error("status unavailable"))
      .mockResolvedValueOnce([]);
    render(<ImportFlow />);
    await act(async () => { await vi.advanceTimersByTimeAsync(100); });
    await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
    await act(async () => { await vi.advanceTimersByTimeAsync(5000); });

    expect(mockSendNotification).not.toHaveBeenCalledWith(
      expect.objectContaining({ body: "Import refinement complete" }),
    );
  });
});
