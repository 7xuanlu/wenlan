// SPDX-License-Identifier: AGPL-3.0-only
import { Brain, FileText, Globe, Notebook } from "@phosphor-icons/react";
import type { PageCitation } from "../../../lib/tauri";

export type ReferenceKind = "page" | "memory" | "file" | "url" | "authored";

export function citationReferenceKind(citation: PageCitation): ReferenceKind {
  switch (citation.source_kind) {
    case "memory": return "memory";
    case "external_file": return "file";
    case "external_url": return "url";
    case "authored": return "authored";
  }
}

export function ReferenceTypeIcon({ kind, size = 13 }: { kind: ReferenceKind; size?: number }) {
  const Icon = kind === "memory" ? Brain : kind === "url" ? Globe : kind === "page" || kind === "authored" ? Notebook : FileText;
  return <Icon size={size} weight="regular" aria-hidden="true" focusable="false" />;
}
