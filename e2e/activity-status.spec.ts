// SPDX-License-Identifier: AGPL-3.0-only
//
// The sidebar status line and its summary popover.
//
// Two things are being proved here. First, that the line is actually on every
// page and survives every width the app supports, because a status line that
// disappears on one view is worse than none: the user learns not to trust it.
// Second, that the popover is operable from the keyboard alone and gives focus
// back when it closes, because it opens from a control at the very bottom of
// the sidebar and a lost focus there strands a keyboard user at the end of the
// document.
//
import { expect, test, type Page } from "@playwright/test";
import type { ActivityResponse } from "../src/lib/tauri";
import { collectBrowserErrors, installTauriMock } from "./tauriMock";

/** Widest supported window, the common laptop, the tablet breakpoint, and the
 *  narrowest phone the shell claims to support. The last two put the sidebar
 *  in its overlay drawer, which is a different render path. */
const WIDTHS = [1487, 1280, 768, 375] as const;

const VIEWS = ["Home", "Wiki", "Entities", "Spaces"] as const;

/**
 * A fixture with work in every asset, so the popover has three real sentences
 * to render rather than three empty states. Synthesis runs on Anthropic and
 * everyday on device: that mix is the one the trust sentence has to get right.
 */
const ACTIVITY_FIXTURE: ActivityResponse = {
  state: "organizing",
  last_activity_at: 1_783_728_000,
  assets: [
    {
      kind: "memories",
      state: "running",
      done: 7,
      total: 12,
      blocked: 0,
      steps: [
        { name: "store", state: "idle", done: 12, total: 12, failed: 0, job: null },
        { name: "summarize", state: "running", done: 7, total: 12, failed: 0, job: "everyday" },
        { name: "link", state: "idle", done: 7, total: 12, failed: 0, job: "everyday" },
      ],
    },
    {
      kind: "entities",
      state: "idle",
      done: 21,
      total: 21,
      blocked: 0,
      steps: [
        { name: "detect", state: "idle", done: 21, total: 21, failed: 0, job: "everyday" },
        { name: "confirm", state: "idle", done: 9, total: 21, failed: 0, job: null },
      ],
    },
    {
      kind: "pages",
      state: "idle",
      done: 3,
      total: 3,
      blocked: 0,
      steps: [
        { name: "write", state: "idle", done: 3, total: 3, failed: 0, job: "synthesis" },
      ],
    },
  ],
  everyday: { job: "everyday", lane: "on_device", model: "Qwen3 4B", mode: "auto", available: true },
  synthesis: { job: "synthesis", lane: "anthropic", model: "Claude Sonnet", mode: "pinned", available: true },
};

/** The shared mock answers `get_activity` with an idle, empty shape so every
 *  other spec gets a quiet line. This layers the busy fixture over it. */
async function installActivityFixture(page: Page, activity: unknown): Promise<void> {
  await page.addInitScript((fixture) => {
    const internals = window.__TAURI_INTERNALS__;
    if (!internals) return;
    const orig = internals.invoke.bind(internals);
    internals.invoke = (async (command: string, args?: unknown) => {
      if (command === "get_activity") return fixture;
      return orig(command, args);
    }) as typeof internals.invoke;
  }, activity);
}

/** Settings swaps the main sidebar for its own, which is a plain aside with
 *  no drawer behaviour, so this reads the first aside either way. */
async function openSidebar(page: Page): Promise<void> {
  const aside = page.locator("aside").first();
  await expect(aside).toBeAttached();
  if ((await aside.getAttribute("aria-hidden")) === "true") {
    await page.locator('[data-sidebar-toggle="true"]').click();
    await expect(aside).toHaveAttribute("aria-hidden", "false");
  }
}

async function goToView(page: Page, view: string): Promise<void> {
  await openSidebar(page);
  await page
    .getByRole("navigation", { name: "Primary navigation" })
    .getByRole("button", { name: view, exact: true })
    .click();
}

for (const width of WIDTHS) {
  test(`status line rides every view at ${width}px`, async ({ page }) => {
    const errors = collectBrowserErrors(page);
    await page.setViewportSize({ width, height: 900 });
    await installTauriMock(page, { locale: "en", rawActions: [], memories: [] });
    await installActivityFixture(page, ACTIVITY_FIXTURE);
    await page.goto("/");

    for (const view of VIEWS) {
      await goToView(page, view);
      // Navigating from the drawer closes it, so ask for it again.
      await openSidebar(page);

      const line = page.getByTestId("activity-status");
      await expect(line).toBeVisible();
      await expect(line).toHaveAttribute("data-state", "organizing");
      // Memories is the busiest asset: 5 left against 0 everywhere else.
      await expect(page.getByTestId("activity-status-detail")).toHaveText("7/12");

      // The line sits at the bottom: nothing inside the sidebar is below it.
      const isLast = await line.evaluate((node) => {
        const sidebar = node.closest("aside");
        if (!sidebar) return false;
        const box = node.getBoundingClientRect();
        return [...sidebar.querySelectorAll("*")].every((other) => {
          const rect = other.getBoundingClientRect();
          return rect.height === 0 || node.contains(other) || rect.bottom <= box.bottom + 1;
        });
      });
      expect(isLast, `status line is the bottom-most element on ${view}`).toBe(true);

      // A status line that widens the sidebar would push the whole shell
      // sideways, which is the failure mode worth guarding at 375px.
      const overflow = await page
        .locator("aside.memory-sidebar")
        .evaluate((node) => node.scrollWidth - node.clientWidth);
      expect(overflow, `sidebar overflow on ${view}`).toBeLessThanOrEqual(1);
      const documentOverflow = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      expect(documentOverflow, `document overflow on ${view}`).toBeLessThanOrEqual(1);
    }

    // Settings is reached from the account menu, not the primary navigation.
    await openSidebar(page);
    await page.getByRole("button", { name: /account menu/i }).click();
    await page.getByRole("menuitem", { name: "Settings" }).click();
    await openSidebar(page);
    await expect(page.getByTestId("activity-status")).toBeVisible();

    expect(errors.pageErrors).toEqual([]);
    expect(errors.consoleErrors).toEqual([]);
  });
}

