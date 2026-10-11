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
    let completed = false;
    await page.exposeBinding('__sampleFirstRunGate', (_source, command: string) => {
      if (command === 'should_show_wizard') return !completed;
      completed = true;
      return null;
    });
    await page.addInitScript(() => {
      const invoke = window.__wenlanTauriInvoke;
      window.__wenlanTauriInvoke = (command, args) =>
        ['should_show_wizard', 'set_setup_completed'].includes(command)
          ? (window as unknown as { __sampleFirstRunGate: (command: string) => Promise<unknown> }).__sampleFirstRunGate(command)
          : invoke(command, args);
    });
    await page.goto('/');
    await expect(page.getByTestId('notes-welcome')).toBeVisible();
    await page.getByRole('button', { name: copy.guide.tryExample, exact: true }).click();
    await expect(page.getByTestId('first-use-sample')).toBeVisible();
    expect(completed).toBe(false);
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
    await expect(page.getByTestId('first-use-sample')).toHaveCount(0);
    expect(completed).toBe(true);
    await expect(page.locator('[data-testid="setup-wizard"]')).toHaveCount(0);
    const webName = /^(Web access|网页访问|網頁存取)$/;
    await expect(page.getByRole('heading', { name: webName })).toHaveCount(1);
    // One click turns Web access on: there is no consent checkbox and no
    // "choose a Space first" step, and the Space to share is already picked.
    const webSection = page.locator('section').filter({ has: page.getByRole('heading', { name: webName }) });
    const webToggle = webSection.getByRole('button', { name: /^(Turn on|开启|開啟)$/ });
    await expect(webToggle).toHaveCount(1);
    await expect(webToggle).toBeVisible();
    await expect(webToggle).toBeEnabled();
    await expect(webSection.getByRole('combobox')).toHaveValue(/.+/);
    await expect(webSection.getByRole('checkbox')).toHaveCount(0);
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
