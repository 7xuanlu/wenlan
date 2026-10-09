// SPDX-License-Identifier: AGPL-3.0-only
import { type Page } from "@playwright/test";

/** Reach note views via the dedicated inspector, at every viewport width. */
export async function openPageTool(page: Page, tool: string, actions = "Page actions"): Promise<void> {
  const hant = actions === "頁面操作";
  const hans = actions === "页面操作";
  const open = hant ? "開啟筆記側欄" : hans ? "打开笔记侧栏" : "Open note sidebar";
  const toggle = page.getByRole("button", {name: open, exact: true});
  if (await toggle.count()) await toggle.click();
  const info = ["Page info", "頁面資訊", "页面信息"].includes(tool);
  await page.getByRole("tab", {name: info ? (hant ? "資料" : hans ? "资料" : "Info") : tool, exact: true}).click();
}

/** Memory context belongs to the dossier's action menu. */
export async function openMemoryContext(
  page: Page,
  context = "Memory context",
  actions = "Memory actions",
): Promise<void> {
  await page.getByRole("button", { name: actions, exact: true }).click();
  await page.getByRole("menuitem", { name: context, exact: true }).click();
}
