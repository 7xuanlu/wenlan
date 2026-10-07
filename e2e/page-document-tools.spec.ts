// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test, type Page } from "@playwright/test";
import { collectBrowserErrors, installTauriMock } from "./tauriMock";
import { openPageTool } from "./helpers/pageTools";

const copies = [
  { locale: "en", open: "Open Fixture architecture", editor: "Page editor", actions: "Page actions", info: "Page info", map: "Mind map", note: "Note", close: "Close" },
  { locale: "zh-Hant", open: "開啟 Fixture architecture", editor: "頁面編輯器", actions: "頁面操作", info: "頁面資訊", map: "心智圖", note: "筆記", close: "關閉" },
  { locale: "zh-Hans", open: "打开 Fixture architecture", editor: "页面编辑器", actions: "页面操作", info: "页面信息", map: "思维导图", note: "笔记", close: "关闭" },
] as const;

async function expectDocumentToolsAligned(page: Page, actionsName: string): Promise<void> {
  const actions = page.getByRole("button", { name: actionsName, exact: true });
  await expect(actions).toBeInViewport();
  // The matching H1 is rendered by the writing editor; other documents use
  // the outer title. Both place the overflow trigger alongside that title.
  const title = page.locator(
    ".page-detail-title:visible, .page-document-editor--title .cm-content > .cm-line:first-child",
  );
  await expect(title).toHaveCount(1);
  const titleBox = (await title.boundingBox())!;
  const actionsBox = (await actions.boundingBox())!;
  expect(Math.abs(actionsBox.y - titleBox.y)).toBeLessThanOrEqual(12);
  expect(actionsBox.x).toBeGreaterThan(titleBox.x);
  expect(actionsBox.x + actionsBox.width).toBeLessThanOrEqual(
    await page.evaluate(() => innerWidth),
  );
}

