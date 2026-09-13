// SPDX-License-Identifier: AGPL-3.0-only
//
// The toolbar Activity button, its status badge, and its summary popover.
//
// Two things are being proved here. First, that the status is actually on
// every page and survives every width the app supports, because a status that
// disappears on one view is worse than none: the user learns not to trust it.
// Toolbar placement is what makes that hold with the sidebar collapsed or
// swapped for Settings. Second, that the popover is operable from the keyboard
// alone and gives focus back when it closes.
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
      done: 9,
      total: 21,
      blocked: 0,
      steps: [
        { name: "detect", state: "idle", done: 12, total: 12, failed: 0, job: "everyday" },
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
  test(`Activity status rides every view at ${width}px`, async ({ page }) => {
    const errors = collectBrowserErrors(page);
    await page.setViewportSize({ width, height: 900 });
    await installTauriMock(page, { locale: "en", rawActions: [], memories: [] });
    await installActivityFixture(page, ACTIVITY_FIXTURE);
    await page.goto("/");

    for (const view of VIEWS) {
      await goToView(page, view);

      const button = page.getByTestId("activity-status");
      await expect(button).toBeVisible();
      await expect(button).toHaveAttribute("data-state", "organizing");
      // State, never a number: the assets count different things, so the
      // badge is a ring while steeping and the word lives in the name.
      await expect(button).toHaveText("Activity");
      await expect(button).toHaveAccessibleName("Activity, Steeping");
      await expect(page.getByTestId("activity-status-dot")).toHaveAttribute(
        "data-dot-state",
        "organizing",
      );

      // The status lives in the top toolbar, fully on screen, and there is
      // exactly one of it: no second copy left behind in a sidebar.
      const placement = await button.evaluate((node) => {
        const box = node.getBoundingClientRect();
        return {
          inHeader: node.closest("header") !== null,
          inSidebar: node.closest("aside") !== null,
          onScreen: box.left >= 0 && box.right <= window.innerWidth && box.top >= 0,
        };
      });
      expect(placement, `status placement on ${view}`).toEqual({
        inHeader: true,
        inSidebar: false,
        onScreen: true,
      });
      await expect(page.getByTestId("activity-status")).toHaveCount(1);

      // A badge that widens the toolbar would push the whole shell sideways,
      // which is the failure mode worth guarding at 375px.
      const documentOverflow = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      expect(documentOverflow, `document overflow on ${view}`).toBeLessThanOrEqual(1);
    }

    // The popover drops from the button and stays inside the window.
    await page.getByTestId("activity-status").click();
    const popover = page.getByRole("dialog", { name: "Background activity" });
    await expect(popover).toBeVisible();
    const popoverBox = await popover.evaluate((node) => {
      const box = node.getBoundingClientRect();
      const trigger = document.querySelector('[data-testid="activity-status"]')!.getBoundingClientRect();
      return { left: box.left, right: box.right, top: box.top, triggerBottom: trigger.bottom };
    });
    expect(popoverBox.left, "popover left edge").toBeGreaterThanOrEqual(0);
    expect(popoverBox.right, "popover right edge").toBeLessThanOrEqual(width);
    expect(popoverBox.top, "popover below the button").toBeGreaterThanOrEqual(popoverBox.triggerBottom);
    await page.keyboard.press("Escape");
    await expect(popover).toBeHidden();

    // Settings swaps the sidebar for its own; the toolbar status stays.
    await openSidebar(page);
    await page.getByRole("button", { name: /account menu/i }).click();
    await page.getByRole("menuitem", { name: "Settings" }).click();
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
    "Wenlan is steeping what you have given it.",
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

  // ── See all activity navigates and closes the popover behind it ──
  await page.getByTestId("activity-summary-open").click();
  await expect(page.getByRole("dialog", { name: "Background activity" })).toBeHidden();
  await expect(page.getByTestId("activity-now")).toBeVisible();
  await expect(trigger).toHaveAttribute("aria-current", "page");

  // ── A click anywhere else closes it ──
  await trigger.click();
  await expect(page.getByRole("dialog", { name: "Background activity" })).toBeVisible();
  await page.mouse.click(640, 700);
  await expect(page.getByRole("dialog", { name: "Background activity" })).toBeHidden();

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
  await expect(popover.getByRole("button", { name: "See all activity" })).toBeVisible();
  const unlabelledImages = await popover.evaluate((node) =>
    [...node.querySelectorAll("svg, img")].filter(
      (element) =>
        element.getAttribute("aria-hidden") !== "true" &&
        !element.getAttribute("aria-label") &&
        !element.getAttribute("alt"),
    ).length,
  );
  expect(unlabelledImages).toBe(0);

  // The popover follows the button in the DOM, so Tab from the trigger has to
  // land inside it rather than skipping over it into the rest of the toolbar.
  // "Turn on a model" is the first stop when a model is missing; this fixture
  // has none, so the first stop is See all activity.
  await page.keyboard.press("Tab");
  await expect(popover.getByTestId("activity-summary-open")).toBeFocused();
});

/** Everyday has no model: memories wait, and so does scanning them for entities. */
const BLOCKED_FIXTURE: ActivityResponse = {
  ...ACTIVITY_FIXTURE,
  state: "blocked",
  assets: [
    {
      kind: "memories",
      state: "blocked",
      done: 0,
      total: 8,
      blocked: 8,
      steps: [
        { name: "store", state: "idle", done: 8, total: 8, failed: 0, job: null },
        { name: "summarize", state: "blocked", done: 0, total: 8, failed: 0, job: "everyday" },
        { name: "link", state: "blocked", done: 0, total: 8, failed: 0, job: "everyday" },
      ],
    },
    {
      kind: "entities",
      state: "blocked",
      done: 0,
      total: 8,
      blocked: 8,
      steps: [
        { name: "detect", state: "blocked", done: 0, total: 8, failed: 0, job: "everyday" },
        { name: "confirm", state: "idle", done: 0, total: 0, failed: 0, job: null },
      ],
    },
    ACTIVITY_FIXTURE.assets[2],
  ],
  everyday: { job: "everyday", lane: "none", model: null, mode: "unconfigured", available: false },
  synthesis: { job: "synthesis", lane: "on_device", model: "Qwen3 8B", mode: "pinned", available: true },
};

test("a missing model leads the popover, in units that match each row", async ({ page }) => {
  const errors = collectBrowserErrors(page);
  await page.setViewportSize({ width: 1280, height: 900 });
  await installTauriMock(page, { locale: "en", rawActions: [], memories: [] });
  await installActivityFixture(page, BLOCKED_FIXTURE);
  await page.goto("/");

  const trigger = page.getByTestId("activity-status");
  await expect(trigger).toHaveAttribute("data-state", "blocked");
  await expect(trigger).toHaveText("Activity");
  await expect(page.getByTestId("activity-status-dot")).toHaveAttribute("data-dot-state", "blocked");

  await trigger.click();
  const popover = page.getByRole("dialog", { name: "Background activity" });
  await expect(popover).toBeVisible();

  // The cause is the first thing in the popover, and it carries the fix.
  const causes = popover.getByTestId("activity-summary-causes");
  await expect(causes).toContainText("Steeping is paused: no everyday model is loaded.");
  expect(await popover.evaluate((node) => node.firstElementChild?.getAttribute("data-testid"))).toBe(
    "activity-summary-causes",
  );
  await expect(popover.getByTestId("activity-summary-headline")).toHaveCount(0);

  // Each row counts its own thing: 8 memories waiting, 0 entities found yet.
  await expect(popover.getByTestId("activity-asset-memories")).toContainText(
    "8 not yet summarized. All are searchable now.",
  );
  await expect(popover.getByTestId("activity-asset-count-entities")).toHaveText("0");
  await expect(popover.getByTestId("activity-asset-entities")).toContainText(
    "8 memories not yet scanned for entities",
  );

  // Tab from the trigger lands on the fix first.
  await trigger.focus();
  await page.keyboard.press("Tab");
  const turnOn = popover.getByRole("button", { name: "Turn on a model" });
  await expect(turnOn).toBeFocused();

  await turnOn.click();
  await expect(popover).toBeHidden();
  await expect(page.getByText("On-device and routed models")).toBeVisible();

  expect(errors.pageErrors).toEqual([]);
  expect(errors.consoleErrors).toEqual([]);
});
