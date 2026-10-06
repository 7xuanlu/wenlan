// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test } from "@playwright/test";
import { getSpaceEntityButton } from "./spaceEntity";

test("returns the entity when it and View all are both visible", async ({ page }) => {
  await page.setContent(`
    <section aria-label="Key topics" role="region">
      <details open>
        <summary>Key topics</summary>
        <button>Ada Lovelace</button>
        <button>View all 2</button>
      </details>
    </section>
  `);

  const entity = await getSpaceEntityButton(page, "Ada Lovelace");

  await expect(entity).toBeVisible();
});


test("opens the Topics disclosure before returning an entity", async ({ page }) => {
  await page.setContent(`
    <section aria-label="Key topics" role="region">
      <details>
        <summary>Key topics</summary>
        <button>Ada Lovelace</button>
      </details>
    </section>
  `);
  await expect(page.getByRole("button", { name: "Ada Lovelace" })).toBeHidden();
  const entity = await getSpaceEntityButton(page, "Ada Lovelace");
  await expect(entity).toBeVisible();
  await expect(page.locator("details")).toHaveAttribute("open");
});
