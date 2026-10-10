// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test, type Page } from '@playwright/test';
import { resources } from '../src/i18n/resources';
import { collectBrowserErrors, installTauriMock } from './tauriMock';
import { knowledgeLibrary, installKnowledgeProtocol } from './fixtures/knowledgeCheck';

async function openActivity(page: Page) {
  if (!await page.getByTestId('activity-status').isVisible()) await page.locator('[data-sidebar-toggle=true]').click();
  await page.getByTestId('activity-status').click();
  await page.getByTestId('activity-summary-open').click();
}
async function start(page: Page, { locale = 'en', theme = 'light', width = 1440, mode = 'normal' }:
  {locale?: 'en'|'zh-Hant'|'zh-Hans';theme?:string;width?:number;mode?: Parameters<typeof installKnowledgeProtocol>[1]} = {}) {
  await page.setViewportSize({width,height:960});
  const errors = collectBrowserErrors(page);
  await installTauriMock(page, {locale, rawActions:[], fixture: knowledgeLibrary(), localStorage: {'wenlan-theme':theme}});
  await installKnowledgeProtocol(page, mode);
  await page.goto('/'); await page.evaluate(() => document.fonts.ready);
  await expect(page.getByRole('heading',{name:resources[locale].translation.pages.overview.readingStartTitle,exact:true})).toBeVisible();
  await openActivity(page);
  const copy = resources[locale].translation.knowledgeCheck;
  await page.getByRole('button', {name:copy.title,exact:true}).click();
  await expect(page.getByRole('heading', {level:1,name:copy.title})).toBeVisible();
  expect(await page.evaluate(() => (window as any).__knowledgeCalls.length)).toBe(0);
  return {copy,errors};
}
const calls = (page: Page) => page.evaluate(() => (window as any).__knowledgeCalls as {command:string;args:any}[]);
function noAI(commands: Awaited<ReturnType<typeof calls>>) {
  expect(commands.filter(c => c.command === 'repair_lint').every(c => c.args.query.profile === 'general')).toBe(true);
  expect(commands.filter(c => c.command === 'repair_plan').every(c => !c.args.request.deep_report)).toBe(true);
}

test('inspect original, close pane, repair two exact items serially and keep remaining decisions', async({page}, info) => {
  const {copy,errors} = await start(page);
  await page.getByRole('button', {name:copy.check,exact:true}).click();
  await expect(page.locator('.knowledge-check-row')).toHaveCount(4);
  const first = page.locator('.knowledge-check-row').first();
  await first.click();
  const pane = page.getByRole('complementary', {name:copy.detail});
  await expect(pane).toBeVisible();
  await expect(pane.getByText('Weekend plan', {exact:true})).toBeVisible();
  await expect(pane.getByText('Packing list', {exact:true})).toBeVisible();
  await page.screenshot({path:info.outputPath('ready-detail.png')});
  await pane.getByRole('button', {name:resources.en.translation.common.close,exact:true}).click();
  await expect(first).toBeFocused();
  await page.getByRole('button', {name:copy.repairItems_other.replace('{{count}}','2'),exact:true}).click();
  await expect(page.getByText(copy.allVerified,{exact:true})).toBeVisible();
  const commands = await calls(page);
  expect(commands.filter(c=>['repair_apply','repair_verify','repair_resume_runtime'].includes(c.command)).map(c=>c.command)).toEqual(['repair_apply','repair_verify','repair_resume_runtime','repair_apply','repair_verify','repair_resume_runtime']);
  expect(commands.filter(c=>c.command==='repair_apply').map(c=>c.args.request.manifest_id)).toHaveLength(2);
  await expect(page.locator('.knowledge-check-row-status-verified')).toHaveCount(2);
  await expect(page.locator('.knowledge-check-row').filter({hasText:copy.status.review})).toHaveCount(1);
  await page.screenshot({path:info.outputPath('verified-with-remaining.png')});
  noAI(commands); expect(errors.pageErrors).toEqual([]); expect(errors.consoleErrors).toEqual([]);
  // Returning to Activity must retain the completed workflow and permit a fresh check.
  await page.getByRole('button', {name:copy.back,exact:true}).click();
  await page.getByRole('button', {name:copy.title,exact:true}).click();
  await expect(page.getByRole('button', {name:copy.checkAgain,exact:true})).toBeEnabled();
  await page.getByRole('button', {name:copy.checkAgain,exact:true}).click();
  await expect(page.locator('.knowledge-check-row')).toHaveCount(4);
  expect((await calls(page)).filter(c => c.command === 'repair_apply')).toHaveLength(2);
});

test('unknown apply stops next item and recovery verifies without duplicate writes',async({page},info)=>{
  const {copy,errors}=await start(page,{mode:'unknown'});
  await page.getByRole('button',{name:copy.check,exact:true}).click();
  await page.getByRole('button',{name:copy.repairItems_other.replace('{{count}}','2'),exact:true}).click();
  await expect(page.getByRole('button',{name:copy.recover,exact:true})).toBeEnabled();
  expect((await calls(page)).filter(c=>c.command==='repair_apply')).toHaveLength(1);
  await page.screenshot({path:info.outputPath('unknown-apply.png')});
  await page.getByRole('button',{name:copy.recover,exact:true}).click();
  await expect.poll(async()=> (await calls(page)).filter(c=>c.command==='repair_verify').length).toBe(1);
  await expect(page.locator('.knowledge-check-row-status-verified')).toHaveCount(1);
  expect((await calls(page)).filter(c=>c.command==='repair_apply')).toHaveLength(1);
  await page.screenshot({path:info.outputPath('recovered-no-reapply.png')});
  noAI(await calls(page));expect(errors.pageErrors).toEqual([]);
});

