// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test, type Page } from "@playwright/test";
import { collectBrowserErrors, installTauriMock } from "./tauriMock";

const languages = [
  { locale: "en", theme: "light", spaces: "Spaces", graph: "Graph", sources: "Sources", more: "More", customize: "Customize navigation", reset: "Reset defaults" },
  { locale: "zh-Hant", theme: "dark", spaces: "空間", graph: "圖譜", sources: "來源", more: "更多", customize: "自訂導覽", reset: "恢復預設" },
  { locale: "zh-Hans", theme: "light", spaces: "空间", graph: "图谱", sources: "来源", more: "更多", customize: "自定义导航", reset: "恢复默认" },
] as const;

async function capture(page: Page, name: string) {
  const directory = process.env.WENLAN_UI_EVIDENCE_DIR;
  if (!directory) return;
  await page.mouse.move(page.viewportSize()!.width - 1, 1);
  await page.evaluate(async () => {
    await document.fonts.ready;
    await Promise.all(document.getAnimations()
      .filter(animation => Number.isFinite(animation.effect?.getComputedTiming().endTime))
      .map(animation => animation.finished.catch(() => undefined)));
  });
  await page.screenshot({ path: `${directory}/navigation-preferences-${name}.png` });
}

async function expectPanelWithinViewport(page: Page) {
  await expect.poll(async () => {
    const panel = await page.locator(".notes-more-panel").boundingBox();
    const viewport = page.viewportSize()!;
    return panel !== null && panel.x >= 7 && panel.y >= 7
      && panel.x + panel.width <= viewport.width - 7
      && panel.y + panel.height <= viewport.height - 7;
  }).toBe(true);
}

