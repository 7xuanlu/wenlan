// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test, type Page } from "@playwright/test";
import { collectBrowserErrors, installTauriMock } from "./tauriMock";
import { createSpacesNavigationFixture } from "./fixtures/spacesNavigation";
import { openTopicMenu } from "./helpers/topicTools";

async function capture(page: Page, name: string) {
  if (!process.env.WENLAN_UI_EVIDENCE_DIR) return;
  await page.evaluate(async () => {
    await document.fonts.ready;
    await Promise.all(document.getAnimations()
      .filter(animation => Number.isFinite(animation.effect?.getComputedTiming().endTime))
      .map(animation => animation.finished.catch(() => undefined)));
  });
  await page.screenshot({ path: `${process.env.WENLAN_UI_EVIDENCE_DIR}/topics-browse-${name}.png` });
}

const cases = [
  { locale: "en", theme: "light", more: "More", title: "Topics", search: "Search topics", openAda: "Ada Lovelace", options: "Topic options", archived: "View archived topics", archivedTitle: "Archived topics", confirm: "Confirm topic", archive: "Archive", restore: "Restore", back: "Back", edit: "Edit note", noteConfirm: "Mark note confirmed" },
  { locale: "zh-Hant", theme: "dark", more: "更多", title: "主題", search: "搜尋主題", openAda: "Ada Lovelace", options: "主題選項", archived: "查看已歸檔主題", archivedTitle: "已歸檔的主題", confirm: "確認主題", archive: "歸檔", restore: "復原", back: "返回", edit: "編輯筆記", noteConfirm: "標記筆記為已確認" },
] as const;

for (const copy of cases) test(`${copy.locale} topics browse before managing`, async ({ page }) => {
  const errors = collectBrowserErrors(page);
  const controller = await installTauriMock(page, { fixture: createSpacesNavigationFixture(), locale: copy.locale, rawActions: [], localStorage: { "wenlan-theme": copy.theme } });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/");
  await page.getByRole("button", { name: copy.more, exact: true }).click();
  await page.getByRole("button", { name: copy.title, exact: true }).click();
  const overview = page.locator(".entities-view");
  await expect(overview.getByRole("button", { name: copy.openAda, exact: true })).toBeVisible();
  await expect(overview.getByText("Charles Babbage", { exact: true })).toBeVisible();
  await expect(overview.getByRole("tablist")).toHaveCount(0);
  await expect(overview.getByRole("checkbox")).toHaveCount(0);
  await expect(overview.getByRole("button", { name: copy.archive, exact: true })).toHaveCount(0);
  expect((await overview.getByRole("searchbox", { name: copy.search }).boundingBox())!.y).toBeLessThan(215);
  await capture(page, `${copy.locale}-${copy.theme}-desktop`);
  await page.getByTestId("asset-lens-rows").click();
  await expect(overview.getByRole("table")).toBeVisible();
  await capture(page, `${copy.locale}-${copy.theme}-rows`);
  await page.getByTestId("asset-lens-cards").click();
  await page.setViewportSize({ width: 375, height: 812 });
  await capture(page, `${copy.locale}-${copy.theme}-narrow`);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(375);
  await page.setViewportSize({ width: 1280, height: 800 });
  await overview.getByRole("searchbox", { name: copy.search }).fill("Ada");
  await expect(overview.getByRole("article")).toHaveCount(1);
  await overview.getByRole("button", { name: copy.openAda, exact: true }).click();
  await expect(page.getByRole("heading", { name: "Ada Lovelace", exact: true })).toBeVisible();
  await expect(page.locator(".notes-workspace-sidebar")).toHaveCSS("width", "48px");
  await expect(page.getByRole("button", { name: copy.more, exact: true })).toHaveAttribute("aria-current", "page");
  expect(controller.calls().filter(call => /^(confirm_entity|archive_entities|restore_entities)_cmd$/.test(call.command))).toEqual([]);
  let menu = await openTopicMenu(page, copy.locale);
  await expect(menu.getByRole("menuitemcheckbox", { name: copy.confirm, exact: true })).toHaveAttribute("aria-checked", "false");
  await page.keyboard.press("Escape");
  await capture(page, `${copy.locale}-${copy.theme}-detail`);

  await page.getByRole("button", { name: "Wrote the first published algorithm", exact: true }).click();
  await page.getByRole("textbox", { name: copy.edit, exact: true }).fill("A revised note about Ada");
  await page.getByRole("textbox", { name: copy.edit, exact: true }).press("Enter");
  await expect(page.getByRole("button", { name: "A revised note about Ada", exact: true })).toBeVisible();
  menu = await openTopicMenu(page, copy.locale);
  await menu.getByRole("menuitemcheckbox", { name: copy.confirm, exact: true }).click();
  await expect.poll(() => controller.calls().filter(call => call.command === "confirm_entity_cmd").length).toBe(1);
  menu = await openTopicMenu(page, copy.locale);
  await menu.getByRole("menuitem", { name: copy.archive, exact: true }).click();
  menu = await openTopicMenu(page, copy.locale);
  await expect(menu.getByRole("menuitem", { name: copy.restore, exact: true })).toBeVisible();
  await expect(menu.getByRole("menuitemcheckbox")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: "A revised note about Ada", exact: true })).toBeDisabled();
  await expect(page.getByRole("button", { name: copy.noteConfirm, exact: true })).toBeDisabled();
  await page.getByRole("button", { name: copy.back, exact: true }).click();
  await expect(overview.getByRole("button", { name: copy.openAda, exact: true })).toHaveCount(0);
  await overview.getByRole("button", { name: copy.options, exact: true }).click();
  await page.getByRole("menuitem", { name: copy.archived, exact: true }).click();
  await expect(page.getByRole("heading", { name: copy.archivedTitle, exact: true })).toBeVisible();
  await expect(overview.getByRole("button", { name: copy.openAda, exact: true })).toBeVisible();
  await capture(page, `${copy.locale}-${copy.theme}-archive`);
  await overview.getByRole("button", { name: copy.openAda, exact: true }).click();
  menu = await openTopicMenu(page, copy.locale);
  await menu.getByRole("menuitem", { name: copy.restore, exact: true }).click();
  menu = await openTopicMenu(page, copy.locale);
  await expect(menu.getByRole("menuitem", { name: copy.archive, exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: "A revised note about Ada", exact: true })).toBeEnabled();
  await page.getByRole("button", { name: copy.back, exact: true }).click();
  await expect(overview.getByRole("button", { name: copy.openAda, exact: true })).toBeVisible();
  expect(controller.calls().filter(call => call.command === "archive_entities_cmd").map(call => call.args)).toEqual([{ req: { ids: ["entity-ada"], dry_run: false } }]);
  expect(errors.pageErrors).toEqual([]); expect(errors.consoleErrors).toEqual([]);
});
