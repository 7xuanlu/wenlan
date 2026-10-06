// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test, type Page } from "@playwright/test";
import { collectBrowserErrors, installTauriMock } from "./tauriMock";
import { openPrimaryDestination } from "./helpers/primaryNavigation";
import { openTopicMenu } from "./helpers/topicTools";
import type { Entity } from "../src/lib/tauri";

async function openTopics(page: Page): Promise<void> {
  await page.goto("/");
  if ((page.viewportSize()?.width ?? 0) < 900) await page.getByTitle("Show sidebar").click();
  await openPrimaryDestination(page, "Topics");
  await expect(page.getByRole("heading", { level: 1, name: "Topics" })).toBeVisible();
}

async function storedTopics(page: Page): Promise<Entity[]> {
  return page.evaluate(async () => await window.__wenlanTauriInvoke("list_entities_cmd", {}) as Entity[]);
}

for (const confirmed of [false, true]) test(`archives filtered ${confirmed ? "confirmed" : "detected"} topics from detail and restores their original lifecycle`, async ({ page }) => {
  const browserErrors = collectBrowserErrors(page);
  const controller = await installTauriMock(page, { locale: "en", rawActions: [], localStorage: { "wenlan-entities-view-mode": "rows" } });
  await openTopics(page);
  const original = await storedTopics(page);
  const matching = original.filter(entity => entity.confirmed === confirmed);
  expect(matching).toHaveLength(confirmed ? 6 : 1);
  const others = original.filter(entity => entity.confirmed !== confirmed);
  const overview = page.locator(".entities-view");

  // Lifecycle belongs to detail. Each exact-name filter keeps unrelated topics
  // out of the current action, and persisted state proves they remain untouched.
  for (const entity of matching) {
    await overview.getByRole("searchbox", { name: "Search topics" }).fill(entity.name);
    await expect(overview.getByRole("row")).toHaveCount(2);
    await overview.getByRole("button", { name: entity.name, exact: true }).click();
    let menu = await openTopicMenu(page);
    await expect(menu.getByRole("menuitemcheckbox", { name: confirmed ? "Confirmed" : "Confirm topic", exact: true })).toHaveAttribute("aria-checked", String(confirmed));
    await menu.getByRole("menuitem", { name: "Archive", exact: true }).click();
    menu = await openTopicMenu(page);
    await expect(menu.getByRole("menuitem", { name: "Restore", exact: true })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect.poll(async () => (await storedTopics(page)).find(item => item.id === entity.id)?.status).toBe("archived");
    await page.getByRole("button", { name: "Back", exact: true }).click();
    await expect(overview.getByRole("button", { name: entity.name, exact: true })).toHaveCount(0);
  }
  const archived = await storedTopics(page);
  expect(archived.filter(entity => others.some(other => other.id === entity.id))).toEqual(others);
  expect(archived.filter(entity => matching.some(match => match.id === entity.id)).map(entity => ({ ...entity, status: original.find(item => item.id === entity.id)!.status }))).toEqual(matching);

  await overview.getByRole("searchbox", { name: "Search topics" }).fill("");
  await overview.getByRole("button", { name: "Topic options" }).click();
  await page.getByRole("menuitem", { name: "View archived topics" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Archived topics" })).toBeVisible();
  await expect(overview.getByRole("row")).toHaveCount(matching.length + 1);
  for (const [index, entity] of matching.entries()) {
    if (index > 0) {
      await overview.getByRole("button", { name: "Topic options" }).click();
      await page.getByRole("menuitem", { name: "View archived topics" }).click();
    }
    await overview.getByRole("searchbox", { name: "Search topics" }).fill(entity.name);
    await expect(overview.getByRole("row")).toHaveCount(2);
    await overview.getByRole("button", { name: entity.name, exact: true }).click();
    let menu = await openTopicMenu(page);
    await menu.getByRole("menuitem", { name: "Restore", exact: true }).click();
    menu = await openTopicMenu(page);
    await expect(menu.getByRole("menuitem", { name: "Archive", exact: true })).toBeVisible();
    await expect(menu.getByRole("menuitemcheckbox", { name: confirmed ? "Confirmed" : "Confirm topic", exact: true })).toHaveAttribute("aria-checked", String(confirmed));
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "Back", exact: true }).click();
  }
  // A detail return opens active Topics; archived remains accessible from More.
  await overview.getByRole("searchbox", { name: "Search topics" }).fill("");
  await overview.getByRole("button", { name: "Topic options" }).click();
  await page.getByRole("menuitem", { name: "View archived topics" }).click();
  await expect(page.getByText("No archived items", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Back to topics" }).click();
  await expect(overview.getByRole("row")).toHaveCount(original.length + 1);
  expect(await storedTopics(page)).toEqual(original);
  for (const command of ["archive_entities_cmd", "restore_entities_cmd"]) {
    expect(controller.calls().filter(call => call.command === command).map(call => call.args)).toEqual(matching.map(entity => ({ req: { ids: [entity.id], dry_run: false } })));
  }
  expect(browserErrors.pageErrors).toEqual([]);
  expect(browserErrors.consoleErrors).toEqual([]);
});

test("Topics cards lens stays inside the viewport", async ({ context }) => {
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
    await openTopics(page);

    await expect(page.getByTestId("entities-cards")).toBeVisible();
    // The browser combines one detected and six confirmed topics.
    await expect(page.locator(".asset-card")).toHaveCount(7);
    await expect(page.locator(".asset-card.asset-card--detected")).toHaveCount(0);

    const fitsViewport = await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth);
    expect(fitsViewport).toBe(true);
    const cardBoxes = await Promise.all((await page.locator(".asset-card").all()).map((card) => card.boundingBox()));
    expect(cardBoxes).toHaveLength(7);
    for (const box of cardBoxes) {
      expect(box).not.toBeNull();
      expect(box!.x).toBeGreaterThanOrEqual(0);
      expect(box!.x + box!.width).toBeLessThanOrEqual(width);
    }

    await page.getByRole("button", { name: "Charles Babbage", exact: true }).click();
    await expect(page.getByRole("heading", { level: 1, name: "Charles Babbage" })).toBeVisible();

    await page.screenshot({ path: `.omo/evidence/entities-cards/entities-cards-light-${width}x${height}.png`, fullPage: true });

    expect(browserErrors.pageErrors).toEqual([]);
    expect(browserErrors.consoleErrors).toEqual([]);
    await page.close();
  }
});