for (const copy of copies) for (const width of [1280, 375]) {
  test(`document tools stay in the title menu: ${copy.locale} ${width}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width, height: 900 });
    const errors = collectBrowserErrors(page);
    const controller = await installTauriMock(page, { locale: copy.locale, rawActions: [] });
    await page.goto("/");
    await page.locator("main").getByRole("button", { name: copy.open, exact: true }).click();
    const editor = page.getByRole("textbox", { name: copy.editor, exact: true });
    const actions = page.getByRole("button", { name: copy.actions, exact: true });
    const role = width >= 1100 ? "complementary" : "dialog";
    await expect(editor).toBeEditable();
    for (const name of [copy.note, copy.map, copy.info]) {
      await expect(page.getByRole("button", { name, exact: true })).toHaveCount(0);
    }
    await expect(page.locator(".page-detail-top-row, .page-detail-view-controls")).toHaveCount(0);
    await expectDocumentToolsAligned(page, copy.actions);
    await page.screenshot({ path: testInfo.outputPath("note-title-tools.png") });

    await openPageTool(page, copy.info, copy.actions);
    const info = page.getByRole(role, { name: copy.info, exact: true });
    await expect(info).toBeVisible();
    await expect(editor).toBeEditable();
    await info.getByRole("button", { name: copy.close, exact: true }).press("Escape");
    await expect(info).toHaveCount(0);
    await expect(actions).toBeFocused();
    await expect(editor).toBeEditable();
    expect(controller.calls().filter((call) => call.command === "update_page")).toHaveLength(0);

    const draft = "# Fixture architecture\n\nSaved before the map pane opens.";
    await editor.fill(draft);
    await actions.press("Enter");
    await page.getByRole("menuitem", { name: copy.map, exact: true }).press("Enter");
    const map = page.getByRole(role, { name: copy.map, exact: true });
    await expect(map).toBeVisible();
    await expect(map.locator(".page-canvas")).toBeVisible();
    await expect(editor).toHaveCount(0);
    const reading = page.getByTestId("page-document-reading");
    await expect(reading).toContainText("Saved before the map pane opens.");
    await expect(reading.locator('[contenteditable="true"], textarea')).toHaveCount(0);
    const calls = controller.calls();
    const writeIndex = calls.findIndex((call) => call.command === "update_page");
    const mapIndex = calls.findIndex((call) => call.command === "get_page_map");
    expect(writeIndex).toBeGreaterThanOrEqual(0);
    expect(mapIndex).toBeGreaterThan(writeIndex);
    expect(calls[writeIndex].args).toMatchObject({ content: draft });
    const saved = await page.evaluate(async () =>
      window.__wenlanTauriInvoke("get_page", { id: "page-architecture" }),
    );
    expect(saved).toMatchObject({ content: draft });
    if (width >= 1100) {
      const mapBox = (await map.boundingBox())!;
      const noteBox = (await page.locator(".page-detail-document").boundingBox())!;
      expect(mapBox.width).toBeGreaterThanOrEqual(340);
      expect(mapBox.width).toBeLessThan(noteBox.width);
      expect(mapBox.x).toBeGreaterThanOrEqual(noteBox.x + noteBox.width - 1);
    } else {
      await expect(map).toHaveAttribute("aria-modal", "true");
    }
    await page.screenshot({ path: testInfo.outputPath("map-pane-and-note.png") });
    await map.getByRole("button", { name: copy.close, exact: true }).click();
    await expect(map).toHaveCount(0);
    await expect(editor).toBeEditable();
    await expect(editor).toContainText("Saved before the map pane opens.");
    await expect(editor).toBeFocused();
    expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
    expect(errors.pageErrors).toEqual([]);
  });
}

test("a failed flush keeps the newest draft editable and never opens the map writer", async ({ page }) => {
  const controller = await installTauriMock(page, {
    locale: "en",
    rawActions: [],
    failures: [{ command: "update_page", message: "save unavailable", times: 99 }],
  });
  await page.goto("/");
  await page.locator("main").getByRole("button", { name: "Open Fixture architecture", exact: true }).click();
  const editor = page.getByRole("textbox", { name: "Page editor", exact: true });
  await expect(editor).toBeEditable();
  const draft = "# Fixture architecture\n\nKeep the unsaved draft after the failed map transition.";
  await editor.fill(draft);
  await openPageTool(page, "Mind map");
  await expect(page.locator(".page-editor-notice[role=alert]")).toBeVisible();
  await expect(editor).toBeEditable();
  await expect(editor).toContainText("Keep the unsaved draft after the failed map transition.");
  await expect(page.locator(".page-canvas")).toHaveCount(0);
  expect(controller.calls().filter((call) => call.command === "get_page_map")).toHaveLength(0);
  const saved = await page.evaluate(async () =>
    window.__wenlanTauriInvoke("get_page", { id: "page-architecture" }),
  );
  expect(saved).not.toMatchObject({ content: draft });
});

test("the note and its action menu do not send shortcuts to the open map", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  const controller = await installTauriMock(page, { locale: "en", rawActions: [] });
  await page.goto("/");
  await page.locator("main").getByRole("button", { name: "Open Fixture architecture", exact: true }).click();
  await openPageTool(page, "Mind map");
  const map = page.getByRole("complementary", { name: "Mind map", exact: true });
  const node = map.locator(".react-flow__node").filter({ hasText: "Storage layer" }).first();
  await node.click();
  await expect(map.locator(".react-flow__node.selected")).toHaveCount(1);
  const writes = () => controller.calls().filter((call) =>
    ["put_page_map_layout", "patch_page_map_node", "delete_page_map_node", "create_page_map_node"].includes(call.command),
  ).length;
  expect(writes()).toBe(0); // Opening and selecting an automatic layout must not persist it.
  const before = writes();
  const actions = page.getByRole("button", { name: "Page actions", exact: true });
  await actions.click();
  const menu = page.getByRole("menu", { name: "Page actions", exact: true });
  await expect(menu.getByRole("menuitem", { name: "Page info", exact: true })).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(menu.getByRole("menuitem", { name: "Mind map", exact: true })).toBeFocused();
  await page.keyboard.press("Delete");
  await page.keyboard.press("ControlOrMeta+a");
  await expect(node).toBeVisible();
  await expect(map.locator(".react-flow__node.selected")).toHaveCount(1);
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(actions).toBeFocused();
  await expect(map).toBeVisible();
  await expect(map.locator(".react-flow__node.selected")).toHaveCount(1);
  await page.getByTestId("page-document-reading").click();
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.press("Delete");
  await expect(node).toBeVisible();
  await expect(map.locator(".react-flow__node.selected")).toHaveCount(1);
  await page.waitForTimeout(700); // Covers the map layout write debounce.
  expect(writes()).toBe(before);

  // Pointer reentry restores map keyboard ownership after using the note.
  await node.click();
  await page.keyboard.press("ArrowRight");
  await expect.poll(writes).toBeGreaterThan(before);
});

test("docked tools resize the complete workspace and keep search usable across the pane breakpoint", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 900 });
  const errors = collectBrowserErrors(page);
  await installTauriMock(page, { locale: "en", rawActions: [] });
  await page.goto("/");
  await page.locator("main").getByRole("button", { name: "Open Fixture architecture", exact: true }).click();
  await openPageTool(page, "Mind map");
  const pane = page.getByRole("complementary", { name: "Mind map", exact: true });
  const header = page.locator(".memory-workspace-header");
  const search = header.getByRole("textbox");
  await expect(pane).toBeVisible();
  const panelBox = (await pane.boundingBox())!;
  const headerBox = (await header.boundingBox())!;
  expect(panelBox.y).toBeGreaterThan(0);
  expect(panelBox.y).toBeLessThan(headerBox.height);
  expect(panelBox.x).toBeGreaterThanOrEqual(headerBox.x + headerBox.width);
  expect(panelBox.y + panelBox.height).toBeLessThan(900);
  await expect(search).toBeHidden();
  await header.getByRole("button", { name: "Search", exact: true }).click();
  await expect(search).toBeFocused();
  await search.press("Escape");
  await expect(search).toBeHidden();
  await expect(pane).toBeVisible();

  await page.setViewportSize({ width: 1099, height: 900 });
  const modal = page.getByRole("dialog", { name: "Mind map", exact: true });
  await expect(modal).toBeVisible();
  expect((await header.boundingBox())!.width).toBe(1099);
  await expect.poll(() => modal.evaluate(node => node.contains(document.activeElement))).toBe(true);
  await page.keyboard.press("Escape");
  await expect(modal).toHaveCount(0);
  await expect(page.getByRole("textbox", { name: "Page editor", exact: true })).toBeFocused();

  await page.setViewportSize({ width: 1280, height: 900 });
  await expect(search).toBeVisible();
  expect((await header.boundingBox())!.width).toBe(1280);
  expect(errors.pageErrors).toEqual([]);
  expect(errors.consoleErrors).toEqual([]);
});
