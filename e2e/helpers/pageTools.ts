// SPDX-License-Identifier: AGPL-3.0-only
import { type Page } from "@playwright/test";

/** Reach a document tool through the overflow menu at every viewport width. */
export async function openPageTool(
  page: Page,
  tool: string,
  actions = "Page actions",
): Promise<void> {
  await page.getByRole("button", { name: actions, exact: true }).click();
  await page.getByRole("menuitem", { name: tool, exact: true }).click();
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
