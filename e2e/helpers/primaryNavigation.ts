// SPDX-License-Identifier: AGPL-3.0-only
import { expect, type Page } from "@playwright/test";

/** Follow the same rail-or-More path a user takes after customizing navigation. */
export async function openPrimaryDestination(page: Page, name: string, moreLabel = "More") {
  await expect(page.getByRole("main")).toBeVisible();
  const navigation = page.getByRole("navigation", {
    name: /^(Primary navigation|主要導覽|主导航)$/,
  });
  if (!await navigation.isVisible()) {
    await page.getByRole("button", {
      name: /^(Show sidebar|顯示側邊欄|显示侧边栏)$/,
    }).click();
    await expect(navigation).toBeVisible();
  }
  const destination = navigation.getByRole("button", { name, exact: true });
  if (!await destination.isVisible()) {
    await navigation.getByRole("button", { name: moreLabel, exact: true }).click();
  }
  await destination.click();
}
