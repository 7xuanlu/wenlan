// SPDX-License-Identifier: AGPL-3.0-only
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { expect, test, type Page } from "@playwright/test";
import { collectBrowserErrors, installTauriMock } from "./tauriMock";
import { renderedContrast } from "./helpers/renderedContrast";

const evidenceDir = path.join(
  process.cwd(),
  ".omo/evidence/task-7-spaces-navigation-redesign/screenshots",
);
const fixtureNow = 1_783_728_000_000;

async function settle(page: Page): Promise<void> {
  await page.evaluate(async () => { await document.fonts.ready; });
  await page.evaluate(() => {
    for (const animation of document.getAnimations()) {
      try {
        animation.finish();
      } catch {
        animation.cancel();
      }
    }
  });
  await expect(page.locator("main")).toBeVisible();
  const sidebar = page.locator('aside[aria-label="Primary navigation"]');
  if (await sidebar.getAttribute("aria-hidden") === "false") {
    await expect(sidebar).toHaveCSS("width", "240px");
    await expect(sidebar.locator(":scope > div")).toHaveCSS("opacity", "1");
  }
}

// These redesigned surfaces use explicit browser contracts and review artifacts
// instead of requiring new screenshots in Git. Unchanged surfaces retain their
// existing pixel baselines.
async function assertRedesignedSurface(page: Page, name: string): Promise<boolean> {
  const spaces = name.startsWith("spaces-");
  const wikiReferences = name.startsWith("home-");
  if (!spaces && !wikiReferences) return false;
  const viewport = page.viewportSize()!;
  const overflow = await page.evaluate(() => ({
    page: document.documentElement.scrollWidth - window.innerWidth,
    main: document.querySelector("main")!.scrollWidth - document.querySelector("main")!.clientWidth,
  }));
  expect(overflow.page).toBeLessThanOrEqual(1);
  expect(overflow.main).toBeLessThanOrEqual(1);
  if (spaces) {
    await expect(page.locator(".spaces-suggestions")).not.toHaveAttribute("open");
    await expect(page.locator("summary").filter({ hasText: "Suggested (2)" })).toBeVisible();
    await expect(page.getByTestId("space-row-space-suggested")).toBeHidden();
    const firstRow = page.getByTestId("space-row-space-wenlan");
    await expect(firstRow.getByRole("button", { name: "Wenlan", exact: true })).toBeVisible();
    const box = (await firstRow.boundingBox())!;
    expect(box.y, "confirmed Spaces must appear in the first half of the window").toBeLessThan(viewport.height / 2);
    expect(box.x).toBeGreaterThanOrEqual(0);
    expect(box.x + box.width).toBeLessThanOrEqual(viewport.width);
    const controls = page.locator(".spaces-header-actions");
    expect((await controls.boundingBox())!.height, "header actions must fit one row").toBeLessThan(60);
    const contrast = await renderedContrast(page, [
      { selector: ".spaces-overview h1", label: "Spaces title", foregroundProperty: "color", minimum: 4.5 },
      { selector: ".spaces-suggestions > summary", label: "Suggestions disclosure", foregroundProperty: "color", minimum: 4.5 },
    ]);
    for (const result of contrast) expect(result.ratio, result.label).toBeGreaterThanOrEqual(result.minimum);
  } else {
    const home = page.getByTestId("wiki-home");
    await expect(home).toBeVisible();
    await expect(home).toContainText("Fixture architecture summary");
    await expect(home).not.toContainText("[[");
    const pages = home.getByTestId("wiki-page-list").getByRole("button");
    await expect(pages).toHaveCount(6);
    await expect(pages.first()).toHaveAccessibleName("Open Fixture architecture");
    const rows = await pages.evaluateAll((nodes) => nodes.map((node) => {
      const box = node.getBoundingClientRect();
      return { left: box.left, right: box.right, top: box.top, bottom: box.bottom, height: box.height };
    }));
    for (let index = 0; index < rows.length; index++) {
      expect(rows[index].left).toBeGreaterThanOrEqual(0);
      expect(rows[index].right).toBeLessThanOrEqual(viewport.width);
      expect(rows[index].height).toBeGreaterThanOrEqual(44);
      if (index) expect(rows[index].top).toBeGreaterThanOrEqual(rows[index - 1].bottom - 1);
    }
    const contrast = await renderedContrast(page, [
      { selector: '[data-testid="wiki-home"] h1', label: "Home title", foregroundProperty: "color", minimum: 4.5 },
      { selector: '[data-testid="wiki-page-list"] button p', label: "Page title", foregroundProperty: "color", minimum: 4.5 },
    ]);
    for (const result of contrast) expect(result.ratio, result.label).toBeGreaterThanOrEqual(result.minimum);
  }
  return true;
}

