// SPDX-License-Identifier: AGPL-3.0-only
// Fixture data for the browser preview harness. Mirrors the shapes in
// PageDetail.citations.test.tsx so the preview shows the same states the
// tests assert.
import type {
  Page,
  PageCitation,
  PageLinksResponse,
  PageSourceWithMemory,
  ListPageRevisionsResponse,
} from "../src/lib/tauri";

const cite = (
  occurrence: number,
  marker: number,
  over: Partial<PageCitation> = {},
): PageCitation => ({
  occurrence,
  marker,
  source_kind: "memory",
  locator: `mem-${marker}`,
  score: 0.9,
  status: "verified",
  scope: "sentence",
  ...over,
});

const CITED_CONTENT = `# Wenlan Daemon Architecture

The wenlan daemon runs entirely on the local machine, with no cloud dependency for core recall.[1] Storage is backed by libSQL with vector search extensions.[2]

## Storage layer

Distilled pages and raw memories share a single database file.[3] The schema versioning strategy follows the upstream design notes.[4]

## Distillation

Pages are compiled from source memories by the pipeline described in [[Memory Distillation]].[5] Authored sections survive re-distillation untouched.[6]

See also [[LibSQL Storage]].
`;

const base = {
  summary: null,
  entity_id: null,
  domain: "architecture",
  source_memory_ids: ["mem-1", "mem-2"],
  version: 3,
  status: "active",
  created_at: "2026-06-20T09:00:00+00:00",
  last_compiled: "2026-07-02T18:30:00+00:00",
  last_modified: "2026-07-02T18:30:00+00:00",
};

export const PAGES: Record<string, Page> = {
  "page-cited": {
    ...base,
    id: "page-cited",
    title: "Wenlan Daemon Architecture",
    content: CITED_CONTENT,
    citations: [
      cite(1, 1),
      cite(2, 2, { status: "unverified" }),
      cite(3, 3, { locator: "mem-9" }),
      cite(4, 4, {
        source_kind: "external_url",
        locator: "https://docs.turso.tech/libsql",
      }),
      cite(5, 5, {
        source_kind: "external_file",
        locator: "/Users/lucian/notes/distillation-design.md",
      }),
      cite(6, 6, { source_kind: "authored", locator: "authored" }),
    ],
  } as Page,
  "page-cleared": {
    ...base,
    id: "page-cleared",
    title: "Edit-Cleared Citations",
    content: CITED_CONTENT,
    citations: [],
  } as Page,
  "page-mismatch": {
    ...base,
    id: "page-mismatch",
    title: "Mismatched Citations",
    content:
      "# Mismatched Citations\n\nOnly two markers here.[1] But three citations arrive from the wire.[2]",
    citations: [cite(1, 1), cite(2, 2), cite(3, 3)],
  } as Page,
  "page-plain": {
    ...base,
    id: "page-plain",
    title: "Memory Distillation",
    content:
      "# Memory Distillation\n\nA page with no citation markers at all — the pre-citations rendering path.",
    citations: undefined,
  } as Page,
};

// Pristine copies so the mock redistill_page can restore after update_page
// clears citations (mirrors the backend edit-path contract).
export const PRISTINE: Record<string, Page> = JSON.parse(JSON.stringify(PAGES));

const memory = (
  id: string,
  title: string,
  content: string,
  lastModified: number,
) => ({
  source: { page_id: "page-cited", memory_source_id: id, linked_at: 1_750_000_000 },
  memory: {
    source_id: id,
    title,
    content,
    summary: null,
    memory_type: "memory",
    domain: null,
    source_agent: "claude-code",
    confidence: 0.92,
    confirmed: true,
    pinned: false,
    supersedes: null,
    last_modified: lastModified,
    chunk_count: 1,
  },
});

export const SOURCES: PageSourceWithMemory[] = [
  memory(
    "mem-1",
    "Local-first decision",
    "We keep the daemon local-first: recall never leaves the machine.",
    1_751_300_000,
  ),
  memory(
    "mem-2",
    "libSQL storage choice",
    "Chose libSQL over sqlite-vss for the built-in vector extension.",
    1_748_000_000,
  ),
  // mem-9 is deliberately absent: exercises the unresolvable-source fallback
  // in CitationChip and the count-vs-rows case in PageInfo.
];

export const LINKS: PageLinksResponse = {
  outbound: [
    { label: "Memory Distillation", target_page_id: "page-plain" },
    { label: "LibSQL Storage", target_page_id: null },
  ],
  inbound: [{ source_page_id: "page-plain", label: "Wenlan Daemon Architecture" }],
};

export const REVISIONS: ListPageRevisionsResponse = {
  page_id: "page-cited",
  current_version: 3,
  user_edited: false,
  stale_reason: null,
  entries: [
    {
      version: 3,
      at: 1_751_480_000,
      edited_by: "distiller",
      delta_summary: "Re-distilled after 2 new source memories",
      incoming_source_ids: ["mem-2"],
      citations_summary: "Citations: 6 (1 unverified)",
    },
    {
      version: 2,
      at: 1_750_400_000,
      edited_by: "user",
      delta_summary: "Manual edit of the storage section",
      incoming_source_ids: null,
      citations_summary: null,
    },
    {
      version: 1,
      at: 1_750_000_000,
      edited_by: "distiller",
      delta_summary: "Initial distillation from 2 memories",
      incoming_source_ids: ["mem-1", "mem-2"],
      citations_summary: "Citations: 4",
    },
  ],
};
