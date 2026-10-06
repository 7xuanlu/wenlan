// SPDX-License-Identifier: AGPL-3.0-only
import { expect, type Locator, type Page } from "@playwright/test";

export type SpaceEntityLocale = "en" | "zh-Hans" | "zh-Hant";

const labels: Record<
  SpaceEntityLocale,
  { readonly keyEntities: string; readonly viewAllEntities: RegExp }
> = {
  en: {
    keyEntities: "Key topics",
    viewAllEntities: /^View all \d+$/,
  },
  "zh-Hans": {
    keyEntities: "主要主题",
    viewAllEntities: /^查看全部 \d+ 个$/,
  },
  "zh-Hant": {
    keyEntities: "主要主題",
    viewAllEntities: /^檢視全部 \d+ 個$/,
  },
};

export async function getSpaceEntityButton(
  page: Page,
  entityName: string,
  locale: SpaceEntityLocale = "en",
): Promise<Locator> {
  const copy = labels[locale];
  const region = page.getByRole("region", {
    name: copy.keyEntities,
    exact: true,
  });
  await expect(region).toBeVisible();
  const disclosure = region.locator("details");
  if (await disclosure.getAttribute("open") === null) {
    await disclosure.locator("summary").click();
  }
  await expect(disclosure).toHaveAttribute("open");

  const entity = region.getByRole("button", {
    name: entityName,
    exact: true,
  });
  const viewAll = region.getByRole("button", {
    name: copy.viewAllEntities,
  });
  if (await entity.isVisible()) {
    return entity;
  }

  await expect(viewAll).toBeVisible();
  await viewAll.click();
  await expect(entity).toBeVisible();
  return entity;
}

export async function openSpaceEntity(
  page: Page,
  entityName: string,
  locale: SpaceEntityLocale = "en",
): Promise<void> {
  await (await getSpaceEntityButton(page, entityName, locale)).click();
}