test('manual issue opens the exact existing review item', async({page},info)=>{
  const {copy,errors}=await start(page);
  await page.getByRole('button',{name:copy.check,exact:true}).click();
  await page.locator('.knowledge-check-row').filter({hasText:copy.status.review}).click();
  await page.getByRole('button',{name:copy.reviewItem,exact:true}).click();
  const dialog=page.getByRole('dialog',{name:resources.en.translation.review.title,exact:true});
  await expect(dialog).toBeVisible();
  await expect(dialog.getByLabel(resources.en.translation.sourceRepair.newPageTitle)).toBeVisible();
  expect((await calls(page)).filter(c=>c.command==='repair_apply')).toHaveLength(0);
  await page.screenshot({path:info.outputPath('manual-review.png')});
  expect(errors.pageErrors).toEqual([]);expect(errors.consoleErrors).toEqual([]);
});

test('failed verification stops the batch before the next write',async({page},info)=>{
  const {copy,errors}=await start(page,{mode:'verify-failed'});
  await page.getByRole('button',{name:copy.check,exact:true}).click();
  await page.getByRole('button',{name:copy.repairItems_other.replace('{{count}}','2'),exact:true}).click();
  await expect(page.getByRole('button',{name:copy.recover,exact:true})).toBeEnabled();
  await expect(page.locator('.knowledge-check-row-status-verified')).toHaveCount(0);
  expect((await calls(page)).filter(c=>c.command==='repair_apply')).toHaveLength(1);
  expect((await calls(page)).filter(c=>c.command==='repair_resume_runtime')).toHaveLength(0);
  await page.screenshot({path:info.outputPath('verification-stopped.png')});
  expect(errors.pageErrors).toEqual([]);
});

test('incomplete and clean reports remain distinct',async({page},info)=>{
  const {copy,errors}=await start(page,{mode:'incomplete'});
  await page.getByRole('button',{name:copy.check,exact:true}).click();
  await expect(page.getByRole('alert')).toBeVisible();
  await expect(page.getByText(copy.noIssues,{exact:true})).toHaveCount(0);
  expect((await calls(page)).filter(c=>['repair_apply','repair_plan'].includes(c.command))).toHaveLength(0);
  await expect(page.getByRole('button',{name:copy.checkAgain,exact:true})).toBeEnabled();
  await page.screenshot({path:info.outputPath('incomplete.png')});expect(errors.pageErrors).toEqual([]);
});

test('unstable complete report cannot claim no issues',async({page},info)=>{
  const {copy,errors}=await start(page,{mode:'unstable'});
  await page.getByRole('button',{name:copy.check,exact:true}).click();
  await expect(page.getByRole('alert')).toBeVisible();
  await expect(page.getByText(copy.noIssues,{exact:true})).toHaveCount(0);
  await expect(page.locator('.knowledge-check-summary')).toHaveCount(0);
  expect((await calls(page)).filter(c=>['repair_apply','repair_plan'].includes(c.command))).toHaveLength(0);
  await page.screenshot({path:info.outputPath('unstable-no-clean-claim.png')});
  expect(errors.pageErrors).toEqual([]);
});

test('no findings does not advertise AI or unfinished repairs',async({page},info)=>{
  const {copy,errors}=await start(page,{mode:'empty'});
  await page.getByRole('button',{name:copy.check,exact:true}).click();
  await expect(page.getByText(copy.noIssues,{exact:true})).toBeVisible();
  await expect(page.getByText(copy.planIncomplete,{exact:true})).toHaveCount(0);
  await page.screenshot({path:info.outputPath('clean-general-only.png')});
  noAI(await calls(page));expect(errors.pageErrors).toEqual([]);expect(errors.consoleErrors).toEqual([]);
});

for(const locale of ['en','zh-Hant','zh-Hans'] as const)for(const theme of ['light','dark'])for(const width of [1440,375])test(`layout ${locale} ${theme} ${width}`,async({page},info)=>{
  test.skip(info.project.name!=='webkit');
  const {copy,errors}=await start(page,{locale,theme,width});
  await page.getByRole('button',{name:copy.check,exact:true}).click();
  await expect(page.locator('.knowledge-check-row')).toHaveCount(4);
  await page.screenshot({path:info.outputPath('list.png')});
  await page.locator('.knowledge-check-row').first().click();
  const pane=page.getByRole(width===1440?'complementary':'dialog',{name:copy.detail});
  await expect(pane).toBeVisible();
  await expect(pane.getByText('Packing list',{exact:true})).toBeVisible();
  await page.screenshot({path:info.outputPath('detail.png')});
  expect(await page.evaluate(()=>document.documentElement.scrollWidth-innerWidth)).toBeLessThanOrEqual(1);
  expect(await pane.evaluate(el=>el.scrollWidth-el.clientWidth)).toBeLessThanOrEqual(1);
  expect((await calls(page)).filter(c=>c.command==='repair_apply')).toHaveLength(0);
  noAI(await calls(page)); expect(errors.pageErrors).toEqual([]);expect(errors.consoleErrors).toEqual([]);
});
