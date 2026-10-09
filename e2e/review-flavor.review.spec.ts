// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test, type Page } from "@playwright/test";
import { openPrimaryDestination } from "./helpers/primaryNavigation";
import { collectBrowserErrors } from "./tauriMock";
import type { Page as KnowledgePage } from "../src/lib/tauri";

async function openWiki(page: Page): Promise<void> {
  await page
    .getByRole("navigation", { name: "Primary navigation" })
    .getByRole("button", { name: "Wiki", exact: true })
    .click();
  await expect(page.getByRole("heading", { level: 1, name: "Wiki" })).toBeVisible();
}

async function openFixtureArchitecture(page: Page): Promise<void> {
  await openWiki(page);
  await page.getByRole("region", { name: "Wiki", exact: true })
    .getByRole("button", { name: "Open Fixture architecture", exact: true }).click();
  await expect(
    page.getByRole("heading", { level: 1, name: "Fixture architecture" }),
  ).toBeVisible();
}

/** Read the same isolated runtime that answers the Review UI's IPC calls. */
async function storedReviewPage(page: Page, id: string): Promise<KnowledgePage | null> {
  return page.evaluate(async (pageId) => {
    const modulePath = "/review/tauri-core.ts";
    const { invoke } = await import(modulePath);
    return invoke("get_page", { id: pageId });
  }, id);
}

async function closeWritingView(page: Page): Promise<void> {
  const editor = page.getByRole("textbox", { name: "Page editor", exact: true });
  await expect(editor).toBeVisible();
  await expect(editor).toBeEditable();
  // The editor owns the first Escape and flushes before returning to reading.
  // Main's next Escape is the navigation gesture back to Wiki.
  await editor.press("Escape");
  await expect(editor).toHaveCount(0);
  await expect(page.locator(".page-detail")).toBeVisible();
}

async function installRejectedCommandAudit(page: Page): Promise<void> {
  await page.addInitScript(() => {
    Reflect.set(window, "__WENLAN_REVIEW_COMMAND_FAILURES__", []);
  });
}

async function rejectedCommands(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const failures = Reflect.get(window, "__WENLAN_REVIEW_COMMAND_FAILURES__");
    return Array.isArray(failures) ? failures : [];
  });
}

