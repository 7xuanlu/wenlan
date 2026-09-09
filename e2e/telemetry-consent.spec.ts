// SPDX-License-Identifier: AGPL-3.0-only
import { test, expect } from "@playwright/test";

for (const locale of ["en", "zh-Hant", "zh-Hans"]) {
  test(`${locale}: explicit opt-in and opt-out render without overflow`, async ({page}, testInfo) => {
    const errors: string[] = [];
    page.on("pageerror", error => errors.push(error.message));
    await page.setViewportSize({width: 1000, height: 850});
    await page.goto(`/preview/telemetry.html?locale=${locale}`);
    const toggle = page.getByRole("button", {name: /usage stats|使用統計|使用统计/i});
    await expect(toggle).toBeEnabled();
    await expect(toggle).toHaveAttribute("aria-pressed", "false");
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({path: testInfo.outputPath(`${locale}-off.png`), fullPage: true, animations: "disabled"});
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-pressed", "true");
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-pressed", "false");
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    expect(errors).toEqual([]);
  });
}
for (const state of ["unknown", "unavailable", "save-failure"]) {
  test(`${state}: never presents a successful opt-in`, async ({page}) => {
    await page.goto(`/preview/telemetry.html?state=${state}`);
    const toggle = page.getByRole("button", {name: /usage stats/i});
    if (state === "save-failure") {
      await expect(toggle).toBeEnabled();
      await toggle.click();
      await expect(page.getByText(/could not persist/i)).toBeVisible();
      await expect(toggle).toHaveAttribute("aria-pressed", "false");
    } else {
      await expect(toggle).toBeDisabled();
    }
  });
}
test("known consent can be revoked even when collection is unavailable", async ({page}) => {
  await page.goto("/preview/telemetry.html?state=unavailable-on");
  const toggle = page.getByRole("button", {name: /usage stats/i});
  await expect(toggle).toBeEnabled();
  await expect(toggle).toHaveAttribute("aria-pressed", "true");
  await toggle.click();
  await expect(toggle).toBeDisabled();
  expect(await page.evaluate(() => (window as unknown as {__telemetryFixtureCalls: string[]}).__telemetryFixtureCalls.filter(c => c === "set_telemetry_enabled").length)).toBe(1);
});
