// SPDX-License-Identifier: AGPL-3.0-only
import { expect, type Page } from "@playwright/test";

/** Switch the optional notes panel to folders, including narrow screens. */
export async function showWikiFolders(page: Page) {
  const directory = page.locator(".wiki-workspace-directory");
  await expect(directory).toBeAttached();
  if (!(await directory.isVisible())) {
    // Narrow windows use the Notes toggle. Wider ones hide the list only while
    // Info has taken its room, and Wiki brings it back.
    const toggle = page.locator(".wiki-workspace-folders-toggle");
    if (await toggle.isVisible()) await toggle.click();
    else await page.getByRole("navigation", { name: /^(Primary navigation|主要導覽|主要导航|主导航)$/ })
      .getByRole("button", { name: "Wiki", exact: true }).click();
  }
  await expect(directory).toBeVisible();
  await directory.getByRole("combobox").selectOption("folders");
  await expect(directory.locator(".notes-list-panel")).toBeVisible();
  return directory;
}

/** Open a note through the optional folder view. */
export async function openWikiNote(page: Page, title: string) {
  const directory = await showWikiFolders(page);
  await expect(directory.locator(".notes-inventory-page, .notes-inventory-scope").first()).toBeVisible();
  const note = directory.locator(".notes-inventory-page").getByText(title, { exact: true });
  for (let count = 0; count < 40 && !(await note.isVisible()); count++) {
    const expand = directory.locator('.notes-inventory-disclosure[aria-expanded="false"]').first();
    if (!(await expand.count())) break;
    await expand.click();
  }
  await expect(note).toBeVisible();
  await note.click();
}
