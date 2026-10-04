// SPDX-License-Identifier: AGPL-3.0-only
import { test, expect } from '@playwright/test';
import { installTauriMock, collectBrowserErrors } from './tauriMock';
import { enFirstUse, hansFirstUse, hantFirstUse } from '../src/components/onboarding/firstUseCopy';

// Fixture-only rendering: this does not exercise native APIs or live OAuth.
const cases = [['en', enFirstUse], ['zh-Hans', hansFirstUse], ['zh-Hant', hantFirstUse]] as const;
for (const [locale, copy] of cases) for (const width of [820, 1440]) {
  test(`${locale} ${width}: sample to read-only web setup`, async ({ page, baseURL }, info) => {
    await page.setViewportSize({ width, height: 1000 });
    const errors = collectBrowserErrors(page);
    const external: string[] = [];
    await page.route('**/*', route => {
      const url = new URL(route.request().url());
      if (url.origin === new URL(baseURL!).origin) return route.continue();
      external.push(url.origin);
      return route.abort();
    });
    await installTauriMock(page, { locale, rawActions: [] });
    await page.goto('/');
    await expect(page.getByRole('button', { name: copy.entry, exact: true })).toBeVisible();
    await page.getByRole('button', { name: copy.entry, exact: true }).click();
    await expect(page.getByTestId('first-use-guide')).toBeVisible();
    await page.getByRole('button', { name: copy.guide.tryExample, exact: true }).click();
    await page.getByRole('button', { name: copy.sample.skipToResult, exact: true }).click();
    await page.getByRole('button', { name: copy.sample.useWithAi, exact: true }).click();
    await page.getByRole('tab', { name: 'ChatGPT', exact: true }).click();
    await expect(page.getByText(copy.sample.chatgptNote, { exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: `${copy.sample.copyCommand}: ${copy.sample.handoffLabel}`, exact: true })).toHaveCount(0);
    await expect(page.getByRole('button', { name: `${copy.sample.copyCommand}: ${copy.sample.briefLabel}`, exact: true })).toBeVisible();
    await page.locator('.fus-use').scrollIntoViewIfNeeded();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    await page.screenshot({ path: info.outputPath('sample-chatgpt.png'), fullPage: true, animations: 'disabled' });
    await page.getByRole('tab', { name: 'Codex', exact: true }).click();
    await expect(page.getByRole('button', { name: `${copy.sample.copyCommand}: ${copy.sample.handoffLabel}`, exact: true })).toBeVisible();
    await page.getByRole('tab', { name: 'ChatGPT', exact: true }).click();
    await page.getByRole('button', { name: copy.sample.connectCta, exact: true }).click();
    await expect(page.getByTestId('first-use-guide')).toHaveCount(0);
    await expect(page.locator('[data-testid="setup-wizard"]')).toHaveCount(0);
    const webName = /^(Web access|网页访问|網頁存取)$/;
    await expect(page.getByRole('heading', { name: webName })).toHaveCount(1);
    const webToggle = page.getByRole('button', { name: webName });
    await expect(webToggle).toBeVisible();
    await expect(webToggle).toHaveAttribute('aria-pressed', 'false');
    await expect(webToggle).toBeDisabled();
    const pendingConsent = {
      en: 'Choose a Space before allowing remote queries.',
      'zh-Hans': '请先选择 Space，再授权远程查询。',
      'zh-Hant': '請先選擇 Space，再授權遠端查詢。',
    }[locale];
    await expect(page.getByRole('checkbox', { name: pendingConsent, exact: true })).toBeDisabled();
    await expect.poll(() => webToggle.evaluate(node => {
      const section = node.closest('section');
      return section ? getComputedStyle(section).opacity : null;
    })).toBe('1');
    await webToggle.scrollIntoViewIfNeeded();
    await page.screenshot({ path: info.outputPath('web-settings.png'), fullPage: true, animations: 'disabled' });
    expect(await page.locator('vite-error-overlay').count()).toBe(0);
    expect(errors.pageErrors).toEqual([]);
    expect(errors.consoleErrors).toEqual([]);
    expect(external).toEqual([]);
  });
}
