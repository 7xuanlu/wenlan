// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { toast } from "sonner";
import { getChunks, openSearchResult, type ChunkDetail, type IndexedFileInfo } from "../../../lib/tauri";
import SourceDocumentPreview from "./SourceDocumentPreview";

vi.mock("../../../lib/tauri", () => ({ getChunks: vi.fn(), openSearchResult: vi.fn() }));
vi.mock("sonner", () => ({ toast: vi.fn() }));

const sourceFile = (patch: Partial<IndexedFileInfo> = {}): IndexedFileInfo => ({
  source: "webpage", source_id: "https://example.com/article", title: "Reference article", url: "https://example.com/article", chunk_count: 2, last_modified: 1, ...patch,
});
const chunk = (index: number, content: string): ChunkDetail => ({ id: String(index), content, chunk_index: index, chunk_type: null, language: null });

function renderPreview(file = sourceFile()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const onClose = vi.fn();
  render(<QueryClientProvider client={client}><SourceDocumentPreview file={file} onClose={onClose} /></QueryClientProvider>);
  return { onClose };
}

beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(getChunks).mockResolvedValue([]);
  vi.mocked(openSearchResult).mockResolvedValue(undefined);
});

describe("SourceDocumentPreview", () => {
  it("loads webpage chunks through the webpage source and renders them in document order", async () => {
    vi.mocked(getChunks).mockResolvedValue([chunk(1, "Second paragraph."), chunk(0, "First paragraph.")]);
    renderPreview();
    expect(await screen.findByText("First paragraph.")).toBeInTheDocument();
    expect(getChunks).toHaveBeenCalledExactlyOnceWith("webpage", "https://example.com/article");
    const first = screen.getByText("First paragraph.");
    const second = screen.getByText("Second paragraph.");
    expect(first.compareDocumentPosition(second) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.queryByText("No readable text is available for this source.")).not.toBeInTheDocument();
  });

  it.each([["file", "file"], ["memory", "memory"], ["directory", "memory"], ["obsidian", "memory"]])(
    "uses the correct chunk boundary for a %s import", async (source, chunkSource) => {
      renderPreview(sourceFile({ source, source_id: "import-id", url: null }));
      await screen.findByText("No readable text is available for this source.");
      expect(getChunks).toHaveBeenCalledExactlyOnceWith(chunkSource, "import-id");
    },
  );

  it("keeps pending loading distinct from empty text", () => {
    vi.mocked(getChunks).mockImplementation(() => new Promise(() => {}));
    renderPreview();
    expect(screen.getByRole("status")).toHaveTextContent("Loading source…");
    expect(screen.queryByText("No readable text is available for this source.")).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows a failed preview and retries without substituting unverified inventory summary", async () => {
    vi.mocked(getChunks).mockRejectedValueOnce(new Error("unavailable")).mockResolvedValue([chunk(0, "Recovered text.")]);
    renderPreview(sourceFile({ summary: "Available source summary." }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load this source.");
    expect(screen.queryByText("Available source summary.")).not.toBeInTheDocument();
    expect(screen.queryByText("No readable text is available for this source.")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Recovered text.")).toBeInTheDocument();
    expect(getChunks).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("reports successful empty text without substituting an inventory summary", async () => {
    renderPreview(sourceFile({ summary: "Summary only." }));
    expect(await screen.findByText("No readable text is available for this source.")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Summary" })).not.toBeInTheDocument();
    expect(screen.queryByText("Summary only.")).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Try again" })).not.toBeInTheDocument();
  });

  it("opens the original safe web address only after explicit click", async () => {
    renderPreview();
    await screen.findByText("No readable text is available for this source.");
    expect(openSearchResult).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Open original" }));
    expect(openSearchResult).toHaveBeenCalledExactlyOnceWith("https://example.com/article");
  });

  it.each(["javascript:alert(1)", "file:///private/source.txt", "https://user:password@example.com", "not-an-address"])(
    "never offers Open original for unsafe address %s", async (url) => {
      renderPreview(sourceFile({ source_id: url, url }));
      await screen.findByText("No readable text is available for this source.");
      expect(screen.queryByRole("button", { name: "Open original" })).not.toBeInTheDocument();
      expect(openSearchResult).not.toHaveBeenCalled();
    },
  );

  it("surfaces an original-open error while preserving the source preview", async () => {
    vi.mocked(openSearchResult).mockRejectedValue(new Error("open failed"));
    vi.mocked(getChunks).mockResolvedValue([chunk(0, "Kept document text.")]);
    renderPreview();
    await screen.findByText("Kept document text.");
    fireEvent.click(screen.getByRole("button", { name: "Open original" }));
    await waitFor(() => expect(toast).toHaveBeenCalledWith("Could not open the original."));
    expect(screen.getByText("Kept document text.")).toBeInTheDocument();
  });

  it("uses the webpage identity and never exposes mixed legacy summary when its route is unavailable", async () => {
    vi.mocked(getChunks).mockRejectedValue(new Error("HTTP 404 Not Found"));
    renderPreview(sourceFile({ url: "https://unrelated.example/memory", summary: "Unrelated memory summary." }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load this source.");
    expect(screen.queryByText("Unrelated memory summary.")).not.toBeInTheDocument();
    expect(screen.queryByText("https://unrelated.example/memory")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Open original" }));
    expect(openSearchResult).toHaveBeenCalledExactlyOnceWith("https://example.com/article");
  });

  it("renders inline and reference images in the body without fetching elements", async () => {
    const markdown = "Before image.\n\n![Inline remote image](https://images.example.com/tracker.png)\n\n![Reference remote image][remote]\n\n[remote]: https://images.example.com/reference.png";
    vi.mocked(getChunks).mockResolvedValue([chunk(0, markdown)]);
    renderPreview();
    expect(await screen.findByText("Inline remote image")).toBeInTheDocument();
    expect(screen.getByText("Reference remote image")).toBeInTheDocument();
    const dialog = screen.getByRole("dialog");
    expect(dialog.querySelector("img,iframe,video,audio,source,object,embed,image,[src],[srcset]")).toBeNull();
    expect(openSearchResult).not.toHaveBeenCalled();
  });
});
