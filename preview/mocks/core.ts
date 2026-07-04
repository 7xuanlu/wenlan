// SPDX-License-Identifier: AGPL-3.0-only
// Browser stand-in for @tauri-apps/api/core (aliased in vite.preview.config.ts).
// Two backends: fixture data for /preview/ (flag set inline in its index.html),
// live daemon HTTP for the real app at /.
import { PAGES, PRISTINE, SOURCES, LINKS, REVISIONS } from "../fixtures";
import { liveInvoke } from "./live-invoke";

export async function invoke(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<unknown> {
  if (!(window as { __PREVIEW_FIXTURES__?: boolean }).__PREVIEW_FIXTURES__) {
    return liveInvoke(cmd, args);
  }
  switch (cmd) {
    case "get_page":
      return PAGES[args?.id as string] ?? null;
    case "get_page_sources":
      return SOURCES;
    case "get_page_links":
      return LINKS;
    case "get_page_revisions":
      return { ...REVISIONS, page_id: args?.pageId };
    case "list_registered_sources":
      return [];
    case "update_page": {
      // Mirror the backend edit contract: content updates, citations reset.
      const page = PAGES[args?.id as string];
      if (page) {
        page.content = args?.content as string;
        page.citations = [];
      }
      return null;
    }
    case "redistill_page": {
      const id = args?.pageId as string;
      if (PRISTINE[id]) PAGES[id] = JSON.parse(JSON.stringify(PRISTINE[id]));
      return { status: "ok", updated: true };
    }
    case "delete_page":
      return null;
    default:
      console.warn(`[preview] unmocked invoke: ${cmd}`, args);
      return null;
  }
}

export function convertFileSrc(path: string): string {
  return path;
}

// Stubs so other @tauri-apps plugins that import from api/core still bundle.
export const isTauri = () => false;
export class Resource {
  close() {}
}
export class Channel {
  onmessage: ((msg: unknown) => void) | null = null;
}
export class PluginListener {
  unregister() {}
}
export async function addPluginListener(): Promise<PluginListener> {
  return new PluginListener();
}
export function transformCallback(): number {
  return 0;
}
