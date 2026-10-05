// SPDX-License-Identifier: AGPL-3.0-only
import { test, expect } from "@playwright/test";
import { createSpacesNavigationFixture } from "./fixtures/spacesNavigation";
import { installTauriMock, collectBrowserErrors } from "./tauriMock";
import { resources, supportedAppLocales } from "../src/i18n/resources";

// Browser rendering/selection proof only. Durable apply is covered by the
// core integration test and must also be exercised against an isolated app.
for (const locale of supportedAppLocales) {
  test(`${locale} renders relation sources and explicit choices in the review dialog`, async ({ page }, testInfo) => {
    const base = createSpacesNavigationFixture();
    const ownerIds = ["entity-ada", "entity-babbage", "memory-0"].sort();
    const proposal = {
      id: "relation-review", action: "lint_repair_review" as const, source_ids: ownerIds,
      payload: { action: "lint_repair_review" as const, check_id: "kg.semantic.entity_relations", occurrence_digest: "a".repeat(64), owner_binding_digest: "b".repeat(64), issue: "Review the relationship between Ada Lovelace and Charles Babbage.", choices: [], suggested_research_queries: [] },
      confidence: 0.8, created_at: "2026-09-14T00:00:00Z",
    };
    const fixture = { ...base, refinements: [proposal], distillReview: { ...base.distillReview, pending: [], stale_pages: [], orphan_topics: [] } };
    const copy = resources[locale].translation;
    const errors = collectBrowserErrors(page);
    const mock = await installTauriMock(page, { locale, rawActions: [], fixture });
    await page.setViewportSize(locale === "en" ? { width: 1280, height: 900 } : { width: 1000, height: 800 });
    await page.goto("/");
    await page.getByText(copy.review.kindLintRepair, { exact: true }).click();
    const dialog = page.getByRole("dialog");
    await expect(dialog.getByLabel(copy.sourceRepair.relationFrom)).toBeVisible();
    await expect(dialog.getByText("Ada Lovelace · Charles Babbage", { exact: true })).toBeVisible();
    await expect(dialog.getByRole("button", { name: copy.sourceRepair.prepareChange, exact: true })).toBeDisabled();
    await dialog.getByLabel(copy.sourceRepair.relationFrom).selectOption("entity-ada");
    await dialog.getByLabel(copy.sourceRepair.relationTo, { exact: true }).selectOption("entity-babbage");
    await dialog.getByLabel(copy.sourceRepair.relationType, { exact: false }).fill("collaborates_with");
    await dialog.getByLabel(copy.sourceRepair.relationSource, { exact: true }).selectOption("memory-0");
    await expect(dialog.getByRole("button", { name: copy.sourceRepair.prepareChange, exact: true })).toBeEnabled();
    const prepareBounds = await dialog.getByRole("button", { name: copy.sourceRepair.prepareChange, exact: true }).boundingBox();
    expect(prepareBounds).not.toBeNull();
    expect(prepareBounds!.y + prepareBounds!.height).toBeLessThanOrEqual(page.viewportSize()!.height);
    await page.screenshot({ path: testInfo.outputPath(`${locale}-relation-add.png`), fullPage: true });
    await dialog.getByLabel(copy.sourceRepair.relationSource, { exact: true }).scrollIntoViewIfNeeded();
    const sourceBounds = await dialog.getByLabel(copy.sourceRepair.relationSource, { exact: true }).boundingBox();
    const footerBounds = await dialog.getByRole("button", { name: copy.sourceRepair.prepareChange, exact: true }).boundingBox();
    expect(sourceBounds!.y + sourceBounds!.height).toBeLessThanOrEqual(footerBounds!.y);
    await page.screenshot({ path: testInfo.outputPath(`${locale}-relation-add-bottom.png`), fullPage: true });
    await dialog.getByLabel(copy.sourceRepair.relationAction, { exact: true }).selectOption("retire");
    await dialog.getByLabel(copy.sourceRepair.relationToRetire, { exact: true }).selectOption("relation-1");
    await expect(dialog.getByRole("button", { name: copy.sourceRepair.prepareChange, exact: true })).toBeEnabled();
    await page.screenshot({ path: testInfo.outputPath(`${locale}-relation-retire.png`), fullPage: true });
    expect(await dialog.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true);
    expect(mock.calls().filter((call) => call.command === "repair_prepare_operation")).toHaveLength(0);
    expect(errors.pageErrors).toEqual([]);
    expect(errors.consoleErrors).toEqual([]);
  });
}
