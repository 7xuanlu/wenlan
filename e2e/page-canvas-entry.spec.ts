// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test } from "@playwright/test";
import { installTauriMock, collectBrowserErrors } from "./tauriMock";
import { openPrimaryDestination } from "./helpers/primaryNavigation";
import { openWikiNote } from "./helpers/wikiWorkspace";
import { openPageTool } from "./helpers/pageTools";

const scenarios = [
  { locale: "en", theme: "light", width: 1280, open: "Fixture architecture", editor: "Page editor", actions: "Page actions", sidebarOpen: "Open note sidebar", sidebarClose: "Close note sidebar", inspector: "Note views", info: "Info", map: "Mind map", close: "Close" },
  { locale: "zh-Hant", theme: "dark", width: 375, open: "Fixture architecture", editor: "頁面編輯器", actions: "頁面操作", sidebarOpen: "開啟筆記側欄", sidebarClose: "關閉筆記側欄", inspector: "筆記檢視", info: "資料", map: "心智圖", close: "關閉" },
  { locale: "zh-Hans", theme: "light", width: 768, open: "Fixture architecture", editor: "页面编辑器", actions: "页面操作", sidebarOpen: "打开笔记侧栏", sidebarClose: "关闭笔记侧栏", inspector: "笔记检视", info: "资料", map: "思维导图", close: "关闭" },
] as const;

for (const scenario of scenarios) {
  test(`mind map stays reachable while writing: ${scenario.locale} ${scenario.width}`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: scenario.width, height: 800 });
    const errors = collectBrowserErrors(page);
    const controller = await installTauriMock(page, {
      locale: scenario.locale,
      rawActions: [],
      localStorage: { "wenlan-theme": scenario.theme },
    });
    await page.goto("/");
    await openPrimaryDestination(page, "Wiki");
    await openWikiNote(page, scenario.open);
    const note = page.locator('[data-note-group-id="primary"]');
    const editor = note.getByRole("textbox", { name: scenario.editor, exact: true });
    await expect(editor).toBeEditable();
    const sidebarOpen = note.getByRole("button", { name: scenario.sidebarOpen, exact: true });
    await expect(sidebarOpen).toBeInViewport();
    await expect(note.getByRole("tab", { name: scenario.map, exact: true })).toHaveCount(0);
    await expect(note.locator(".page-detail-top-row, .page-detail-canvas-toggle")).toHaveCount(0);

    await openPageTool(page, scenario.info, scenario.actions);
    const sidebarClose = note.getByRole("button", { name: scenario.sidebarClose, exact: true });
    await expect(sidebarClose).toHaveAttribute("aria-expanded", "true");
    const infoTab = note.getByRole("tab", { name: scenario.info, exact: true });
    const entry = note.getByRole("tab", { name: scenario.map, exact: true });
    await expect(note.getByRole("complementary", { name: scenario.inspector, exact: true })).toBeVisible();
    await expect(infoTab).toBeVisible();
    await expect(infoTab).toHaveAttribute("aria-selected", "true");
    await expect(entry).toBeVisible();
    await expect(entry).toHaveAttribute("aria-selected", "false");
    await expect(entry).toBeInViewport();
    const bounds = await entry.boundingBox();
    expect(bounds).not.toBeNull();
    expect(bounds!.x).toBeGreaterThanOrEqual(0);
    expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(scenario.width);
    await page.screenshot({ path: testInfo.outputPath("entry.png") });

    const draft = "# Fixture architecture\n\nKeep this thought when switching to the mind map.";
    const commandsBeforeDraft = controller.calls().length;
    await editor.fill(draft);
    await entry.press("Enter");
    await expect(entry).toHaveAttribute("aria-selected", "true");
    await expect(note.locator(".page-canvas-surface")).toBeFocused();
    await expect(note.locator(".page-canvas")).toBeVisible();
    await expect(editor).toHaveCount(0);
    const commands = controller.calls().slice(commandsBeforeDraft);
    const writeIndex = commands.findIndex((call) => call.command === "update_page");
    const mapIndex = commands.findIndex((call) => call.command === "get_page_map");
    expect(writeIndex).toBeGreaterThanOrEqual(0);
    expect(mapIndex).toBeGreaterThan(writeIndex);
    expect(commands[writeIndex].args).toMatchObject({ content: draft });
    const saved = await page.evaluate(async () =>
      window.__wenlanTauriInvoke("get_page", { id: "page-architecture" }),
    );
    expect(saved).toMatchObject({ content: draft });
    const pane = note.getByRole("complementary", { name: scenario.inspector, exact: true });
    await expect(pane).toBeVisible();
    await expect(page.getByTestId("page-document-reading")).toContainText("Keep this thought when switching to the mind map.");
    await expect(pane.getByRole("button", { name: scenario.close, exact: true })).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath("map.png") });

    await pane.getByRole("button", { name: scenario.close, exact: true }).press("Enter");
    await expect(pane).toHaveCount(0);
    await expect(editor).toBeEditable();
    await expect(editor).toContainText("Keep this thought when switching to the mind map.");
    await expect(note.getByRole("button", { name: scenario.sidebarOpen, exact: true })).toHaveAttribute("aria-expanded", "false");
    await expect(editor).toBeFocused();
    expect(errors.pageErrors).toEqual([]);
    expect(errors.consoleErrors).toEqual([]);
  });
}
