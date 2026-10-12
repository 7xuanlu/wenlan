// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from "vitest";
import { getSourceKind, getSourcePresentation, type SourcePresentationInput } from "./sourcePresentation";

const labels = { document: "Document", webExcerpt: "Web excerpt" };
const source = (patch: Partial<SourcePresentationInput> = {}): SourcePresentationInput => ({
  source: "file",
  source_id: "/Users/me/Research/notes.pdf",
  title: "Research notes",
  url: null,
  ...patch,
});

describe("source presentation", () => {
  it("classifies indexed Obsidian sections by the underlying file without changing their identity", () => {
    const file = source({ source: "memory", source_agent: "obsidian", source_id: "vault::/notes/Books/Reading.md::2" });
    expect(getSourcePresentation(file, labels).metadata).toBe("Markdown · Books");
    expect(getSourceKind(file)).toBe("text");
    expect(file.source_id).toBe("vault::/notes/Books/Reading.md::2");
    expect(getSourcePresentation(source({ source: "obsidian", source_id: "vault::C:\\Notes\\Reading.md::0" }), labels).format).toBe("Markdown");
  });

  it("keeps a literal ordinal suffix in non-Obsidian filenames", () => {
    expect(getSourceKind(source({ source: "directory", source_id: "folder::/notes/Reading.md::2" }))).toBe("file");
  });
  it("keeps a remote PDF classified by format and uses its host as provenance", () => {
    const file = source({
      source: "webpage",
      source_id: "https://docs.example.test/Guide.PDF?download=1#page=2",
      url: null,
    });
    expect(getSourceKind(file)).toBe("pdf");
    expect(getSourcePresentation(file, labels)).toEqual({
      kind: "pdf", format: "PDF", origin: "docs.example.test", metadata: "PDF · docs.example.test",
    });
  });

  it("uses a local Windows path for format and parent-folder metadata, ignoring a misleading URL", () => {
    const file = source({
      source: "directory",
      source_id: "folder-1::C:\\Users\\me\\Research\\Notes.MD",
      title: "Meeting with Alice.pdf",
      url: "https://unrelated.example.test/article",
    });
    expect(getSourceKind(file)).toBe("text");
    expect(getSourcePresentation(file, labels).metadata).toBe("Markdown · Research");
  });

  it("recognizes image and plain-text formats from URL paths with query strings and fragments", () => {
    const image = source({ source: "webpage", source_id: "https://cdn.example.test/Photo.JPEG?v=2#crop" });
    const text = source({ source: "file", source_id: "https://cdn.example.test/readme.TXT?raw=true" });
    expect(getSourceKind(image)).toBe("image");
    expect(getSourcePresentation(image, labels).metadata).toBe("JPEG · cdn.example.test");
    expect(getSourceKind(text)).toBe("text");
    expect(getSourcePresentation(text, labels).metadata).toBe("TXT · cdn.example.test");
  });

  it("uses the webpage label and host for URLs without a known file format", () => {
    const file = source({ source: "webpage", source_id: "https://news.example.test/story?ref=notes#top" });
    expect(getSourceKind(file)).toBe("webpage");
    expect(getSourcePresentation(file, labels).metadata).toBe("Web excerpt · news.example.test");
  });

  it("recognizes webpage route suffixes while keeping unknown remote files generic", () => {
    const webpage = source({ source: "webpage", source_id: "https://example.test/article.html?x=1#top" });
    const archive = source({ source: "file", source_id: "https://cdn.example.test/archive.zip?download=1" });
    const csv = source({ source: "file", source_id: "https://cdn.example.test/export.csv" });
    expect(getSourceKind(webpage)).toBe("webpage");
    expect(getSourcePresentation(webpage, labels).metadata).toBe("Web excerpt · example.test");
    expect(getSourceKind(archive)).toBe("file");
    expect(getSourceKind(csv)).toBe("file");
  });

  it("keeps unknown real paths generic even when their prose title ends in a known extension", () => {
    const file = source({ source_id: "import::/archive/meeting.backup", title: "Project notes.pdf" });
    expect(getSourceKind(file)).toBe("file");
    expect(getSourcePresentation(file, labels)).toEqual({
      kind: "file", format: "Document", origin: "archive", metadata: "Document · archive",
    });
  });

  it("uses a title extension only when no real path or URL is available", () => {
    const file = source({ source_id: "opaque-source-id", title: "Notes.md", url: null });
    expect(getSourceKind(file)).toBe("text");
    expect(getSourcePresentation(file, labels).metadata).toBe("Markdown");
  });
});
