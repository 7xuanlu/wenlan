// SPDX-License-Identifier: AGPL-3.0-only
import { useState, useCallback, useMemo, useRef, useEffect } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  getPage,
  getPageLinks,
  getPageRevisions,
  redistillPage,
  updatePage,
  deletePage,
  clipboardWrite,
  exportPageToObsidian,
  listRegisteredSources,
  getPageSources,
} from "../../lib/tauri";
import ContentRenderer from "./ContentRenderer";
import RelatedPages from "./page/RelatedPages";
import PageInfo from "./page/PageInfo";
import { processCitations, stripCitationLinks } from "../../lib/pageCitations";
import CitationChip from "./page/CitationChip";

interface PageDetailProps {
  pageId: string;
  onBack: () => void;
  onMemoryClick: (sourceId: string) => void;
  onPageClick?: (pageId: string) => void;
}

function relativeTimeFromISO(iso: string): string {
  const diff = Math.floor((Date.now() - new Date(iso).getTime()) / 1000);
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

function normalizeLinkLabel(label: string): string {
  return label.trim().toLowerCase();
}

function parseWikilink(inner: string): { targetLabel: string; displayText: string } {
  const pipeIndex = inner.indexOf("|");
  const rawTarget = pipeIndex >= 0 ? inner.slice(0, pipeIndex) : inner;
  const headingIndex = rawTarget.indexOf("#");
  const targetLabel = (headingIndex >= 0 ? rawTarget.slice(0, headingIndex) : rawTarget).trim();
  const targetDisplay = targetLabel || rawTarget.trim();
  const alias = pipeIndex >= 0 ? inner.slice(pipeIndex + 1).trim() : "";
  return {
    targetLabel,
    displayText: alias || targetDisplay || inner.trim(),
  };
}

function folderName(path: string): string {
  return path.split("/").filter(Boolean).pop() || path;
}

const PAGE_LINK_ANCHOR_PREFIX = "#concept:";

export default function PageDetail({ pageId, onBack, onMemoryClick, onPageClick }: PageDetailProps) {
  const queryClient = useQueryClient();
  const [copied, setCopied] = useState(false);
  const [exported, setExported] = useState(false);
  const [editing, setEditing] = useState(false);
  const [editContent, setEditContent] = useState("");
  const [exportMenuOpen, setExportMenuOpen] = useState(false);
  const [redistillNotice, setRedistillNotice] = useState<{
    kind: "success" | "warning" | "error";
    message: string;
  } | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const { data: page, isLoading } = useQuery({
    queryKey: ["page", pageId],
    queryFn: () => getPage(pageId),
  });

  const { data: pageLinks } = useQuery({
    queryKey: ["page-links", pageId],
    queryFn: () => getPageLinks(pageId),
    enabled: !!pageId,
    staleTime: 30_000,
    retry: false,
  });

  const { data: pageRevisions } = useQuery({
    queryKey: ["page-revisions", pageId],
    queryFn: () => getPageRevisions(pageId),
    enabled: !!pageId,
    staleTime: 30_000,
    retry: false,
  });

  const outboundTargetByLabel = useMemo(() => {
    const map = new Map<string, string>();
    for (const link of pageLinks?.outbound ?? []) {
      if (link.target_page_id) {
        map.set(normalizeLinkLabel(link.label), link.target_page_id);
      }
    }
    return map;
  }, [pageLinks]);

  const { data: registeredSources = [] } = useQuery({
    queryKey: ["registeredSources"],
    queryFn: () => listRegisteredSources(),
    staleTime: 30000,
  });

  const obsidianSources = useMemo(
    () => registeredSources.filter((s) => s.source_type === "obsidian"),
    [registeredSources],
  );

  const { data: pageSources } = useQuery({
    queryKey: ["page-sources", pageId],
    queryFn: () => getPageSources(pageId),
    enabled: !!pageId,
  });

  useEffect(() => {
    setRedistillNotice(null);
  }, [pageId]);

  const updateMutation = useMutation({
    mutationFn: (content: string) => updatePage(pageId, content),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["page", pageId] });
      queryClient.invalidateQueries({ queryKey: ["pages"] });
      queryClient.invalidateQueries({ queryKey: ["page-links", pageId] });
      queryClient.invalidateQueries({ queryKey: ["page-revisions", pageId] });
      setEditing(false);
    },
  });

  const deleteMutation = useMutation({
    mutationFn: () => deletePage(pageId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["pages"] });
      onBack();
    },
  });

  const redistillMutation = useMutation({
    mutationFn: () => redistillPage(pageId),
    onSuccess: (result) => {
      queryClient.invalidateQueries({ queryKey: ["page", pageId] });
      queryClient.invalidateQueries({ queryKey: ["pages"] });
      queryClient.invalidateQueries({ queryKey: ["page-links", pageId] });
      queryClient.invalidateQueries({ queryKey: ["page-revisions", pageId] });
      queryClient.invalidateQueries({ queryKey: ["page-sources", pageId] });
      if (result.status === "skipped") {
        setRedistillNotice({
          kind: "warning",
          message: result.hint || "Page re-distill skipped.",
        });
        return;
      }
      setRedistillNotice({
        kind: "success",
        message: result.updated ? "Page re-distilled." : "Page already up to date.",
      });
    },
    onError: (error) => {
      setRedistillNotice({
        kind: "error",
        message: error instanceof Error ? error.message : "Page re-distill failed.",
      });
    },
  });

  const pageHasUserEdits = Boolean(page?.user_edited || pageRevisions?.user_edited);
  const handleRedistillClick = () => {
    if (
      pageHasUserEdits &&
      !confirm("Re-distill this edited page? The current version stays in page history for recovery.")
    ) {
      return;
    }
    redistillMutation.mutate();
  };

  const copyAsContext = useCallback(async () => {
    if (!page) return;
    const space = page.domain ? `**Space:** ${page.domain}` : "";
    const version = `**Version:** ${page.version}`;
    const compiled = `**Last compiled:** ${page.last_compiled}`;
    const meta = [space, version, compiled].filter(Boolean).join("\n");
    const text = [
      `## ${page.title}`,
      meta,
      "",
      page.content,
    ].join("\n");
    await clipboardWrite(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }, [page]);

  const handleExportToVault = useCallback(
    async (vaultPath: string) => {
      setExportMenuOpen(false);
      await exportPageToObsidian(pageId, `${vaultPath}/Wenlan/pages`);
      setExported(true);
      setTimeout(() => setExported(false), 2000);
    },
    [pageId],
  );

  const handleSave = () => {
    if (editContent.trim() && editContent !== page?.content) {
      updateMutation.mutate(editContent.trim());
    } else {
      setEditing(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && e.metaKey) {
      e.preventDefault();
      handleSave();
    }
    if (e.key === "Escape") {
      setEditing(false);
    }
  };

  // Focus textarea on edit
  useEffect(() => {
    if (editing && textareaRef.current) {
      textareaRef.current.focus();
      textareaRef.current.selectionStart = textareaRef.current.value.length;
    }
  }, [editing]);

  if (isLoading) return null;

  if (!page) {
    return (
      <div className="flex flex-col items-center justify-center gap-2 py-20">
        <span
          style={{
            fontFamily: "var(--mem-font-body)",
            fontSize: "13px",
            color: "var(--mem-text-tertiary)",
          }}
        >
          Page not found
        </span>
        <button
          onClick={onBack}
          className="transition-colors text-sm"
          style={{ color: "var(--mem-text-secondary)" }}
        >
          Back
        </button>
      </div>
    );
  }

  const sourceCount = pageSources?.length ?? page.source_memory_ids.length;

  // Citations first (occurrence counting mirrors the backend and runs over the
  // raw stored body), then the existing display transforms.
  const processed = processCitations(page.content, page.citations);

  // Strip ## Sources (shown in Page info below)
  // Convert [[wikilinks]] to markdown links if they resolve to pages, else plain text
  const cleanedContent = processed.content
    .replace(/^#\s+.*\n+/, "") // Strip title heading (displayed separately by UI)
    .replace(/## Sources\n[\s\S]*?(?=\n## |\s*$)/, "")
    .replace(/\[\[([^\]]+)\]\]/g, (_match, inner) => {
      const link = parseWikilink(inner);
      const cid = outboundTargetByLabel.get(normalizeLinkLabel(link.targetLabel));
      if (cid) return `[${link.displayText}](${PAGE_LINK_ANCHOR_PREFIX}${cid})`;
      return link.displayText;
    })
    .trim();

  const sourceMemoryByLocator = new Map(
    (pageSources ?? [])
      .filter((cs) => cs.memory !== null)
      .map((cs) => [cs.source.memory_source_id, cs.memory!]),
  );

  // Extract TLDR (first sentence) for native rendering under title.
  // Match first sentence ending with ". " or ".\n" — but not inside [[wikilinks]] or after abbreviations.
  const sentenceEnd = cleanedContent.search(/\.\s/);
  const tldr = sentenceEnd > 0 && sentenceEnd < 400
    ? stripCitationLinks(cleanedContent.slice(0, sentenceEnd + 1).trim())
    : "";
  const displayContent = tldr
    ? cleanedContent.slice(sentenceEnd + 1).trim()
    : cleanedContent;

  // Intercept page/memory link clicks in rendered content (capture phase beats target="_blank")
  const handleContentClick = (e: React.MouseEvent) => {
    const anchor = (e.target as HTMLElement).closest("a");
    if (!anchor) return;
    const href = anchor.getAttribute("href") || "";
    if (href.startsWith(PAGE_LINK_ANCHOR_PREFIX)) {
      e.preventDefault();
      e.stopPropagation();
      onPageClick?.(href.replace(PAGE_LINK_ANCHOR_PREFIX, ""));
    } else if (href.startsWith("#memory:")) {
      e.preventDefault();
      e.stopPropagation();
      onMemoryClick(href.replace("#memory:", ""));
    }
  };

  const outboundLinks = pageLinks?.outbound ?? [];
  const inboundLinks = pageLinks?.inbound ?? [];
  const pageRevisionEntries = pageRevisions?.entries ?? [];

  return (
    <div className="flex flex-col gap-6">
      {/* Back + Header */}
      <div>
        <button
          onClick={onBack}
          className="p-1.5 -ml-1.5 rounded-md transition-colors duration-150 hover:bg-[var(--mem-hover)]"
          style={{ color: "var(--mem-text-tertiary)", background: "none", border: "none", cursor: "pointer", lineHeight: 0, marginBottom: "12px" }}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="M19 12H5M12 19l-7-7 7-7" /></svg>
        </button>

        <div className="flex items-start justify-between gap-4">
          <div className="flex-1 min-w-0">
            <h2
              style={{
                fontFamily: "var(--mem-font-heading)",
                fontSize: "20px",
                color: "var(--mem-text)",
                fontWeight: 500,
                lineHeight: "1.4",
              }}
            >
              {page.title}
            </h2>
            <div
              className="flex items-center gap-2 mt-1.5 flex-wrap"
              style={{ fontFamily: "var(--mem-font-mono)", fontSize: "11px", color: "var(--mem-text-tertiary)" }}
            >
              <span>Last distilled {relativeTimeFromISO(page.last_compiled)}</span>
              <span style={{ opacity: 0.4 }}>&middot;</span>
              <span>from {sourceCount} {sourceCount === 1 ? "memory" : "memories"}</span>
              {page.stale_reason && (
                <>
                  <span style={{ opacity: 0.4 }}>&middot;</span>
                  <span style={{ color: page.stale_reason === "source_conflict" ? "var(--mem-accent-amber)" : "var(--mem-text-tertiary)" }}>
                    {page.stale_reason === "source_conflict" ? "needs review" : "updating..."}
                  </span>
                </>
              )}
            </div>
          </div>

          {/* Actions — icon-only for clean top bar */}
          <div className="flex items-center gap-0.5 shrink-0">
            {!editing && (
              <button
                onClick={() => { setEditContent(page.content); setEditing(true); }}
                className="p-1.5 rounded-md transition-colors duration-150 hover:bg-[var(--mem-hover-strong)]"
                style={{ color: "var(--mem-text-tertiary)" }}
                title="Edit page"
              >
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7" />
                  <path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z" />
                </svg>
              </button>
            )}
            {!editing && (
              <button
                onClick={handleRedistillClick}
                disabled={redistillMutation.isPending}
                className="p-1.5 rounded-md transition-colors duration-150 hover:bg-[var(--mem-hover-strong)] disabled:opacity-50"
                style={{ color: "var(--mem-text-tertiary)" }}
                aria-label={redistillMutation.isPending ? "Re-distilling page" : "Re-distill page"}
                title={redistillMutation.isPending ? "Re-distilling..." : "Re-distill page"}
              >
                <svg
                  aria-hidden="true"
                  width="14"
                  height="14"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                >
                  <path d="M21 12a9 9 0 11-2.64-6.36" />
                  <path d="M21 3v6h-6" />
                </svg>
              </button>
            )}
            <button
              onClick={copyAsContext}
              className={`p-1.5 rounded-md transition-colors duration-150 ${
                copied ? "text-emerald-400" : "hover:bg-[var(--mem-hover-strong)]"
              }`}
              style={copied ? undefined : { color: "var(--mem-text-tertiary)" }}
              title={copied ? "Copied!" : "Copy as context"}
            >
              {copied ? (
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <polyline points="20 6 9 17 4 12" />
                </svg>
              ) : (
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
                  <path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1" />
                </svg>
              )}
            </button>
            {/* Export button: 0 sources = disabled, 1 = direct, 2+ = popover */}
            <div className="relative">
              {obsidianSources.length === 0 ? (
                <button
                  disabled
                  className="p-1.5 rounded-md opacity-40 cursor-not-allowed"
                  style={{ color: "var(--mem-text-tertiary)" }}
                  title="Add an Obsidian source in Settings to export"
                >
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4" />
                    <polyline points="7 10 12 15 17 10" />
                    <line x1="12" y1="15" x2="12" y2="3" />
                  </svg>
                </button>
              ) : (
                <button
                  onClick={() => {
                    if (obsidianSources.length === 1) {
                      handleExportToVault(obsidianSources[0].path);
                    } else {
                      setExportMenuOpen((v) => !v);
                    }
                  }}
                  className={`p-1.5 rounded-md transition-colors duration-150 ${
                    exported ? "text-emerald-400" : "hover:bg-[var(--mem-hover-strong)]"
                  }`}
                  style={exported ? undefined : { color: "var(--mem-text-tertiary)" }}
                  title={exported ? "Exported!" : "Export to Obsidian"}
                >
                  {exported ? (
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <polyline points="20 6 9 17 4 12" />
                    </svg>
                  ) : (
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4" />
                      <polyline points="7 10 12 15 17 10" />
                      <line x1="12" y1="15" x2="12" y2="3" />
                    </svg>
                  )}
                </button>
              )}
              {exportMenuOpen && obsidianSources.length >= 2 && (
                <div
                  className="absolute right-0 top-full mt-1 z-50 rounded-lg py-1 min-w-[180px]"
                  style={{
                    backgroundColor: "var(--mem-surface)",
                    border: "1px solid var(--mem-border)",
                    boxShadow: "0 4px 12px rgba(0,0,0,0.3)",
                  }}
                >
                  {obsidianSources.map((s) => (
                    <button
                      key={s.id}
                      onClick={() => handleExportToVault(s.path)}
                      className="w-full text-left px-3 py-2 text-[13px] transition-colors hover:bg-[var(--mem-hover)]"
                      style={{ color: "var(--mem-text)", fontFamily: "var(--mem-font-body)" }}
                    >
                      {folderName(s.path)}
                    </button>
                  ))}
                </div>
              )}
            </div>
            <button
              onClick={() => { if (confirm("Delete this page?")) deleteMutation.mutate(); }}
              className="p-1.5 rounded-md transition-colors duration-150 hover:bg-red-500/10"
              style={{ color: "var(--mem-text-tertiary)" }}
              title="Delete page"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M3 6h18M8 6V4a2 2 0 012-2h4a2 2 0 012 2v2m3 0v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6h14" />
              </svg>
            </button>
          </div>
        </div>
      </div>

      {redistillNotice && (
        <div
          role="status"
          aria-live="polite"
          className="rounded-lg px-3 py-2"
          style={{
            backgroundColor:
              redistillNotice.kind === "error"
                ? "rgba(239, 68, 68, 0.08)"
                : redistillNotice.kind === "warning"
                  ? "rgba(245, 158, 11, 0.08)"
                  : "rgba(16, 185, 129, 0.08)",
            border: "1px solid var(--mem-border)",
            color:
              redistillNotice.kind === "error"
                ? "#ef4444"
                : redistillNotice.kind === "warning"
                  ? "var(--mem-accent-amber)"
                  : "var(--mem-text-secondary)",
            fontFamily: "var(--mem-font-body)",
            fontSize: "12px",
            lineHeight: "1.5",
          }}
        >
          {redistillNotice.message}
        </div>
      )}

      {/* Content — edit mode or rendered */}
      {editing ? (
        <div className="flex flex-col gap-2">
          <textarea
            ref={textareaRef}
            value={editContent}
            onChange={(e) => setEditContent(e.target.value)}
            onKeyDown={handleKeyDown}
            className="w-full rounded-lg p-4 resize-y outline-none"
            style={{
              minHeight: "300px",
              backgroundColor: "var(--mem-surface)",
              border: "1px solid var(--mem-border)",
              color: "var(--mem-text)",
              fontFamily: "var(--mem-font-mono)",
              fontSize: "13px",
              lineHeight: "1.6",
            }}
          />
          <div className="flex items-center gap-2">
            <button
              onClick={handleSave}
              disabled={updateMutation.isPending}
              className="text-[11px] font-medium px-3 py-1.5 rounded-md transition-all"
              style={{ backgroundColor: "rgba(99, 102, 241, 0.15)", color: "var(--mem-accent-page)" }}
            >
              {updateMutation.isPending ? "Saving..." : "Save (Cmd+Enter)"}
            </button>
            <button
              onClick={() => setEditing(false)}
              className="text-[11px] font-medium px-3 py-1.5 rounded-md transition-all hover:bg-[var(--mem-hover-strong)]"
              style={{ color: "var(--mem-text-tertiary)" }}
            >
              Cancel (Esc)
            </button>
          </div>
        </div>
      ) : (
        <div onClickCapture={handleContentClick}>
          {(page.summary || tldr) && (
            <div
              className="pl-4 py-2 mb-4"
              style={{ borderLeft: "3px solid var(--mem-accent-page)" }}
            >
              <p
                style={{
                  fontFamily: "var(--mem-font-body)",
                  fontSize: "14px",
                  color: "var(--mem-text-secondary)",
                  lineHeight: "1.7",
                  fontStyle: "italic",
                }}
              >
                {page.summary || tldr}
              </p>
            </div>
          )}
          <ContentRenderer
            content={displayContent}
            variant="detail"
            renderCitation={(k) => {
              const c = processed.byOccurrence.get(k);
              if (!c) return null;
              return (
                <CitationChip
                  occurrence={k}
                  citation={c}
                  sourceMemory={sourceMemoryByLocator.get(c.locator) ?? null}
                  sourcesLoading={pageSources === undefined}
                  onOpenMemory={onMemoryClick}
                />
              );
            }}
          />
        </div>
      )}

      {!editing && <RelatedPages outbound={outboundLinks} onPageClick={onPageClick} />}

      {!editing && (
        <PageInfo
          sourceCount={sourceCount}
          sources={pageSources}
          inbound={inboundLinks}
          revisions={pageRevisionEntries}
          citations={page.citations}
          citationState={processed.state}
          onMemoryClick={onMemoryClick}
          onPageClick={onPageClick}
        />
      )}
    </div>
  );
}
