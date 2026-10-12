// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test, type Page } from '@playwright/test';
import { installTauriMock, collectBrowserErrors } from './tauriMock';
import { createSpacesNavigationFixture } from './fixtures/spacesNavigation';
import { resources } from '../src/i18n/resources';
const activity = {
 state:'blocked',last_activity_at:1791554400,assets:[
 {kind:'memories',state:'blocked',done:0,total:9,blocked:9,steps:[{name:'store',state:'idle',done:9,total:9,failed:0,job:null},{name:'summarize',state:'blocked',done:0,total:9,failed:0,job:'everyday'},{name:'link',state:'blocked',done:0,total:9,failed:0,job:'everyday'}]},
 {kind:'entities',state:'blocked',done:0,total:9,blocked:9,steps:[{name:'detect',state:'blocked',done:0,total:9,failed:0,job:'everyday'},{name:'confirm',state:'idle',done:0,total:4,failed:0,job:null}]},
 {kind:'pages',state:'idle',done:17,total:17,blocked:0,steps:[{name:'write',state:'idle',done:17,total:17,failed:0,job:'synthesis'}]}],
 everyday:{job:'everyday',lane:'none',model:null,mode:'unconfigured',available:false},synthesis:{job:'synthesis',lane:'none',model:null,mode:'unconfigured',available:false},refinement:{ready_for_review:0,not_ready:0,groups:[]}
};
async function start(page:Page,locale:'en'|'zh-Hant'|'zh-Hans',theme:string,width:number){
 await page.setViewportSize({width,height:1000}); const errors=collectBrowserErrors(page);
 const runtime=await installTauriMock(page,{locale,rawActions:['store','page_create'],localStorage:{'wenlan-theme':theme},fixture:{...createSpacesNavigationFixture(),refinements:[{id:'cross-spaces',action:'cross_space_discovery',source_ids:['memory-0','memory-1'],payload:{action:'cross_space_discovery',spaces:['Travel','Reading','Product design'],memory_count:6},confidence:0.9,created_at:'2026-10-09 00:00:00'}],distillReview:{pages_created:0,scoped:false,created_ids:[],pending:[{source_ids:['memory-0','memory-1'],contents:['Fictional weekend plan: meet at 10:30, visit the riverside bookshop, then compare notes over coffee.','Bring the reading notes and a light raincoat.'],space:'Weekend reading plan',estimated_tokens:30}],stale_pages:[],stale_truncated:false,orphan_topics:[{label:'Reading habits and design decisions',count:3},{label:'Weekend planning',count:2}]}}});
 await page.addInitScript(a=>{const orig=window.__wenlanTauriInvoke;window.__wenlanTauriInvoke=async(command,args)=>command==='get_activity'?a:orig(command,args);},activity);
 await page.goto('/');await page.evaluate(()=>document.fonts.ready);
 if(!await page.getByTestId('activity-status').isVisible()) await page.locator('[data-sidebar-toggle=true]').click();
 await page.getByTestId('activity-status').click();
 return {errors,runtime,tr:resources[locale].translation};
}
for(const locale of ['en','zh-Hant','zh-Hans'] as const) for(const theme of ['light','dark']) for(const width of [1440,375]) test(`hierarchy ${locale} ${theme} ${width}`,async({page},info)=>{
 const {errors,runtime,tr}=await start(page,locale,theme,width);
 const order=['pages','memories','entities'];
 const summary=page.getByTestId('activity-summary');await expect(page.getByTestId('activity-asset-pages')).toBeVisible();
 expect(await summary.locator('[data-testid^="activity-asset-"]').evaluateAll(nodes=>nodes.map(n=>n.getAttribute('data-testid')).filter(id=>/^activity-asset-(pages|memories|entities)$/.test(id??'')))).toEqual(order.map(k=>'activity-asset-'+k));
 await page.screenshot({path:info.outputPath('summary.png')});
 await page.getByTestId('activity-summary-open').click();await expect(page.getByTestId('activity-now')).toBeVisible();
 expect(await page.locator('.mem-activity-asset-heading h3').allTextContents()).toEqual(order.map(k=>tr.activityStatus.rowAsset[k as keyof typeof tr.activityStatus.rowAsset]));
 await expect(page.getByRole('heading',{name:'Weekend reading plan',exact:true})).toBeVisible();
 await page.waitForFunction(()=>document.querySelector('main')?.getAnimations({subtree:true}).filter(a=>a.effect?.getComputedTiming().iterations!==Infinity).every(a=>a.playState!=='running'));
 await page.screenshot({path:info.outputPath('overview.png')});
 await page.locator('.mem-activity-discoveries').scrollIntoViewIfNeeded();await page.screenshot({path:info.outputPath('suggestions.png')});
 const headings=page.locator('.mem-activity-page h2');expect(await headings.allTextContents()).toEqual([tr.activityStatus.nowTitle,tr.activityDiscoveries.title,tr.activityDiscoveries.recent]);
 const overflow=await page.locator('.mem-activity-page,.mem-activity-asset-row,.mem-activity-discoveries,.mem-activity-step').evaluateAll(nodes=>nodes.filter(n=>n.scrollWidth>n.clientWidth+1).map(n=>({class:n.className,width:n.clientWidth,scroll:n.scrollWidth})));expect(overflow).toEqual([]);
 await page.getByRole('heading',{name:tr.activityDiscoveries.recent,exact:true}).scrollIntoViewIfNeeded();await page.screenshot({path:info.outputPath('recent.png')});
 expect(runtime.calls().filter(c=>['accept_refinement','reject_refinement','redistill_page','update_memory_cmd','create_page'].includes(c.command))).toEqual([]);
 expect(errors.pageErrors).toEqual([]);expect(errors.consoleErrors).toEqual([]);
});

test('suggestions retain local hide/restore, full sources and explicit dismiss',async({page})=>{
 const {runtime,errors,tr}=await start(page,'en','light',1280);await page.getByTestId('activity-summary-open').click();
 const panel=page.locator('.mem-activity-discoveries');await expect(panel.getByRole('heading',{name:'Weekend reading plan',exact:true})).toBeVisible();
 await panel.locator('.mem-activity-source-preview > summary').click();await expect(panel.getByText('Bring the reading notes and a light raincoat.',{exact:true})).toBeVisible();
 const row=panel.locator('li').filter({has:page.getByRole('heading',{name:'Weekend reading plan',exact:true})}).first();await row.getByRole('button',{name:tr.review.hide,exact:true}).click();await expect(panel.getByRole('heading',{name:'Weekend reading plan',exact:true})).toHaveCount(0);
 await panel.locator('.mem-activity-hidden-results > summary').click();await panel.getByRole('button',{name:tr.review.restore,exact:true}).click();await expect(panel.getByRole('heading',{name:'Weekend reading plan',exact:true})).toBeVisible();
 expect(runtime.calls().filter(c=>['accept_refinement','reject_refinement','redistill_page'].includes(c.command))).toEqual([]);
 await panel.getByRole('button',{name:tr.review.dismiss,exact:true}).click();await expect(panel.getByRole('heading',{name:tr.activityDiscoveries.crossSpace,exact:true})).toHaveCount(0);expect(runtime.calls().filter(c=>c.command==='reject_refinement')).toHaveLength(1);
 await page.getByRole('button',{name:tr.knowledgeCheck.title,exact:true}).click();await expect(page.locator('.knowledge-check')).toBeVisible();
 expect(errors.pageErrors).toEqual([]);expect(errors.consoleErrors).toEqual([]);
});
