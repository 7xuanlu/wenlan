// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test } from "@playwright/test";
import { collectBrowserErrors, installTauriMock } from "./tauriMock";
import { createSpacesNavigationFixture } from "./fixtures/spacesNavigation";
import { openPrimaryDestination } from "./helpers/primaryNavigation";
import { openTopicContext } from "./helpers/topicTools";

const copies = [
  { locale: "en", more: "More", topics: "Topics", actions: "Topic actions", context: "Topic context", close: "Close", expand: "Full screen", edit: "Edit note", confirm: "Confirm topic" },
  { locale: "zh-Hant", more: "更多", topics: "主題", actions: "主題操作", context: "主題脈絡", close: "關閉", expand: "全螢幕檢視", edit: "編輯筆記", confirm: "確認主題" },
  { locale: "zh-Hans", more: "更多", topics: "主题", actions: "主题操作", context: "主题脉络", close: "关闭", expand: "全屏查看", edit: "编辑笔记", confirm: "确认主题" },
] as const;

for (const copy of copies) for (const width of [1280, 375]) {
  test(`topic document tools preserve reading and nested context: ${copy.locale} ${width}`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    const errors = collectBrowserErrors(page);
    const controller = await installTauriMock(page, {
      fixture: createSpacesNavigationFixture(), locale: copy.locale, rawActions: [],
      localStorage: { "wenlan-theme": width === 375 ? "dark" : "light" },
    });
    await page.goto("/");
    await openPrimaryDestination(page, copy.topics, copy.more);
    await page.getByRole("button", { name: "Ada Lovelace", exact: true }).click();
    const title = page.getByRole("heading", { name: "Ada Lovelace", exact: true, level: 1 });
    const actions = page.getByRole("button", { name: copy.actions, exact: true });
    await expect(title).toBeVisible();
    await expect(actions).toBeInViewport();
    const titleBox = (await title.boundingBox())!;
    const actionBox = (await actions.boundingBox())!;
    expect(Math.abs(titleBox.y + titleBox.height / 2 - actionBox.y - actionBox.height / 2)).toBeLessThanOrEqual(4);
    await expect(page.locator(".entity-dossier-seal, .entity-dossier-dateline")).toHaveCount(0);
    await expect(page.locator(".entity-topic-context")).toHaveCount(0);
    await expect(page.getByRole("button", { name: copy.confirm, exact: true })).toHaveCount(0);
    const note = page.getByRole("button", { name: "Wrote the first published algorithm", exact: true });
    await expect(note).toBeVisible();
    await actions.press("ArrowDown");
    await expect(page.getByRole("menuitem", { name: copy.context, exact: true })).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(actions).toBeFocused();

    const pane = await openTopicContext(page, copy.locale);
    const graph = pane.getByRole("button", { name: copy.expand, exact: true });
    await graph.click();
    const overlay = page.getByRole("dialog", { name: copy.expand, exact: true });
    await expect(overlay).toBeVisible();
    await expect(overlay.getByRole("button", { name: copy.close, exact: true })).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(overlay).toHaveCount(0);
    await expect(pane).toBeVisible();
    await expect(graph).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(pane).toHaveCount(0);
    await expect(actions).toBeFocused();
    await expect(title).toBeVisible();
    expect(controller.calls().filter(call => /^(confirm_entity|archive_entities|restore_entities)_cmd$/.test(call.command))).toEqual([]);

    // Preserve the existing direct edit/blur-save contract and same-page draft.
    await note.click();
    const editor = page.getByRole("textbox", { name: copy.edit, exact: true });
    await editor.fill("Topic note saved before opening context.");
    await openTopicContext(page, copy.locale);
    await pane.getByRole("button", { name: copy.close, exact: true }).click();
    await expect(page.getByRole("button", { name: "Topic note saved before opening context.", exact: true })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
    expect(errors.pageErrors).toEqual([]);
    expect(errors.consoleErrors).toEqual([]);
  });
}

for (const width of [1280, 375]) test(`nested Atlas filters remain visible and keep keyboard ownership at ${width}`, async ({ page }) => {
  await page.setViewportSize({ width, height: 900 });
  const errors = collectBrowserErrors(page);
  await installTauriMock(page, { fixture: createSpacesNavigationFixture(), locale: "en", rawActions: [] });
  await page.goto("/");
  await openPrimaryDestination(page, "Topics");
  await page.getByRole("button", { name: "Ada Lovelace", exact: true }).click();
  const context = await openTopicContext(page);
  await context.getByRole("button", { name: "Full screen", exact: true }).click();
  const graph = page.getByRole("dialog", { name: "Full screen", exact: true });
  await graph.getByRole("group", { name: "Graph view", exact: true }).getByRole("button", { name: "Atlas", exact: true }).click();
  const displayTrigger = graph.getByRole("button", { name: "Display", exact: true });
  await displayTrigger.click();
  const display = graph.getByRole("group", { name: "Show in graph", exact: true });
  const filter = display.getByRole("button", { name: "Filter topic types", exact: true });
  await filter.click();
  const types = graph.getByRole("dialog", { name: "Topic types", exact: true });
  await expect(types).toBeVisible();
  await expect(types).toBeFocused();
  const person = types.getByRole("button", { name: "Person", exact: true });
  // Real hit testing verifies the portal is on top, not merely in the DOM.
  expect(await person.evaluate((node) => {
    const box = node.getBoundingClientRect();
    return node.contains(document.elementFromPoint(box.x + box.width / 2, box.y + box.height / 2));
  })).toBe(true);
  await page.keyboard.press("Tab");
  await expect(types.getByRole("button", { name: "Close type filters", exact: true })).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(person).toBeFocused();
  await page.keyboard.press("Space");
  await expect(person).toHaveAttribute("aria-pressed", "false");
  await page.keyboard.press("Escape");
  await expect(types).toHaveCount(0);
  await expect(filter).toBeFocused();
  await expect(display).toBeVisible();
  await page.keyboard.press("Escape");
  // The enclosing full-screen dialog owns Escape once the nested filter has
  // closed, even while Display is still expanded.
  await expect(graph).toHaveCount(0);
  await expect(context.getByRole("button", { name: "Full screen", exact: true })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(context).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Topic actions", exact: true })).toBeFocused();
  expect(errors.pageErrors).toEqual([]);
  expect(errors.consoleErrors).toEqual([]);
});
