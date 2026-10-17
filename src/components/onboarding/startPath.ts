// SPDX-License-Identifier: AGPL-3.0-only
//
// The path a new user picked on the welcome screen. The getting-started
// checklist puts that step first, so the Wiki opens on the thing they said
// they wanted to do. Per-viewer convenience only: losing it just restores the
// default order.

export type StartPath = "tool" | "sources" | "note";

const KEY = "onboarding:startPath";
const PATHS: readonly StartPath[] = ["tool", "sources", "note"];

export function readStartPath(): StartPath | null {
  try {
    const value = localStorage.getItem(KEY);
    return PATHS.find((path) => path === value) ?? null;
  } catch {
    return null;
  }
}

export function writeStartPath(path: StartPath) {
  try {
    localStorage.setItem(KEY, path);
  } catch {
    // Storage can be unavailable; the checklist then keeps its default order.
  }
}
