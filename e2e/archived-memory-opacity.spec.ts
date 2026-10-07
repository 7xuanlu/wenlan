// SPDX-License-Identifier: AGPL-3.0-only
//
// Computed-opacity guard for archive-superseded memories.
//
// PR #564 faded such memories to ARCHIVED_MEMORY_OPACITY and every jsdom test
// passed while Chrome computed 1 for the list row: the `mem-fade-up` entry
// animation runs with `both` fill, so its final keyframe keeps applying after
// the animation ends, and an animation outranks an inline style (#568). jsdom
// runs no cascade and no keyframes, so only a real browser can catch that
// class of defect. Each test puts an archived memory next to an unarchived one
// on one surface and reads the computed opacity of both — after checking that
// the entry animation really targets the faded element or its wrapper, since
// the guard proves nothing if the animation quietly stops running.
import { openPrimaryDestination } from "./helpers/primaryNavigation";
import { expect, test, type Locator, type Page } from "@playwright/test";
import { installTauriMock } from "./tauriMock";
import type { MemoryItem } from "../src/lib/tauri";
import { ARCHIVED_MEMORY_OPACITY } from "../src/components/memory/archivedMemoryOpacity";

const ARCHIVED_TITLE = "Ship the beta on Friday";
const NEIGHBOUR_TITLE = "Keep the review workflow local-first";
const MUTED = String(ARCHIVED_MEMORY_OPACITY);

function makeMemory(
  overrides: Partial<MemoryItem> & Pick<MemoryItem, "source_id" | "title">,
): MemoryItem {
  return {
    content: overrides.title,
    summary: null,
    memory_type: "fact",
    domain: "Wenlan",
    space: "Wenlan",
    source_agent: "claude-code",
    confidence: 0.9,
    confirmed: true,
    pinned: false,
    supersedes: null,
    last_modified: 1_700_000_000,
    chunk_count: 1,
    access_count: 0,
    is_recap: false,
    // Keep the neighboring row at its normal resting opacity.
    stability: "confirmed",
    ...overrides,
  };
}

// The predecessor a decision replaced in 'archive' mode. The daemon stamps it
// `is_archived`; that flag is all the surfaces read.
const memories: readonly MemoryItem[] = [
  makeMemory({
    source_id: "mem-archived",
    title: ARCHIVED_TITLE,
    is_archived: true,
    last_modified: 1_700_000_100,
  }),
  makeMemory({ source_id: "mem-neighbour", title: NEIGHBOUR_TITLE }),
];

async function openApp(page: Page): Promise<void> {
  await page.setViewportSize({ width: 1280, height: 900 });
  await installTauriMock(page, { locale: "en", localStorage: { "wenlan-spaces-view-mode": "rows" }, rawActions: [], memories });
  await page.goto("/");
}

async function expectMuted(archived: Locator, neighbour: Locator): Promise<void> {
  await expect(archived).toHaveCSS("opacity", MUTED);
  await expect(neighbour).toHaveCSS("opacity", "1");
}

test("list row on the Memories page", async ({ page }) => {
  await openApp(page);
  await openPrimaryDestination(page, "Memories");
  const list = page.getByRole("region", { name: "Memory list" });
  const archived = list.getByRole("article", { name: ARCHIVED_TITLE, exact: true });
  const neighbour = list.getByRole("article", { name: NEIGHBOUR_TITLE, exact: true });

  await expect(archived.getByText("archived")).toBeVisible();
  // The row itself is the animated element — the shape that failed before #568.
  await expect(archived).toHaveCSS("animation-name", "mem-fade-up");
  await expectMuted(archived, neighbour);
});