for (const copy of languages) {
  test(`${copy.locale} navigation customization persists and stays reachable at every size`, async ({ page }) => {
    const errors = collectBrowserErrors(page);
    await page.setViewportSize({ width: 1280, height: 800 });
    await installTauriMock(page, {
      locale: copy.locale,
      rawActions: [],
      preserveLocalStorage: true,
      localStorage: { "wenlan-theme": copy.theme },
    });
    await page.goto("/");
    const nav = page.locator(".notes-rail-nav");
    const more = nav.getByRole("button", { name: copy.more, exact: true });
    const railNames = () => nav.locator(".notes-rail-button").evaluateAll(buttons => buttons.map(button => button.getAttribute("aria-label")));
    const defaults = ["Wiki", copy.spaces, copy.graph, copy.more];
    await expect.poll(railNames).toEqual(defaults);
    await nav.getByRole("button", { name: "Wiki", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Wiki", level: 1 })).toBeVisible();
    await capture(page, `${copy.locale}-${copy.theme}-default`);

    await more.click();
    await nav.getByRole("button", { name: copy.sources, exact: true }).click();
    await expect(page.getByRole("heading", { name: copy.sources, exact: true })).toBeVisible();
    await expect(more).toHaveAttribute("aria-current", "page");
    await more.click();
    await nav.getByRole("button", { name: copy.customize, exact: true }).click();
    const customize = page.getByRole("group", { name: copy.customize, exact: true });
    await expect(customize.getByRole("checkbox")).toHaveCount(6);
    await expect(customize.getByRole("checkbox", { name: /^(Home|首頁|首页)$/ })).toHaveCount(0);
    await customize.getByRole("checkbox", { name: copy.sources, exact: true }).check();
    await customize.getByRole("checkbox", { name: copy.graph, exact: true }).uncheck();
    await expect.poll(railNames).toEqual(["Wiki", copy.spaces, copy.sources, copy.more]);
    await expect(nav.locator('[aria-current="page"]')).toHaveCount(1);
    await expect(nav.getByRole("button", { name: copy.sources, exact: true })).toHaveAttribute("aria-current", "page");
    await expectPanelWithinViewport(page);
    await capture(page, `${copy.locale}-${copy.theme}-customize`);
    await page.keyboard.press("Escape");
    await expect(customize).toHaveCount(0);
    await expect(more).toBeFocused();

    // A real reload must retain the user's choice, not reseed it through the fixture.
    await page.reload();
    await expect.poll(railNames).toEqual(["Wiki", copy.spaces, copy.sources, copy.more]);
    expect(await page.evaluate(() => JSON.parse(localStorage.getItem("wenlan-navigation-v1")!))).toEqual({ version: 1, visible: ["pages", "spaces", "sources"] });
    await more.click();
    await nav.getByRole("button", { name: copy.graph, exact: true }).click();
    await expect(page.getByTestId("atlas-view")).toBeVisible();
    await expect(more).toHaveAttribute("aria-current", "page");

    await more.click();
    await nav.getByRole("button", { name: copy.customize, exact: true }).click();
    for (const checkbox of await customize.getByRole("checkbox").all()) await checkbox.check();
    await expect(nav.locator(".notes-rail-button")).toHaveCount(7);
    await page.setViewportSize({ width: 1280, height: 600 });
    await expectPanelWithinViewport(page);
    await capture(page, `${copy.locale}-${copy.theme}-all-pinned-short`);

    await page.keyboard.press("Escape");
    await page.setViewportSize({ width: 375, height: 812 });
    // The sidebar becomes a drawer at this width. Its toggle stays in its existing place.
    await expect(page.locator(".notes-workspace-sidebar")).toHaveAttribute("aria-hidden", "true");
    await page.locator("[data-sidebar-toggle]").click();
    await more.click();
    await nav.getByRole("button", { name: copy.customize, exact: true }).click();
    await expectPanelWithinViewport(page);
    await capture(page, `${copy.locale}-${copy.theme}-all-pinned-narrow`);
    for (const checkbox of await customize.getByRole("checkbox").all()) await checkbox.uncheck();
    await expect.poll(railNames).toEqual([copy.more]);
    await expectPanelWithinViewport(page);
    await expect(nav.locator('[aria-current="page"]')).toHaveCount(1);
    await capture(page, `${copy.locale}-${copy.theme}-all-hidden-narrow`);
    await page.keyboard.press("Escape");
    await expect(more).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(page.locator(".notes-workspace-sidebar")).toHaveAttribute("aria-hidden", "true");
    await expect(page.locator("[data-sidebar-toggle]")).toBeFocused();
    await page.locator("[data-sidebar-toggle]").click();
    await more.click();
    await expect(nav.getByRole("button", { name: "Wiki", exact: true })).toBeVisible();
    await nav.getByRole("button", { name: copy.customize, exact: true }).click();
    await customize.getByRole("button", { name: copy.reset, exact: true }).click();
    await expect.poll(railNames).toEqual(defaults);
    await page.keyboard.press("Escape");
    await nav.getByRole("button", { name: "Wiki", exact: true }).click();
    await expect(page.getByRole("heading", { name: "Wiki", level: 1 })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(375);
    await page.setViewportSize({ width: 1280, height: 800 });
    await expect(more).toBeVisible();
    await more.click();
    await nav.getByRole("button", { name: copy.customize, exact: true }).click();
    await capture(page, `${copy.locale}-${copy.theme}-default-customize`);
    expect(errors.pageErrors).toEqual([]);
    expect(errors.consoleErrors).toEqual([]);
  });
}

test("closing navigation customization keeps the note in its writing session", async ({ page }) => {
  const errors = collectBrowserErrors(page);
  await page.setViewportSize({ width: 1280, height: 800 });
  await installTauriMock(page, { locale: "en", rawActions: [] });
  await page.goto("/");
  const nav = page.locator(".notes-rail-nav");
  const more = nav.getByRole("button", { name: "More", exact: true });
  await page.locator(".notes-list-panel").getByRole("button", { name: "Open Fixture architecture", exact: true }).click();
  const editor = page.locator(".cm-content[contenteditable='true']");
  await expect(editor).toBeVisible();
  for (const narrow of [false, true]) {
    if (narrow) {
      await page.setViewportSize({ width: 375, height: 812 });
      await expect(page.locator(".notes-workspace-sidebar")).toHaveAttribute("aria-hidden", "true");
      await page.locator("[data-sidebar-toggle]").click();
    }
    await more.click();
    await nav.getByRole("button", { name: "Customize navigation", exact: true }).click();
    await expect(nav.getByRole("button", { name: "Back to More", exact: true })).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("group", { name: "Customize navigation", exact: true })).toHaveCount(0);
    await expect(more).toBeFocused();
    await expect(editor).toBeVisible();
    if (narrow) {
      await page.keyboard.press("Escape");
      await expect(page.locator(".notes-workspace-sidebar")).toHaveAttribute("aria-hidden", "true");
      await expect(editor).toBeVisible();
      await expect(page.locator("[data-sidebar-toggle]")).toBeFocused();
    }
  }
  expect(errors.pageErrors).toEqual([]);
  expect(errors.consoleErrors).toEqual([]);
});
