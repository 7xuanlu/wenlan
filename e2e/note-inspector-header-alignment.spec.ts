// SPDX-License-Identifier: AGPL-3.0-only
import {expect,test,type Page,type Locator} from '@playwright/test';
import {resources} from '../src/i18n/resources';
import {createReviewDecisionFixture} from './fixtures/reviewDecisions';
import {installTauriMock,collectBrowserErrors} from './tauriMock';
import {openWikiNote} from './helpers/wikiWorkspace';
const frame=(p:Page,id='primary')=>p.locator(`[data-note-group-id="${id}"]`);
async function geometry(group:Locator){return group.evaluate(el=>{
 const rect=(s:string)=>{const r=el.querySelector(s)!.getBoundingClientRect();return{x:r.x,y:r.y,right:r.right,bottom:r.bottom,width:r.width,height:r.height};};
 const targets=['.note-tabs [role="tab"][aria-selected="true"]','.note-tab-create','.note-tab-close','.note-inspector-tabs [role="tab"]:first-child','.note-inspector-tabs [role="tab"]:last-child','.page-info-drawer-workspace-toggle','.page-info-drawer-close'];
 const content=el.querySelector('.page-info-drawer-content')!,contentRect=content.getBoundingClientRect();
 return{noteHeader:rect('.note-group-header'),inspectorHeader:rect('.page-info-drawer-header'),inspectorPanel:rect('.page-info-drawer--inspector'),canvasInspector:!!el.querySelector('.page-info-drawer--inspector .note-inspector-panel--canvas'),contentClearance:contentRect.y+Number.parseFloat(getComputedStyle(content).paddingTop||'0'),noteTabs:rect('.note-group-tabs'),noteText:rect('.note-tabs [role="tab"][aria-selected="true"] span'),infoText:rect('.note-inspector-tabs [role="tab"]:first-child span'),overflow:document.documentElement.scrollWidth>innerWidth,targets:targets.map(s=>{const node=el.querySelector(s)!;const r=node.getBoundingClientRect();return{selector:s,width:r.width,height:r.height,hit:node.contains(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2))};})};
 });}
async function aligned(group:Locator){const g=await geometry(group);expect(Math.abs(g.noteHeader.y-g.inspectorHeader.y)).toBeLessThanOrEqual(1);expect(g.inspectorHeader.height).toBe(g.canvasInspector&&g.inspectorPanel.width<=279?96:52);expect(g.inspectorHeader.bottom).toBeLessThanOrEqual(g.contentClearance+1);expect(g.noteTabs.right).toBeLessThanOrEqual(g.inspectorHeader.x+1);expect(Math.abs((g.noteText.y+g.noteText.height/2)-(g.infoText.y+g.infoText.height/2))).toBeLessThanOrEqual(2);expect(g.overflow).toBe(false);for(const target of g.targets){expect(target.hit,target.selector).toBe(true);expect(target.width,target.selector).toBeGreaterThanOrEqual(24);}}
const scenes=[
 {width:1280,theme:'dark',locale:'en',touch:false},
 {width:1280,theme:'light',locale:'zh-Hant',touch:false},
 {width:375,theme:'light',locale:'en',touch:false},
 {width:375,theme:'dark',locale:'zh-Hans',touch:false},
 {width:2560,theme:'light',locale:'en',touch:false},
 {width:375,theme:'dark',locale:'zh-Hant',touch:true},
] as const;
for(const scene of scenes)test.describe(`${scene.width} ${scene.theme} ${scene.locale}${scene.touch?' touch':''}`,()=>{
 test.use({viewport:{width:scene.width,height:960},hasTouch:scene.touch});
 test('note and inspector tabs share a row and remain independently usable',async({page},info)=>{
 const t=resources[scene.locale].translation,errors=collectBrowserErrors(page);
 await installTauriMock(page,{fixture:createReviewDecisionFixture('wiki-folders'),locale:scene.locale,rawActions:[],localStorage:{'wenlan-theme':scene.theme}});
 await page.goto('/');await openWikiNote(page,'Fixture architecture');await openWikiNote(page,'Why fixtures stay deterministic');
 const left=frame(page),right=frame(page,'secondary');
 await left.getByRole('tab',{name:'Why fixtures stay deterministic',exact:true}).click({button:'right'});await page.getByRole('menuitem',{name:t.pages.groups.moveToRight,exact:true}).click();
 for(const group of [left,right])await group.locator('.note-inspector-toggle').click();
 await expect(right.locator('.page-info-drawer')).toBeVisible();
 await page.screenshot({path:info.outputPath('both-info.png')});
 await info.attach('both-info-geometry',{body:JSON.stringify(await Promise.all([geometry(left),geometry(right)])),contentType:'application/json'});
 await aligned(left);await aligned(right);
 await right.locator('.note-inspector-tabs [role="tab"]').nth(1).click();
 await expect(right.locator('.page-canvas')).toBeVisible();
 await right.locator('.page-info-drawer-workspace-toggle').click();
 await expect(right.locator('.page-info-drawer-expanded-group')).toBeVisible();
 await aligned(left);await aligned(right);
 await expect(left.locator('.note-inspector-tabs [role="tab"]').first()).toHaveAttribute('aria-selected','true');
 await page.screenshot({path:info.outputPath('right-expanded.png')});
 await right.locator('.page-info-drawer-workspace-toggle').click();await aligned(right);
 if(!scene.touch){
  const tab=right.getByRole('tab',{name:'Why fixtures stay deterministic',exact:true});const from=(await tab.boundingBox())!,target=(await left.locator('.page-info-drawer').boundingBox())!;
  await page.mouse.move(from.x+from.width/2,from.y+from.height/2);await page.mouse.down();await page.mouse.move(target.x+target.width-15,target.y+150,{steps:14});
  await expect(page.locator('.note-tab-split-preview[data-kind="transfer"]')).toBeVisible();await page.keyboard.press('Escape');await page.mouse.up();
  await expect(tab).toBeVisible();await expect(page.locator('[data-note-group-id]')).toHaveCount(2);
 }
 await right.locator('.page-info-drawer-close').click();await expect(right.locator('.page-info-drawer')).toHaveCount(0);await expect(right.locator('.note-inspector-toggle')).toBeVisible();
 await expect(left.locator('.page-info-drawer')).toBeVisible();await left.locator('.page-info-drawer-close').click();
 await expect(left.locator('.note-inspector-toggle')).toBeVisible();await expect(left.getByRole('tab',{name:'Fixture architecture',exact:true})).toBeVisible();await expect(right.getByRole('tab',{name:'Why fixtures stay deterministic',exact:true})).toBeVisible();
 expect(errors.pageErrors).toEqual([]);expect(errors.consoleErrors).toEqual([]);
 });
});
