// SPDX-License-Identifier: AGPL-3.0-only
//
// Icon overlay helpers. The ambient overlay was removed in Phase 1 PR2;
// the icon-overlay surfaces in this file remain until Task 7 deletes them too.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface ShowIconPayload {
  text: string;
  x: number;  // macOS logical coords (origin = bottom-left)
  y: number;
}

export function listenShowIcon(
  callback: (payload: ShowIconPayload) => void,
): Promise<UnlistenFn> {
  return listen<ShowIconPayload>("show-icon", (event) => {
    callback(event.payload);
  });
}

export async function triggerIconClick(text: string, x: number, y: number): Promise<void> {
  return invoke("trigger_icon_click", { text, x, y });
}
