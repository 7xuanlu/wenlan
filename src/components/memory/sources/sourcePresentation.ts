// SPDX-License-Identifier: AGPL-3.0-only
import type { IndexedFileInfo } from "../../../lib/tauri";

export type SourceKind = "pdf" | "text" | "image" | "webpage" | "file";
export type SourcePresentationInput = Pick<IndexedFileInfo, "source" | "source_id" | "title" | "url" | "source_agent">;
const WEBPAGE_EXTENSIONS = new Set(["html", "htm", "php", "asp", "aspx"]);

export interface SourcePresentationLabels {
  document: string;
  webExcerpt: string;
}

export interface SourcePresentation {
  kind: SourceKind;
  format: string;
  origin: string | null;
  metadata: string;
}

function httpUrl(value: string | null | undefined): URL | null {
  if (!value) return null;
  try {
    const url = new URL(value);
    return url.protocol === "http:" || url.protocol === "https:" ? url : null;
  } catch {
    return null;
  }
}

function sourceUrl(file: SourcePresentationInput): URL | null {
  const idUrl = httpUrl(file.source_id);
  if (file.source === "webpage") return idUrl ?? httpUrl(file.url);
  if (idUrl) return idUrl;

  // A local path is stronger provenance than an optional URL attached to the
  // indexed record (for example, a source article URL stored with an imported PDF).
  const separator = file.source_id.indexOf("::");
  const rawId = separator >= 0 ? file.source_id.slice(separator + 2) : file.source_id;
  const hasPathShape = separator >= 0 || /[\\/]/.test(rawId) || /\.[a-z0-9]+$/i.test(rawId);
  return hasPathShape ? null : httpUrl(file.url);
}

function sourcePath(file: SourcePresentationInput, url: URL | null): { value: string | null; known: boolean } {
  const separator = file.source_id.indexOf("::");
  const rawId = separator >= 0 ? file.source_id.slice(separator + 2) : file.source_id;
  // Obsidian imported IDs append a section ordinal after the real file path.
  // Keep that storage identifier intact; only strip it for visual classification.
  const raw = separator >= 0 && (file.source === "obsidian" || file.source_agent === "obsidian")
    ? rawId.replace(/::\d+$/, "") : rawId;
  if (!raw || httpUrl(raw)) return { value: null, known: separator >= 0 };

  // Namespaced IDs are used for imported paths, while plain opaque IDs (such
  // as a database key) are not enough evidence to override a useful title.
  const hasPathShape = separator >= 0 || /[\\/]/.test(raw) || /\.[a-z0-9]+$/i.test(raw);
  if (hasPathShape) return { value: raw.replace(/\\/g, "/"), known: true };

  // A webpage URL is the location, never a local path.
  if (url) return { value: null, known: false };
  return { value: null, known: false };
}

function extensionOf(value: string | null): string | null {
  if (!value) return null;
  const name = value.replace(/\\/g, "/").split("/").pop() ?? "";
  return /\.([a-z0-9]+)$/i.exec(name)?.[1].toLowerCase() ?? null;
}

function parentFolder(path: string | null): string | null {
  if (!path) return null;
  const normalized = path.replace(/\\/g, "/").replace(/\/+$/, "");
  const split = normalized.lastIndexOf("/");
  return split > 0 ? normalized.slice(0, split).split("/").pop() || null : null;
}

function sourceFormat(extension: string | null): { kind: SourceKind; label: string } | null {
  if (!extension) return null;
  if (extension === "pdf") return { kind: "pdf", label: "PDF" };
  if (["md", "markdown", "mdx"].includes(extension)) return { kind: "text", label: "Markdown" };
  if (["txt", "text"].includes(extension)) return { kind: "text", label: extension === "txt" ? "TXT" : "Text" };
  if (["png", "jpg", "jpeg", "gif", "webp", "svg", "avif", "bmp", "tif", "tiff"].includes(extension)) {
    return { kind: "image", label: extension.toUpperCase() };
  }
  return null;
}

interface SourceClassification {
  kind: SourceKind;
  format: string | null;
  origin: string | null;
}

function classifySource(file: SourcePresentationInput): SourceClassification {
  const url = sourceUrl(file);
  const path = sourcePath(file, url);
  const pathExtension = extensionOf(path.value);
  const urlExtension = url ? extensionOf(url.pathname) : null;
  const titleExtension = !path.known && !url ? extensionOf(file.title) : null;
  const format = sourceFormat(pathExtension ?? urlExtension ?? titleExtension);
  const host = url?.hostname || null;

  if (format) {
    return { kind: format.kind, format: format.label, origin: host ?? parentFolder(path.value) };
  }

  // A URL without a recognized file format represents a web page. This also
  // handles query strings and fragments because extension parsing uses URL.pathname.
  if (
    (url && (!urlExtension || WEBPAGE_EXTENSIONS.has(urlExtension))) ||
    (file.source === "webpage" && (!url || !urlExtension || WEBPAGE_EXTENSIONS.has(urlExtension)))
  ) {
    return { kind: "webpage", format: null, origin: host };
  }

  return { kind: "file", format: null, origin: host ?? parentFolder(path.value) };
}

/** Return only the visual resource kind, suitable for icon-only surfaces. */
export function getSourceKind(file: SourcePresentationInput): SourceKind {
  return classifySource(file).kind;
}

/** Resolve a source's content format and provenance consistently across views. */
export function getSourcePresentation(
  file: SourcePresentationInput,
  labels: SourcePresentationLabels,
): SourcePresentation {
  const classification = classifySource(file);
  if (classification.format) {
    return {
      kind: classification.kind,
      format: classification.format,
      origin: classification.origin,
      metadata: [classification.format, classification.origin].filter(Boolean).join(" · "),
    };
  }

  const format = classification.kind === "webpage" ? labels.webExcerpt : labels.document;
  return {
    ...classification,
    format,
    metadata: [format, classification.origin].filter(Boolean).join(" · "),
  };
}
