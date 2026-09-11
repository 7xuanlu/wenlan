// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test, type Page } from "@playwright/test";
import { collectBrowserErrors, installTauriMock } from "./tauriMock";

async function openEntities(page: Page): Promise<void> {
  await page.goto("/");
  // Narrow viewports collapse the navigation behind the sidebar toggle.
  if ((page.viewportSize()?.width ?? 0) < 900) {
    await page.getByTitle("Show sidebar").click();
  }
  const navigation = page.getByRole("navigation", { name: "Primary navigation" });
  await navigation.getByRole("button", { name: "Entities", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Entities" })).toBeVisible();
  if ((page.viewportSize()?.width ?? 0) < 900) await page.waitForTimeout(250);
}

test("archives every detected entity matching the current filter, then restores it back to Detected", async ({ page }) => {
  const browserErrors = collectBrowserErrors(page);
  await installTauriMock(page, {
    locale: "en",
    rawActions: [],
    localStorage: { "wenlan-entities-view-mode": "rows" },
  });
  await openEntities(page);

  // The fixture ships exactly one detected entity (Ada Lovelace, never
  // confirmed) alongside six already-confirmed ones, so Detected starts at
  // one row and no filter needs to be set for "all matching" to mean "all".
  await expect(page.getByRole("tab", { name: "Detected" })).toBeVisible();
  await expect(page.getByRole("cell", { name: "Ada Lovelace", exact: true })).toBeVisible();

  await page.getByRole("button", { name: "Archive all matching" }).click();

  const archiveDialog = page.getByRole("dialog");
  await expect(archiveDialog.getByText("Archive 1 detected entity?")).toBeVisible();
  await expect(archiveDialog.getByText("Filter", { exact: true })).toBeVisible();
  await expect(archiveDialog.getByText("Any, any number of memories")).toBeVisible();
  await expect(
    archiveDialog.getByText("Archived entities can be restored from the Archived tab."),
  ).toBeVisible();

  await archiveDialog.getByRole("button", { name: "Archive", exact: true }).click();

  await expect(archiveDialog).toHaveCount(0);
  await expect(page.getByText("No detected entities match")).toBeVisible();

  await page.getByRole("tab", { name: "Archived" }).click();
  await expect(page.getByRole("cell", { name: "Ada Lovelace", exact: true })).toBeVisible();

  await page.getByRole("row", { name: /Ada Lovelace/ }).getByRole("button", { name: "Restore" }).click();

  // Ada was never confirmed before archiving, so she comes back Detected, not
  // Confirmed -- the exact inverse of the archive, not a reset to a fixed
  // state (crates/wenlan-core/src/db.rs: restore only flips `pages.status`).
  await expect(page.getByText("No archived entities")).toBeVisible();
  await page.getByRole("tab", { name: "Detected" }).click();
  await expect(page.getByRole("cell", { name: "Ada Lovelace", exact: true })).toBeVisible();

  expect(browserErrors.pageErrors).toEqual([]);
  expect(browserErrors.consoleErrors).toEqual([]);
});

test("archives every confirmed entity matching the current filter, then restores them all back to Confirmed", async ({ page }) => {
  const browserErrors = collectBrowserErrors(page);
  await installTauriMock(page, {
    locale: "en",
    rawActions: [],
    localStorage: { "wenlan-entities-view-mode": "rows" },
  });
  await openEntities(page);

  // The fixture ships six confirmed entities (Babbage plus five),
  // so Confirmed starts at six rows with the default unfiltered view.
  await page.getByRole("tab", { name: "Confirmed" }).click();
  await expect(page.getByRole("cell", { name: "Charles Babbage", exact: true })).toBeVisible();

  await page.getByRole("button", { name: "Archive all matching" }).click();

  const archiveDialog = page.getByRole("dialog");
  await expect(archiveDialog.getByText("Archive 6 confirmed entities?")).toBeVisible();
  await expect(archiveDialog.getByText("Filter", { exact: true })).toBeVisible();
  await expect(archiveDialog.getByText("Any, any number of memories")).toBeVisible();
  // Every confirmed fixture row already has a memory, so the dialog warns
  // that archiving takes memories with it.
  await expect(archiveDialog.getByText("Includes", { exact: true })).toBeVisible();
  await expect(
    archiveDialog.getByText("To keep those, set Memories to None first. Archived entities can be restored from the Archived tab."),
  ).toBeVisible();

  await archiveDialog.getByRole("button", { name: "Archive", exact: true }).click();

  await expect(archiveDialog).toHaveCount(0);
  await expect(page.getByText("No confirmed entities yet")).toBeVisible();

  await page.getByRole("tab", { name: "Archived" }).click();
  await expect(page.getByRole("cell", { name: "Charles Babbage", exact: true })).toBeVisible();

  await page.getByRole("button", { name: "Restore all" }).click();

  // All six were confirmed before archiving, so they all come back
  // Confirmed -- the exact inverse of the archive.
  await expect(page.getByText("No archived entities")).toBeVisible();
  await page.getByRole("tab", { name: "Confirmed" }).click();
  await expect(page.getByRole("cell", { name: "Charles Babbage", exact: true })).toBeVisible();

  expect(browserErrors.pageErrors).toEqual([]);
  expect(browserErrors.consoleErrors).toEqual([]);
});

test("Entities cards lens stays inside the viewport", async ({ context }) => {
  const sizes = [
    [1487, 1058],
    [1280, 900],
    [768, 900],
    [375, 812],
  ] as const;
  for (const [width, height] of sizes) {
    // A fresh page per width: the Tauri mock binding can only be registered once per page.
    const page = await context.newPage();
    await page.setViewportSize({ width, height });
    const browserErrors = collectBrowserErrors(page);
    // No view-mode seed: entities open in the cards lens by default.
    await installTauriMock(page, { locale: "en", rawActions: [] });
    await openEntities(page);

    await expect(page.getByTestId("entities-cards")).toBeVisible();
    // The fixture ships exactly one detected entity (Ada Lovelace).
    await expect(page.locator(".asset-card")).toHaveCount(1);
    await expect(page.locator(".asset-card.asset-card--detected")).toHaveCount(1);

    const fitsViewport = await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth);
    expect(fitsViewport).toBe(true);
    const cardBoxes = await Promise.all((await page.locator(".asset-card").all()).map((card) => card.boundingBox()));
    expect(cardBoxes).toHaveLength(1);
    for (const box of cardBoxes) {
      expect(box).not.toBeNull();
      expect(box!.x).toBeGreaterThanOrEqual(0);
      expect(box!.x + box!.width).toBeLessThanOrEqual(width);
    }

    await page.getByRole("tab", { name: "Confirmed" }).click();
    await page.getByRole("button", { name: "Open Charles Babbage" }).click();
    await expect(page.getByRole("heading", { level: 1, name: "Charles Babbage" })).toBeVisible();

    await page.screenshot({ path: `.omo/evidence/entities-cards/entities-cards-light-${width}x${height}.png`, fullPage: true });

    expect(browserErrors.pageErrors).toEqual([]);
    expect(browserErrors.consoleErrors).toEqual([]);
    await page.close();
  }
});