async function capture(page: Page, name: string): Promise<void> {
  await page.mouse.move(1, 1);
  await page.evaluate(() => {
    if (document.activeElement instanceof HTMLElement) {
      document.activeElement.blur();
    }
  });
  await page.waitForTimeout(32);
  await page.locator("main").evaluate((node) => {
    node.scrollLeft = 0;
    node.scrollTop = 0;
  });
  await expect.poll(() => page.locator("main").evaluate((node) => node.scrollTop)).toBe(0);
  await settle(page);
  await page.screenshot({ path: path.join(evidenceDir, `${name}.png`), fullPage: false });
  await test.info().attach(name, { path: path.join(evidenceDir, `${name}.png`), contentType: "image/png" });
  if (!(await assertRedesignedSurface(page, name))) {
    await expect(page).toHaveScreenshot(`${name}.png`, {
      animations: "disabled",
      fullPage: false,
      maxDiffPixelRatio: 0.0015,
    });
  }
}

async function openSidebar(page: Page): Promise<void> {
  const sidebar = page.locator('aside[aria-label="Primary navigation"]');
  const overlay = await page.evaluate(() => window.matchMedia("(max-width: 899px)").matches);
  await expect(sidebar).toHaveCSS("position", overlay ? "fixed" : "relative");
  if (await sidebar.getAttribute("aria-hidden") === "true") {
    await page.getByTitle("Show sidebar").click();
  }
  await expect(sidebar).toHaveAttribute("aria-hidden", "false");
}

