// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test } from "@playwright/test";
import { openPrimaryDestination } from "./helpers/primaryNavigation";
import { openSpaceEntity } from "./helpers/spaceEntity";
import { collectBrowserErrors, installTauriMock } from "./tauriMock";

test("Wiki -> Spaces -> Space -> Page -> back and Space -> Entity -> back", async ({ page }) => {
  // Keep the six-row Wiki long enough to exercise real scrolling and reset.
  await page.setViewportSize({ width: 1280, height: 500 });
  // Given a clean fixture and browser error capture.
  const browserErrors = collectBrowserErrors(page);
  // Rows lens: this journey asserts the Wiki page scrolls and resets on
  // navigation, and the default Cards grid fits the fixture inside the viewport.
  await installTauriMock(page, { locale: "en", localStorage: { "wenlan-wiki-view-mode": "rows", "wenlan-spaces-view-mode": "rows" }, rawActions: [] });
  await page.goto("/");

  // When the two primary hierarchy journeys are driven through the rendered shell.
  const primaryNavigation = page.getByRole("navigation", { name: "Primary navigation" });
  await openPrimaryDestination(page, "Wiki");
  await expect(primaryNavigation.locator('[aria-current="page"]')).toHaveCount(1);
  const more = primaryNavigation.getByRole("button", { name: "More", exact: true });
  const wiki = primaryNavigation.getByRole("button", { name: "Wiki", exact: true });
  await expect(wiki).toHaveAttribute("aria-current", "page");
  await expect(more).not.toHaveAttribute("aria-current");
  await more.click();
  await expect(primaryNavigation.locator('[aria-current="page"]')).toHaveCount(1);
  await expect(wiki).toHaveAttribute("aria-current", "page");
  await expect(primaryNavigation.getByRole("button", { name: "Home", exact: true })).toHaveCount(0);
  await expect(more).not.toHaveAttribute("aria-current");
  await page.keyboard.press("Escape");
  await expect(wiki).toHaveAttribute("aria-current", "page");
  await expect(more).not.toHaveAttribute("aria-current");

  await primaryNavigation.getByRole("button", { name: "Wiki", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Wiki" })).toBeVisible();
  await expect(page.locator("main").getByRole("button", { name: "Open Independent research" })).toContainText("Independent");
  await expect(primaryNavigation.getByRole("button", { name: "Wiki" })).toHaveAttribute("aria-current", "page");
  await page.locator("main").getByRole("button", { name: "Open Independent research" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Independent research" })).toBeVisible();
  await expect(primaryNavigation.getByRole("button", { name: "Wiki" })).toHaveAttribute("aria-current", "page");
  await page.locator("main").getByRole("button", { name: "Back", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Wiki" })).toBeVisible();

  const main = page.locator("main");
  await main.evaluate((node) => { node.scrollTop = node.scrollHeight; });
  await expect.poll(() => main.evaluate((node) => node.scrollTop)).toBeGreaterThan(0);

  await primaryNavigation.getByRole("button", { name: "Spaces", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Spaces" })).toBeVisible();
  await expect.poll(() => main.evaluate((node) => node.scrollTop)).toBe(0);
  await expect(page.getByRole("navigation", { name: "Recent spaces" })).toHaveCount(0);
  const wenlanRow = page.getByTestId("space-row-space-wenlan");
  await wenlanRow.getByRole("button", { name: "Wenlan", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Wenlan" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("heading", { level: 1, name: "Spaces" })).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Recent spaces" })).toHaveCount(0);
  const spaces = primaryNavigation.getByRole("button", { name: "Spaces", exact: true });
  await expect(primaryNavigation.locator('[aria-current="page"]')).toHaveCount(1);
  await expect(spaces).toHaveAttribute("aria-current", "page");
  await expect(spaces).not.toHaveAttribute("aria-pressed");
  await wenlanRow.getByRole("button", { name: "Wenlan", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Wenlan" })).toBeVisible();
  await expect(primaryNavigation.locator('[aria-current="page"]')).toHaveCount(1);
  await expect(spaces).toHaveAttribute("aria-current", "page");

  await page.getByRole("button", { name: /Fixture architecture/ }).first().click();
  await expect(page.getByRole("heading", { level: 1, name: "Fixture architecture" })).toBeVisible();
  await page.locator("main").getByRole("button", { name: "Back", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Wenlan" })).toBeVisible();

  await openSpaceEntity(page, "Ada Lovelace");
  await expect(page.getByRole("heading", { level: 1, name: "Ada Lovelace" })).toBeVisible();
  await page.keyboard.press("Escape");

  // Then history returns to the Space and the browser stayed error-free.
  await expect(page.getByRole("heading", { level: 1, name: "Wenlan" })).toBeVisible();
  await expect(primaryNavigation.locator('[aria-current="page"]')).toHaveCount(1);
  await expect(primaryNavigation.getByRole("button", { name: "Spaces", exact: true })).toHaveAttribute("aria-current", "page");
  await expect(page.getByRole("navigation", { name: "Recent spaces" })).toHaveCount(0);
  await expect(spaces).not.toHaveAttribute("aria-pressed");
  expect(browserErrors.pageErrors).toEqual([]);
  expect(browserErrors.consoleErrors).toEqual([]);
});
