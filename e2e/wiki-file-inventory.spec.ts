// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test } from "@playwright/test";
import { createReviewDecisionFixture } from "./fixtures/reviewDecisions";
import { collectBrowserErrors, installTauriMock } from "./tauriMock";

const copy = {
  en: { files: "Note files", drafts: "Drafts", unfiled: "Other notes", all: "All notes", back: "Back", show: "Show sidebar" },
  "zh-Hans": { files: "笔记文件", drafts: "草稿", unfiled: "其他笔记", all: "所有笔记", back: "返回", show: "显示侧边栏" },
  "zh-Hant": { files: "筆記檔案", drafts: "草稿", unfiled: "其他筆記", all: "所有筆記", back: "返回", show: "顯示側邊欄" },
};
for (const locale of ["en", "zh-Hans", "zh-Hant"] as const) {
  for (const width of [1280, 375]) {
    test(`file scopes, lenses and history share one inventory ${locale} ${width}`, async ({ page }) => {
      const labels = copy[locale];
      const errors = collectBrowserErrors(page);
      await page.setViewportSize({ width, height: 800 });
      await installTauriMock(page, { locale, fixture: createReviewDecisionFixture("wiki-files") });
      await page.goto("/");
      const sidebar = page.locator(".notes-list-panel");
      if (width < 800) await page.locator("[data-sidebar-toggle]").click();
      await expect(sidebar.locator(".notes-page-button")).toHaveCount(0);
      await sidebar.getByRole("button", { name: labels.files, exact: true }).click();
      await expect(page.getByRole("heading", { name: labels.files, exact: true })).toBeVisible();
      const overview = page.locator(".wiki-overview");
      await overview.getByTestId("asset-lens-cards").click();
      await expect(overview.getByTestId("wiki-cards").locator(".asset-card")).toHaveCount(5);
      await expect(overview.getByText("fixture-architecture.md", { exact: true })).toBeVisible();
      await overview.getByTestId("asset-lens-rows").click();
      await expect(overview.locator("tbody tr")).toHaveCount(5);
      await expect(overview.getByText("Independent research", { exact: true })).toHaveCount(0);
      await overview.getByRole("button", { name: /Fixture architecture/ }).click();
      await expect(page.getByRole("heading", { level: 1, name: "Fixture architecture", exact: true })).toBeVisible();
      if (width < 800) await page.locator("[data-sidebar-toggle]").click();
      await expect(sidebar.locator('[aria-current="page"]')).toHaveText("fixture-architecture.md");
      if (width < 800) await page.locator("[data-sidebar-toggle]").click();
      await page.locator(".workspace-history-navigation").getByRole("button", { name: labels.back, exact: true }).click();
      await expect(page.getByRole("heading", { name: labels.files, exact: true })).toBeVisible();
      await expect(overview.locator("tbody tr")).toHaveCount(5);
      if (width < 800) await page.locator("[data-sidebar-toggle]").click();
      await sidebar.getByRole("button", { name: labels.drafts, exact: true }).click();
      await expect(page.getByRole("heading", { name: labels.drafts, exact: true })).toBeVisible();
      await expect(overview.locator("tbody tr")).toHaveCount(1);
      await expect(overview.getByText("Next experiment", { exact: true })).toBeVisible();
      await page.locator(".workspace-history-navigation").getByRole("button", { name: labels.back, exact: true }).click();
      await expect(page.getByRole("heading", { name: labels.files, exact: true })).toBeVisible();
      expect(errors.pageErrors).toEqual([]);
      expect(errors.consoleErrors).toEqual([]);
      expect(await page.locator("body").evaluate(el => el.scrollWidth <= window.innerWidth)).toBe(true);
    });
  }
}