async function openSpaces(page: Page): Promise<void> {
  await openSidebar(page);
  await page.getByRole("navigation", { name: "Primary navigation" }).getByRole("button", { name: "Spaces", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Spaces" })).toBeVisible();
}

async function openWiki(page: Page): Promise<void> {
  await openSidebar(page);
  await page.getByRole("navigation", { name: "Primary navigation" }).getByRole("button", { name: "Wiki", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Wiki" })).toBeVisible();
}

async function captureFiveSurfaces(page: Page, label: string): Promise<void> {
  await openSidebar(page);
  await page.getByRole("navigation", { name: "Primary navigation" }).getByRole("button", { name: "Home", exact: true }).click();
  await capture(page, `home-${label}`);
  await openWiki(page);
  await capture(page, `pages-${label}`);
  await openSpaces(page);
  await capture(page, `spaces-${label}`);
  await page.getByTestId("space-row-space-wenlan").getByRole("button", { name: "Wenlan", exact: true }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Wenlan" })).toBeVisible();
  await capture(page, `space-${label}`);
  await page
    .getByRole("region", { name: "Recently refined" })
    .getByRole("button", { name: /^Ada Lovelace/ })
    .click();
  await expect(page.getByRole("heading", { level: 1, name: "Ada Lovelace" })).toBeVisible();
  await capture(page, `entity-${label}`);
}

test("captures the complete responsive and native-reference matrix", async ({ page }) => {
  test.setTimeout(240_000);
  // Given a fresh deterministic UI with fixed density and clean error capture.
  const browserErrors = collectBrowserErrors(page);
  await mkdir(evidenceDir, { recursive: true });
  await page.clock.setFixedTime(fixtureNow);
  await installTauriMock(page, { locale: "en", rawActions: [] });
  await page.goto("/");
  expect(await page.evaluate(() => window.devicePixelRatio)).toBe(1);

  // When all responsive and native-reference surfaces are rendered.
  for (const viewport of [
    { width: 1280, height: 900, label: "1280x900" },
    { width: 768, height: 900, label: "768x900" },
    { width: 375, height: 812, label: "375x812" },
  ] as const) {
    await page.setViewportSize(viewport);
    await captureFiveSurfaces(page, viewport.label);
    await page.evaluate(() => document.documentElement.setAttribute("data-theme", "dark"));
    await captureFiveSurfaces(page, `${viewport.label}-dark`);
    await page.evaluate(() => document.documentElement.setAttribute("data-theme", "light"));
  }

  await openSpaces(page);
  const wenlanRow = page.getByTestId("space-row-space-wenlan");
  const mobileMetadata = wenlanRow.getByTestId("space-mobile-metadata");
  const metadataFields = [
    { testId: "space-mobile-pages", label: "Pages", value: "6" },
    { testId: "space-mobile-memories", label: "Memories", value: "205" },
    { testId: "space-mobile-updated", label: "Updated", value: "Jul 10, 2026" },
  ] as const;
  await expect(mobileMetadata).toBeVisible();
  for (const field of metadataFields) {
    const metadata = wenlanRow.getByTestId(field.testId);
    await expect(metadata).toBeVisible();
    await expect(metadata.locator("dt")).toHaveText(field.label);
    await expect(metadata.locator("dd")).toHaveText(field.value);
  }
  await page.getByLabel("Filter spaces").focus();
  await page.keyboard.press("Tab");
  await page.keyboard.press("Tab");
  const wenlanControl = wenlanRow.getByRole("button", { name: "Wenlan", exact: true });
  await expect(wenlanControl).toBeFocused();
  const focusOutline = await wenlanControl.evaluate((node) => {
    const style = getComputedStyle(node);
    return { style: style.outlineStyle, width: Number.parseFloat(style.outlineWidth) };
  });
  expect(focusOutline.style).not.toBe("none");
  expect(focusOutline.width).toBeGreaterThanOrEqual(2);
  await wenlanRow.evaluate((node) => node.scrollIntoView({ block: "center" }));
  await settle(page);
  const targetedCapture = "spaces-375x812-inventory-metadata-focus.png";
  await page.screenshot({ path: path.join(evidenceDir, targetedCapture), fullPage: false });
  await assertRedesignedSurface(page, "spaces-375x812-inventory-metadata-focus");
  await test.info().attach("spaces-mobile-keyboard-focus", {
    path: path.join(evidenceDir, targetedCapture), contentType: "image/png",
  });
  await writeFile(path.join(process.cwd(), ".omo/evidence/task-7-spaces-navigation-redesign/mobile-inventory-focus.json"), `${JSON.stringify({
    focusOutline,
    labelsAndValues: metadataFields,
    screenshot: path.join(evidenceDir, targetedCapture),
    viewport: { height: 812, width: 375 },
  }, null, 2)}\n`);

  await page.setViewportSize({ width: 1586, height: 992 });
  await openSidebar(page);
  await page.getByRole("navigation", { name: "Primary navigation" }).getByRole("button", { name: "Home", exact: true }).click();
  await openSidebar(page);
  await capture(page, "home-native-1586x992");
  await page.setViewportSize({ width: 1635, height: 962 });
  await openSpaces(page);
  await capture(page, "spaces-native-1635x962");
  await page.setViewportSize({ width: 1586, height: 992 });
  await page.getByTestId("space-row-space-wenlan").getByRole("button", { name: "Wenlan", exact: true }).click();
  await capture(page, "space-native-1586x992");

  // Then all captures are viewport-only and the browser stayed error-free.
  expect(browserErrors.pageErrors).toEqual([]);
  expect(browserErrors.consoleErrors).toEqual([]);
});
