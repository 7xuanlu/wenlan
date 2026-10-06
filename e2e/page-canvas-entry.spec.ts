// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test } from "@playwright/test";
import { installTauriMock, collectBrowserErrors } from "./tauriMock";
import { openPrimaryDestination } from "./helpers/primaryNavigation";

const scenarios = [
  { locale: "en", theme: "light", width: 1280, open: "Open Fixture architecture", editor: "Page editor", map: "Mind map", back: "Back to note" },
  { locale: "zh-Hant", theme: "dark", width: 375, open: "開啟 Fixture architecture", editor: "頁面編輯器", map: "心智圖", back: "返回筆記" },
  { locale: "zh-Hans", theme: "light", width: 768, open: "打开 Fixture architecture", editor: "页面编辑器", map: "思维导图", back: "返回笔记" },
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
    const entry = page.getByRole("button", { name: scenario.map, exact: true });
    await expect(entry).toBeVisible();
    // The name must be visible text, not only a tooltip on an unfamiliar icon.
    await expect(entry).toHaveText(scenario.map);
    const bounds = await entry.boundingBox();
    expect(bounds).not.toBeNull();
    expect(bounds!.x).toBeGreaterThanOrEqual(0);
    expect(bounds!.x + bounds!.width).toBeLessThanOrEqual(scenario.width);
    await page.screenshot({ path: testInfo.outputPath("entry.png") });

    const draft = "# Fixture architecture\n\nKeep this thought when switching to the mind map.";
    await editor.fill(draft);
    await entry.press("Enter");
    await expect(page.locator(".page-canvas")).toBeVisible();
    await expect(editor).toHaveCount(0);
    const commands = controller.calls();
    const writeIndex = commands.findIndex((call) => call.command === "update_page");
    const mapIndex = commands.findIndex((call) => call.command === "get_page_map");
    expect(writeIndex).toBeGreaterThanOrEqual(0);
    expect(mapIndex).toBeGreaterThan(writeIndex);
    await expect(page.getByRole("button", { name: scenario.back, exact: true })).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath("map.png") });

    await page.getByRole("button", { name: scenario.back, exact: true }).press("Enter");
    await expect(editor).toBeEditable();
    await expect(editor).toContainText("Keep this thought when switching to the mind map.");
    await expect(entry).toBeVisible();
    expect(errors.pageErrors).toEqual([]);
  });
}
