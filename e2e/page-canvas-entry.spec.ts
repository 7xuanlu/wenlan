// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test } from "@playwright/test";
import { installTauriMock, collectBrowserErrors } from "./tauriMock";
import { openPrimaryDestination } from "./helpers/primaryNavigation";

const scenarios = [
  { locale: "en", theme: "light", width: 1280, open: "Open Fixture architecture", editor: "Page editor", actions: "Page actions", map: "Mind map", close: "Close" },
  { locale: "zh-Hant", theme: "dark", width: 375, open: "開啟 Fixture architecture", editor: "頁面編輯器", actions: "頁面操作", map: "心智圖", close: "關閉" },
  { locale: "zh-Hans", theme: "light", width: 768, open: "打开 Fixture architecture", editor: "页面编辑器", actions: "页面操作", map: "思维导图", close: "关闭" },
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
    await page.locator(".wiki-overview").getByRole("button", { name: scenario.open, exact: true }).click();
    const editor = page.getByRole("textbox", { name: scenario.editor, exact: true });
    await expect(editor).toBeEditable();
    const actions = page.getByRole("button", { name: scenario.actions, exact: true });
    await expect(actions).toBeInViewport();
    await expect(page.getByRole("button", { name: scenario.map, exact: true })).toHaveCount(0);
    await expect(page.locator(".page-detail-top-row, .page-detail-canvas-toggle")).toHaveCount(0);
    await actions.press("Enter");
    const entry = page.getByRole("menuitem", { name: scenario.map, exact: true });
    await expect(entry).toBeVisible();
    await expect(entry).toHaveText(scenario.map);
    const bounds = await entry.boundingBox();
    expect(bounds).not.toBeNull();
    expect(bounds!.x).toBeGreaterThanOrEqual(0);
    expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(scenario.width);
    await page.screenshot({ path: testInfo.outputPath("entry.png") });
    await entry.press("Escape");
    await expect(actions).toBeFocused();

    const draft = "# Fixture architecture\n\nKeep this thought when switching to the mind map.";
    await editor.fill(draft);
    await actions.press("Enter");
    await page.getByRole("menuitem", { name: scenario.map, exact: true }).press("Enter");
    await expect(page.locator(".page-canvas")).toBeVisible();
    await expect(editor).toHaveCount(0);
    const commands = controller.calls();
    const writeIndex = commands.findIndex((call) => call.command === "update_page");
    const mapIndex = commands.findIndex((call) => call.command === "get_page_map");
    expect(writeIndex).toBeGreaterThanOrEqual(0);
    expect(mapIndex).toBeGreaterThan(writeIndex);
    const pane = page.getByRole(scenario.width >= 1100 ? "complementary" : "dialog", { name: scenario.map, exact: true });
    await expect(pane).toBeVisible();
    await expect(page.getByTestId("page-document-reading")).toContainText("Keep this thought when switching to the mind map.");
    await expect(pane.getByRole("button", { name: scenario.close, exact: true })).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath("map.png") });

    await pane.getByRole("button", { name: scenario.close, exact: true }).press("Enter");
    await expect(pane).toHaveCount(0);
    await expect(editor).toBeEditable();
    await expect(editor).toContainText("Keep this thought when switching to the mind map.");
    await expect(actions).toBeVisible();
    await expect(editor).toBeFocused();
    expect(errors.pageErrors).toEqual([]);
  });
}