test("the popover opens by click and by keyboard, and gives focus back", async ({ page }) => {
  const errors = collectBrowserErrors(page);
  // Desktop width on purpose: below 900px the drawer owns Escape, and this
  // spec is about the popover's own keyboard contract.
  await page.setViewportSize({ width: 1280, height: 900 });
  await installTauriMock(page, { locale: "en", rawActions: [], memories: [] });
  await installActivityFixture(page, ACTIVITY_FIXTURE);
  await page.goto("/");

  const trigger = page.getByTestId("activity-status");
  await expect(trigger).toHaveAttribute("aria-expanded", "false");

  // ── Click opens it, with all three assets and their real sentences ──
  await trigger.click();
  const popover = page.getByRole("dialog", { name: "Background activity" });
  await expect(popover).toBeVisible();
  await expect(trigger).toHaveAttribute("aria-expanded", "true");
  await expect(popover.getByTestId("activity-summary-headline")).toHaveText(
    "Wenlan is organizing what you have given it.",
  );
  await expect(popover.getByTestId("activity-asset-memories")).toContainText(
    "7 of 12 summarized and linked",
  );
  await expect(popover.getByTestId("activity-asset-entities")).toContainText(
    "21 found, 9 confirmed in the Wiki",
  );
  await expect(popover.getByTestId("activity-asset-pages")).toContainText(
    "3 pages written, all current",
  );
  // Synthesis resolves to Anthropic, so the trust line must name it rather
  // than claim the machine keeps everything.
  await expect(popover.getByTestId("activity-summary-trust")).toContainText("Anthropic");
  await expect(popover.getByTestId("activity-summary-trust")).not.toContainText(
    "Nothing leaves your device",
  );

  // ── Escape closes it and focus lands back on the trigger ──
  await page.keyboard.press("Escape");
  await expect(popover).toBeHidden();
  await expect(trigger).toHaveAttribute("aria-expanded", "false");
  await expect(trigger).toBeFocused();

  // ── Enter on the focused trigger opens it again ──
  await page.keyboard.press("Enter");
  await expect(page.getByRole("dialog", { name: "Background activity" })).toBeVisible();

  // ── Open Activity navigates and closes the popover behind it ──
  await page.getByTestId("activity-summary-open").click();
  await expect(page.getByRole("dialog", { name: "Background activity" })).toBeHidden();
  await expect(page.getByTestId("activity-now")).toBeVisible();

  expect(errors.pageErrors).toEqual([]);
  expect(errors.consoleErrors).toEqual([]);
});

test("the popover is reachable and readable from the keyboard alone", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  await installTauriMock(page, { locale: "en", rawActions: [], memories: [] });
  await installActivityFixture(page, ACTIVITY_FIXTURE);
  await page.goto("/");

  const trigger = page.getByTestId("activity-status");
  await trigger.focus();
  await expect(trigger).toBeFocused();
  await page.keyboard.press("Enter");

  const popover = page.getByRole("dialog", { name: "Background activity" });
  await expect(popover).toBeVisible();

  // Structure, not styling: a dialog with an accessible name, an action that
  // is a real button, and decorative swatches hidden from the tree.
  await expect(popover).toHaveAttribute("aria-label", "Background activity");
  await expect(popover.getByRole("button", { name: "Open Activity" })).toBeVisible();
  const unlabelledImages = await popover.evaluate((node) =>
    [...node.querySelectorAll("svg, img")].filter(
      (element) =>
        element.getAttribute("aria-hidden") !== "true" &&
        !element.getAttribute("aria-label") &&
        !element.getAttribute("alt"),
    ).length,
  );
  expect(unlabelledImages).toBe(0);

  // The popover renders ABOVE the line and before it in the DOM, so its
  // action is the previous stop in the tab order, not the next one. Tabbing
  // backwards from the trigger has to land inside the popover rather than
  // skipping over it into the rest of the sidebar.
  await page.keyboard.press("Shift+Tab");
  await expect(popover.getByTestId("activity-summary-open")).toBeFocused();
});
