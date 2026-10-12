// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { toast } from "sonner";
import { getChunks, openSearchResult, type IndexedFileInfo, type ChunkDetail } from "../../../lib/tauri";
import SourceDocumentPreview from "./SourceDocumentPreview";
vi.mock("../../../lib/tauri", () => ({ getChunks: vi.fn(), openSearchResult: vi.fn() }));
const previewFlushHarness = vi.hoisted(() => ({ flush: vi.fn<() => Promise<boolean>>(async () => true) }));
vi.mock("./SourceAnnotations", async () => {
  const { useEffect } = await import("react");
  return { default: ({ onRegisterFlush }: { onRegisterFlush?: (flush: (() => Promise<boolean>) | null) => void }) => {
    useEffect(() => {
      onRegisterFlush?.(previewFlushHarness.flush);
      return () => onRegisterFlush?.(null);
    }, [onRegisterFlush]);
    return <section>Editable source description.</section>;
  } };
});
vi.mock("sonner", () => ({ toast: vi.fn() }));
const sourceFile = (patch: Partial<IndexedFileInfo> = {}): IndexedFileInfo => ({ source: "webpage", source_id: "https://example.com/article", title: "Reference article", url: "https://example.com/article", chunk_count: 2, last_modified: 1, ...patch });
const textChunk = (content: string, chunk_index = 0): ChunkDetail => ({ id: `chunk-${chunk_index}`, content, chunk_index, chunk_type: null, language: null });
const importedFile = () => sourceFile({ source: "file", source_id: "article.md", url: null });
function renderPreview(file = sourceFile(), onRegisterFlush?: (flush: (() => Promise<boolean>) | null) => void) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const onClose = vi.fn();
  render(<QueryClientProvider client={client}><SourceDocumentPreview file={file} onClose={onClose} onRegisterFlush={onRegisterFlush} /></QueryClientProvider>);
  return { onClose };
}
beforeEach(() => { vi.resetAllMocks(); previewFlushHarness.flush.mockResolvedValue(true); vi.mocked(getChunks).mockResolvedValue([]); vi.mocked(openSearchResult).mockResolvedValue(undefined); });
describe("SourceDocumentPreview", () => {
  it("keeps link preview simple, with URL and Description instead of an extracted webpage", () => {
    renderPreview();
    expect(screen.getByText("Editable source description.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Open original" })).toHaveAttribute("href", "https://example.com/article");
    expect(getChunks).not.toHaveBeenCalled();
    expect(openSearchResult).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Refresh" })).not.toBeInTheDocument();
  });
  it("keeps the preview open when description flush fails on close", async () => {
    previewFlushHarness.flush.mockResolvedValue(false);
    const { onClose } = renderPreview(sourceFile(), vi.fn());
    await waitFor(() => expect(previewFlushHarness.flush).not.toHaveBeenCalled());
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(previewFlushHarness.flush).toHaveBeenCalledOnce());
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByText("Editable source description.")).toBeInTheDocument();
  });
  it.each([["file", "file"], ["memory", "memory"], ["directory", "memory"], ["obsidian", "memory"]])("uses the correct chunk boundary for a %s import", async (source, chunkSource) => {
    renderPreview(sourceFile({ source, source_id: "import-id", url: null }));
    await screen.findByText("No readable text is available for this source.");
    expect(getChunks).toHaveBeenCalledExactlyOnceWith(chunkSource, "import-id");
  });
  it("keeps file loading distinct from empty text", () => {
    vi.mocked(getChunks).mockImplementation(() => new Promise(() => {}));
    renderPreview(importedFile());
    expect(screen.getByRole("status")).toHaveTextContent("Loading source…");
    expect(screen.queryByText("No readable text is available for this source.")).not.toBeInTheDocument();
  });
  it("shows file load failures and retries without substituting inventory summaries", async () => {
    vi.mocked(getChunks).mockRejectedValueOnce(new Error("unavailable")).mockResolvedValue([textChunk("Recovered file text.")]);
    renderPreview({ ...importedFile(), summary: "Unverified summary." });
    expect(await screen.findByRole("alert")).toHaveTextContent("Could not load this source.");
    expect(screen.queryByText("Unverified summary.")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Recovered file text.")).toBeInTheDocument();
    expect(getChunks).toHaveBeenCalledTimes(2);
  });
  it("preserves file chunk order and uses the shared Markdown rendering", async () => {
    vi.mocked(getChunks).mockResolvedValue([textChunk("Last paragraph.", 1), textChunk("> Quoted passage.", 0)]);
    renderPreview(importedFile());
    const quote = await screen.findByText("Quoted passage.");
    expect(quote.closest("blockquote")).not.toBeNull();
    expect(quote.compareDocumentPosition(screen.getByText("Last paragraph.")) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
  it.each(["javascript:alert(1)", "file:///private/source.txt", "https://user:password@example.com", "not-an-address"])("never renders a link for unsafe address %s", (url) => {
    renderPreview(sourceFile({ source_id: url, url }));
    expect(screen.queryByRole("link", { name: "Open original" })).not.toBeInTheDocument();
    expect(openSearchResult).not.toHaveBeenCalled();
  });
  it("reports original-open errors while keeping Description visible", async () => {
    vi.mocked(openSearchResult).mockRejectedValue(new Error("open failed"));
    renderPreview();
    fireEvent.click(screen.getByRole("link", { name: "Open original" }));
    await waitFor(() => expect(toast).toHaveBeenCalledWith("Could not open the original."));
    expect(screen.getByText("Editable source description.")).toBeInTheDocument();
  });
  it("uses the source identity instead of an unrelated optional URL or summary", () => {
    renderPreview(sourceFile({ url: "https://unrelated.example/memory", summary: "Unrelated summary." }));
    expect(screen.queryByText("Unrelated summary.")).not.toBeInTheDocument();
    const link = screen.getByRole("link", { name: "Open original" });
    expect(link).toHaveAttribute("href", "https://example.com/article");
    fireEvent.click(link);
    expect(openSearchResult).toHaveBeenCalledExactlyOnceWith("https://example.com/article");
  });
  it("renders file images as text without fetching remote image elements", async () => {
    vi.mocked(getChunks).mockResolvedValue([textChunk("![Inline remote image](https://images.example.com/tracker.png)")]);
    renderPreview(importedFile());
    expect(await screen.findByText("Inline remote image")).toBeInTheDocument();
    expect(screen.getByRole("dialog").querySelector("img,iframe,video,audio,source,object,embed,image,[src],[srcset]")).toBeNull();
  });
});
