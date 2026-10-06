// SPDX-License-Identifier: AGPL-3.0-only
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { expect, test } from "@playwright/test";
import { installTauriMock } from "./tauriMock";
import { createSpaceProjectFixture } from "./fixtures/spaceProject";
import { openPrimaryDestination } from "./helpers/primaryNavigation";
const root = process.env.WENLAN_UI_EVIDENCE_DIR ?? "/private/tmp/wenlan-notes-first-delivery/spaces-project/ui/screenshots";
const labels = { en: { spaces: "Spaces", more: "More", notes: "Notes", sources: "Sources", close: "Close" }, "zh-Hant": { spaces: "空間", more: "更多", notes: "筆記", sources: "來源", close: "關閉" }, "zh-Hans": { spaces: "空间", more: "更多", notes: "笔记", sources: "来源", close: "关闭" } };
for (const variant of [
  { name: "desktop-light-en", width: 1280, theme: "light", locale: "en" },
  { name: "desktop-dark-zh-Hant", width: 1280, theme: "dark", locale: "zh-Hant" },
  { name: "narrow-light-zh-Hans", width: 375, theme: "light", locale: "zh-Hans" },
] as const) {
  test(`captures final project ${variant.name}`, async ({ page }) => {
    await mkdir(root, { recursive: true });
    await page.setViewportSize({ width: variant.width, height: 900 });
    await installTauriMock(page, { locale: variant.locale, rawActions: [], fixture: createSpaceProjectFixture(), localStorage: { "wenlan-theme": variant.theme, "wenlan-spaces-view-mode": "rows" } });
    await page.goto("/"); await openPrimaryDestination(page, labels[variant.locale].spaces, labels[variant.locale].more);
    await page.getByTestId("space-row-project-wenlan").getByRole("button", { name: "Wenlan", exact: true }).click();
    const shots: string[] = [];
    const capture = async (state: string) => {
      await page.evaluate(async () => { await document.fonts.ready; });
      expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
      const file = `${variant.name}-${state}.png`; shots.push(file); await page.screenshot({ path: path.join(root, file), fullPage: true });
    };
    await expect(page.getByRole("tab", { name: labels[variant.locale].notes })).toHaveAttribute("aria-selected", "true"); await capture("notes");
    await page.getByRole("tab", { name: labels[variant.locale].sources }).click();
    await expect(page.getByText("Writing experience observations", { exact: true })).toBeVisible(); await capture("sources");
    await page.getByText("Writing experience observations", { exact: true }).click();
    await expect(page.getByText("Start with the note itself. Keep references close when the reader needs them.")).toBeVisible(); await capture("preview");
    await page.getByRole("button", { name: labels[variant.locale].close, exact: true }).click();
    if (variant.locale === "en") { await page.getByRole("button", { name: "Actions for Wenlan" }).click(); await expect(page.getByRole("menuitem", { name: "Review page changes" })).toBeVisible(); await capture("menu"); }
    await writeFile(path.join(root, `${variant.name}.json`), JSON.stringify({ ...variant, shots }, null, 2));
  });
}
for (const state of ["empty", "loading", "error"] as const) {
  test(`captures source ${state} and retry`, async ({ page }) => {
    await mkdir(root, { recursive: true });
    const fixture = createSpaceProjectFixture(state === "empty");
    await installTauriMock(page, { locale: "en", rawActions: [], fixture, delays: state === "loading" ? { list_indexed_files: 60_000 } : undefined, failures: state === "error" ? [{ command: "list_indexed_files", message: "fixture source unavailable", times: 1 }] : undefined, localStorage: { "wenlan-theme": "light", "wenlan-spaces-view-mode": "rows" } });
    await page.goto("/"); await openPrimaryDestination(page, "Spaces");
    await page.getByTestId("space-row-project-wenlan").getByRole("button", { name: "Wenlan", exact: true }).click();
    await page.getByRole("tab", { name: "Sources" }).click();
    if (state === "empty") await expect(page.getByText("No sources in this space yet.")).toBeVisible();
    if (state === "loading") await expect(page.getByRole("status")).toBeVisible();
    if (state === "error") await expect(page.getByRole("alert")).toContainText("Could not load sources for this space.");
    await page.screenshot({ path: path.join(root, `desktop-light-en-${state}.png`), fullPage: true });
    if (state === "error") { await page.getByRole("button", { name: "Try again" }).click(); await expect(page.getByText("Writing experience observations")).toBeVisible(); await page.screenshot({ path: path.join(root, "desktop-light-en-retry.png"), fullPage: true }); }
  });
}
