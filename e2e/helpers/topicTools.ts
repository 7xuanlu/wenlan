// SPDX-License-Identifier: AGPL-3.0-only
import { expect, type Locator, type Page } from "@playwright/test";
import type { SpaceEntityLocale } from "./spaceEntity";

const labels = {
  en: { actions: "Topic actions", context: "Topic context" },
  "zh-Hant": { actions: "主題操作", context: "主題脈絡" },
  "zh-Hans": { actions: "主题操作", context: "主题脉络" },
} as const;

export async function openTopicMenu(page: Page, locale: SpaceEntityLocale = "en"): Promise<Locator> {
  const copy = labels[locale];
  await page.getByRole("button", { name: copy.actions, exact: true }).click();
  const menu = page.getByRole("menu", { name: copy.actions, exact: true });
  await expect(menu).toBeVisible();
  return menu;
}

/** Context is requested explicitly; ordinary topic reading stays uncluttered. */
export async function openTopicContext(page: Page, locale: SpaceEntityLocale = "en"): Promise<Locator> {
  const menu = await openTopicMenu(page, locale);
  await menu.getByRole("menuitem", { name: labels[locale].context, exact: true }).click();
  const pane = page.getByRole((page.viewportSize()?.width ?? 0) >= 1100 ? "complementary" : "dialog", {
    name: labels[locale].context,
    exact: true,
  });
  await expect(pane).toBeVisible();
  return pane;
}
