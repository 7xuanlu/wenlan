// SPDX-License-Identifier: AGPL-3.0-only
import { expect, type Locator, type Page } from "@playwright/test";
import type { SpaceEntityLocale } from "./spaceEntity";

const labels = {
  en: { actions: "Topic actions", context: "Info" },
  "zh-Hant": { actions: "主題操作", context: "資訊" },
  "zh-Hans": { actions: "主题操作", context: "信息" },
} as const;

export async function openTopicMenu(page: Page, locale: SpaceEntityLocale = "en"): Promise<Locator> {
  const copy = labels[locale];
  await page.getByRole("button", { name: copy.actions, exact: true }).click();
  const menu = page.getByRole("menu", { name: copy.actions, exact: true });
  await expect(menu).toBeVisible();
  return menu;
}

/** Ensure Info is open, including wide views where it opens by default. */
export async function openTopicContext(page: Page, locale: SpaceEntityLocale = "en"): Promise<Locator> {
  const copy = labels[locale];
  const toggle = page.getByRole("button", { name: copy.context, exact: true });
  if ((await toggle.getAttribute("aria-expanded")) !== "true") {
    await toggle.click();
  }
  const pane = page.getByRole((page.viewportSize()?.width ?? 0) >= 1100 ? "complementary" : "dialog", {
    name: labels[locale].context,
    exact: true,
  });
  await expect(pane).toBeVisible();
  return pane;
}
