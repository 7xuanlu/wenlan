// SPDX-License-Identifier: AGPL-3.0-only
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import { openFile, type PageCitation } from "../../../lib/tauri";

export function citationFilePath(locator: string): string {
  const sep = locator.lastIndexOf("::");
  return sep === -1 ? locator : locator.slice(sep + 2);
}

export async function openCitationTarget(citation: PageCitation): Promise<string | null> {
  try {
    if (citation.source_kind === "external_file") {
      await openFile(citationFilePath(citation.locator));
    } else if (citation.source_kind === "external_url") {
      await shellOpen(citation.locator);
    }
    return null;
  } catch (err) {
    return err instanceof Error ? err.message : String(err);
  }
}
