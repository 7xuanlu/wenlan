// SPDX-License-Identifier: AGPL-3.0-only
import { Brain } from "@phosphor-icons/react";
import type { PageCitation } from "../../../lib/tauri";
import { PageIcon } from "../assets/PageIcon";
import { SourceTypeIcon } from "../sources/SourceTypeIcon";
import { getSourceKind } from "../sources/sourcePresentation";

export type ReferenceKind = "page" | "memory" | "file" | "url" | "authored";

export function citationReferenceKind(citation: PageCitation): ReferenceKind {
  switch (citation.source_kind) {
    case "memory": return "memory";
    case "external_file": return "file";
    case "external_url": return "url";
    case "authored": return "authored";
  }
}

export function ReferenceTypeIcon({ kind, size = 13, locator }: { kind: ReferenceKind; size?: number; locator?: string }) {
  if (kind === "memory") return <Brain size={size} weight="regular" aria-hidden="true" focusable="false" />;
  if (kind === "page" || kind === "authored") return <PageIcon size={size} />;
  const sourceKind = getSourceKind({ source: kind === "url" ? "webpage" : "file", source_id: locator ?? "", title: "" });
  return <SourceTypeIcon kind={sourceKind} size={size} />;
}