test("keeps every enabled primary destination inside the Review command contract", async ({
  page,
}) => {
  const browserErrors = collectBrowserErrors(page);
  await installRejectedCommandAudit(page);
  await page.goto("/");

  const navigation = page.getByRole("navigation", { name: "Primary navigation" });

  await navigation.getByRole("button", { name: "Wiki", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Wiki" })).toBeVisible();

  await navigation.getByRole("button", { name: "Spaces", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Spaces" })).toBeVisible();

  await navigation.getByRole("button", { name: "Graph", exact: true }).click();
  await expect(page.getByTestId("atlas-view")).toBeVisible();
  // Same fixture and same count line as graph-rendering.visual.spec: pages
  // lead, and the topic count is over what is drawn (the three connected ones).
  await expect(page.getByText(/^7 pages · 3 topics(?: · \d+ regions?)?$/)).toBeVisible();

  await openPrimaryDestination(page, "Memories");
  await expect(page.getByRole("region", { name: "Memory list" })).toBeVisible();
  // Keep the same fixture row even if a mutation changes list ordering.
  const firstMemory = page.getByRole("article", { name: "Fixture architecture", exact: true });
  const memoryActions = firstMemory.getByRole("button", { name: "Memory actions", exact: true });
  await memoryActions.click();
  await firstMemory.getByRole("menuitem", { name: "Unpin memory", exact: true }).click();
  await memoryActions.click();
  await expect(firstMemory.getByRole("menuitem", { name: "Pin memory", exact: true })).toBeVisible();
  await firstMemory.getByRole("menuitem", { name: "Unconfirm memory", exact: true }).click();
  await memoryActions.click();
  await expect(firstMemory.getByRole("menuitem", { name: "Confirm memory", exact: true })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(memoryActions).toBeFocused();

  await openPrimaryDestination(page, "Sources");
  await expect(page.getByRole("heading", { level: 1, name: "Sources", exact: true })).toBeVisible();
  await expect(
    page.getByRole("heading", { level: 2, name: "Bring your sources together", exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("group", { name: "Filter sources", exact: true })).toBeVisible();
  await expect(page.getByRole("searchbox", { name: "Search sources", exact: true })).toBeVisible();
  await expect(navigation.getByRole("button", { name: "More", exact: true })).toHaveAttribute("aria-current", "page");

  await openPrimaryDestination(page, "Wiki");
  await expect(page.getByRole("heading", { level: 1, name: "Wiki", exact: true })).toBeVisible();
  await expect(navigation.getByRole("button", { name: "Wiki", exact: true })).toHaveAttribute("aria-current", "page");
  await expect(navigation.getByRole("button", { name: "More", exact: true })).not.toHaveAttribute("aria-current");

  await page.waitForTimeout(250);
  expect(await rejectedCommands(page)).toEqual([]);
  expect(browserErrors.pageErrors).toEqual([]);
  expect(browserErrors.consoleErrors).toEqual([]);
});

test("proves Review identity and exercises Wiki Page mutations", async ({ page }) => {
  const browserErrors = collectBrowserErrors(page);
  await installRejectedCommandAudit(page);
  await page.goto("/");

  const fixtureNotice = page.locator('[data-review-environment="fixture-only"]');
  await expect(fixtureNotice).toBeVisible();
  await expect(fixtureNotice).toContainText("TEST DATA");
  await expect(fixtureNotice).toContainText("Fixture data · resets on relaunch");

  await openFixtureArchitecture(page);
  const initialPage = await storedReviewPage(page, "page-architecture");
  expect(initialPage).not.toBeNull();
  const editor = page.getByRole("textbox", { name: "Page editor", exact: true });
  await expect(editor).toBeVisible();
  await expect(editor).toBeEditable();
  const editedSource = "# Fixture architecture\n\nEdited through the Review-flavor lane.";
  await editor.fill(editedSource);
  await expect.poll(async () => (await storedReviewPage(page, "page-architecture"))?.content).toBe(editedSource);
  const savedPage = await storedReviewPage(page, "page-architecture");
  expect(savedPage?.version).toBeGreaterThan(initialPage!.version);
  expect(savedPage?.user_edited).toBe(true);
  await expect(page.locator('.page-detail [role="status"]')).toHaveText("Saved");

  // Reopening uses persisted fixture content rather than the original editor DOM.
  await openFixtureArchitecture(page);
  await closeWritingView(page);
  await expect(page.getByText("Edited through the Review-flavor lane.")).toBeVisible();

  page.once("dialog", (dialog) => dialog.accept());
  await page.getByRole("button", { name: "Page actions", exact: true }).click();
  await page.getByRole("menuitem", { name: "Re-distill page", exact: true }).click();
  await expect(page.getByText("Page re-distilled.", { exact: true })).toBeVisible();
  await expect.poll(async () => (await storedReviewPage(page, "page-architecture"))?.last_compiled).toBe("2026-07-10T12:33:00Z");

  page.once("dialog", (dialog) => dialog.accept());
  await page.getByRole("button", { name: "Page actions" }).click();
  await page.getByRole("menuitem", { name: "Delete page" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Wiki" })).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Open Fixture architecture" }),
  ).toHaveCount(0);
  expect(await storedReviewPage(page, "page-architecture")).toBeNull();

  expect(await rejectedCommands(page)).toEqual([]);
  expect(browserErrors.pageErrors).toEqual([]);
  expect(browserErrors.consoleErrors).toEqual([]);
});

test("creates and publishes a Page draft through the Review runtime", async ({ page }) => {
  const browserErrors = collectBrowserErrors(page);
  await page.goto("/");
  await openWiki(page);

  await page.getByRole("button", { name: "New page", exact: true }).click();
  await page.getByRole("textbox", { name: "Title", exact: true }).fill(
    "Review lane authored Page",
  );
  await page.getByRole("textbox", { name: "Content", exact: true }).fill(
    "This Page proves draft creation and publication in the isolated Review runtime.",
  );
  await page.getByRole("button", { name: "Publish", exact: true }).click();

  await expect(
    page.getByRole("heading", { level: 1, name: "Review lane authored Page" }),
  ).toBeVisible();
  await expect(
    page.getByText(
      "This Page proves draft creation and publication in the isolated Review runtime.",
    ),
  ).toBeVisible();

  expect(browserErrors.pageErrors).toEqual([]);
  expect(browserErrors.consoleErrors).toEqual([]);
});

test("marks a stored page reviewed through the Review presence contract", async ({ page }) => {
  // Review answers page_review_supported and review_page through its isolated
  // fixture runtime. This exercises the app gesture and registered command
  // contract; a real daemon's minted presence proof is a separate live lane.
  const browserErrors = collectBrowserErrors(page);
  await installRejectedCommandAudit(page);
  await page.goto("/");
  await openFixtureArchitecture(page);

  // Review attests stored text, so the menu omits it while writing. Escape
  // flushes the editor before that action becomes reachable.
  await expect(page.getByRole("textbox", { name: "Page editor", exact: true })).toBeVisible();
  const actions = page.getByRole("button", { name: "Page actions", exact: true });
  await actions.click();
  await expect(page.getByRole("menuitem", { name: "Mark page reviewed", exact: true })).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(actions).toBeFocused();
  await closeWritingView(page);

  await expect(page.getByRole("button", { name: "Page actions", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Page actions", exact: true }).click();
  const review = page.getByRole("menuitem", { name: "Mark page reviewed", exact: true });
  await expect(review).toBeEnabled();
  await review.click();

  await expect(page.getByTestId("page-review-notice")).toHaveText("Marked as reviewed.");

  // Nothing was rejected by `review/tauri-core.ts`, which is the assertion the
  // contract exists to make.
  await page.waitForTimeout(250);
  expect(await rejectedCommands(page)).toEqual([]);
  expect(browserErrors.pageErrors).toEqual([]);
  expect(browserErrors.consoleErrors).toEqual([]);
});

test("creates a Space and navigates to its rendered detail", async ({ page }) => {
  const browserErrors = collectBrowserErrors(page);
  await page.goto("/");

  await page
    .getByRole("navigation", { name: "Primary navigation" })
    .getByRole("button", { name: "Spaces", exact: true })
    .click();
  await expect(page.getByRole("heading", { level: 1, name: "Spaces" })).toBeVisible();

  await page.getByRole("button", { name: "New", exact: true }).click();
  await page.getByRole("textbox", { name: "Name", exact: true }).fill("Review Lane");
  await page.getByRole("textbox", { name: "Description", exact: true }).fill(
    "Created by the Review-flavor Playwright lane.",
  );
  await page.getByRole("button", { name: "Create", exact: true }).click();

  const card = page.getByTestId("space-card-space-review-lane");
  await expect(card.getByRole("button", { name: "Open Review Lane", exact: true })).toBeVisible();
  await card.getByRole("button", { name: "Open Review Lane", exact: true }).click();
  await expect(
    page.getByRole("heading", { level: 1, name: "Review Lane" }),
  ).toBeVisible();
  await expect(page.getByText("Created by the Review-flavor Playwright lane.")).toBeVisible();

  expect(browserErrors.pageErrors).toEqual([]);
  expect(browserErrors.consoleErrors).toEqual([]);
});
