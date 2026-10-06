// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test } from "@playwright/test";
import { createReviewDecisionFixture } from "./fixtures/reviewDecisions";
import { collectBrowserErrors, installTauriMock } from "./tauriMock";
const labels = {
 en:{all:"All notes",back:"Back",newFolder:"New folder",folderName:"Folder name",create:"Create",newPage:"New page",empty:"This folder has no notes yet."},
 "zh-Hans":{all:"所有笔记",back:"返回",newFolder:"新建文件夹",folderName:"文件夹名称",create:"创建",newPage:"新建笔记",empty:"这个文件夹还没有笔记。"},
 "zh-Hant":{all:"所有筆記",back:"返回",newFolder:"新增資料夾",folderName:"資料夾名稱",create:"建立",newPage:"新增筆記",empty:"這個資料夾還沒有筆記。"},
};
for (const locale of ["en","zh-Hans","zh-Hant"] as const) for (const width of [1280,375]) {
 test(`real directories and cards/list/history agree ${locale} ${width}`,async({page})=>{
  const copy=labels[locale]; const errors=collectBrowserErrors(page);
  await page.setViewportSize({width,height:800});
  await installTauriMock(page,{locale,fixture:createReviewDecisionFixture("wiki-folders")});
  await page.goto("/");
  const sidebar=page.locator(".notes-list-panel"); const overview=page.locator(".wiki-overview");
  const showSidebar=async()=>{if(width<800)await page.locator("[data-sidebar-toggle]").click();};
  await showSidebar(); await sidebar.getByRole("button",{name:"Work",exact:true}).click();
  await expect(page.getByRole("heading",{name:"Work",exact:true})).toBeVisible();
  await expect(overview.locator(".wiki-child-folder")).toHaveText("Research");
  await overview.getByTestId("asset-lens-cards").click();
  await expect(overview.locator(".asset-card")).toHaveCount(2);
  const cards=await overview.locator(".asset-card-title").allTextContents();
  await overview.getByTestId("asset-lens-rows").click();
  await expect(overview.locator("tbody tr")).toHaveCount(2);
  const rows=await overview.locator(".wiki-page-link-title").allTextContents();expect(rows).toEqual(cards);
  await expect(overview.getByRole("button",{name:/Deterministic fixtures/})).toHaveCount(0);
  await overview.getByRole("button",{name:/Fixture architecture/}).click();
  await expect(page.getByRole("heading",{level:1,name:"Fixture architecture",exact:true})).toBeVisible();
  await showSidebar(); await expect(sidebar.getByRole("button",{name:"Work",exact:true})).toHaveAttribute("aria-current","page");
  if(width<800)await page.locator("[data-sidebar-toggle]").click();
  await page.locator(".workspace-history-navigation").getByRole("button",{name:copy.back,exact:true}).click();
  await expect(page.getByRole("heading",{name:"Work",exact:true})).toBeVisible();
  await showSidebar(); await sidebar.getByRole("button",{name:"Empty",exact:true}).click();
  await expect(overview.getByText(copy.empty,{exact:true})).toBeVisible();
  await showSidebar(); await sidebar.getByRole("button",{name:copy.newFolder,exact:true}).click();
  await sidebar.getByRole("textbox",{name:copy.folderName,exact:true}).fill("New child");
  await sidebar.getByRole("button",{name:copy.create,exact:true}).click();
  await expect(page.getByRole("heading",{name:"New child",exact:true})).toBeVisible();
  await expect(overview.getByText(copy.empty,{exact:true})).toBeVisible();
  if(width<800)await page.locator("[data-sidebar-toggle]").click();
  expect(errors.pageErrors).toEqual([]);expect(errors.consoleErrors).toEqual([]);
  expect(await page.locator("body").evaluate(el=>el.scrollWidth<=window.innerWidth)).toBe(true);
 });
}
test("new draft retains folder across reload, then publishing and moving preserve identity and Space",async({page})=>{
 const errors=collectBrowserErrors(page);const runtime=await installTauriMock(page,{locale:"en",fixture:createReviewDecisionFixture("wiki-folders")});await page.goto("/");
 const sidebar=page.locator(".notes-list-panel"); await sidebar.getByRole("button",{name:"Work",exact:true}).click();
 await page.locator(".wiki-overview").getByRole("button",{name:"New page",exact:true}).click();
 await page.locator(".page-draft-space select").selectOption("Research");
 await page.getByRole("textbox",{name:"Title",exact:true}).fill("Folder draft");
 await page.getByRole("textbox",{name:"Content",exact:true}).fill("Saved content stays in the chosen folder.");
 await expect(page.getByText("Saved",{exact:true})).toBeVisible();
 const create=runtime.calls().find(call=>call.command==="create_page_draft");expect(create?.args).toMatchObject({folderPath:"Work",space:"Research"});
 const draftId=(create?.args as {clientDraftId:string}).clientDraftId;
 await page.reload();await sidebar.getByRole("button",{name:"Work",exact:true}).click();
 await page.locator(".wiki-overview").getByRole("button",{name:"Open Folder draft · Draft",exact:true}).click();
 await expect(page.getByRole("textbox",{name:"Content",exact:true})).toHaveValue("Saved content stays in the chosen folder.");
 await expect(page.locator(".page-draft-space select")).toHaveValue("Research");
 await page.getByRole("button",{name:"Publish",exact:true}).click();
 await expect(page.getByRole("heading",{level:1,name:"Folder draft",exact:true})).toBeVisible();
 await page.getByRole("button",{name:"Page actions",exact:true}).click();
 await page.getByRole("menuitem",{name:"Move to folder",exact:true}).click();
 const dialog=page.getByRole("dialog",{name:"Move to folder",exact:true});await dialog.getByRole("combobox",{name:"Destination folder",exact:true}).selectOption("Reading");
 runtime.failNext("page_move","destination collision");await dialog.getByRole("button",{name:"Move",exact:true}).click();await expect(dialog.getByRole("alert")).toBeVisible();
 await dialog.getByRole("button",{name:"Move",exact:true}).click();await expect(dialog).toBeHidden();
 await expect(sidebar.getByRole("button",{name:"Reading",exact:true})).toHaveAttribute("aria-current","page");
 await expect(sidebar.getByRole("button",{name:"Open Folder draft",exact:true})).toBeVisible();
 const moves=runtime.calls().filter(call=>call.command==="page_move");expect(moves).toHaveLength(2);expect(moves[0].args).toMatchObject({id:draftId,folderPath:"Reading"});expect(moves[1].args).toEqual(moves[0].args);
 expect(await page.evaluate(async id => window.__TAURI_INTERNALS__.invoke("get_page",{id}),draftId)).toMatchObject({id:draftId,space:"Research",content:"Saved content stays in the chosen folder."});
 await sidebar.getByRole("button",{name:"Reading",exact:true}).click();await expect(page.locator(".wiki-overview").getByRole("button",{name:/Open Folder draft/})).toBeVisible();
 expect(errors.pageErrors).toEqual([]);expect(errors.consoleErrors).toEqual([]);
});

test("old daemon keeps every note available and disables unsupported folder operations", async ({ page }) => {
  const errors = collectBrowserErrors(page);
  await installTauriMock(page, { locale: "en", fixture: createReviewDecisionFixture("wiki-folders"), failures: [{command:"knowledge_folders_list",message:"Unknown command",times:100}] });
  await page.goto("/");
  const sidebar = page.locator(".notes-list-panel");
  await expect(sidebar.getByText("Folder browsing is unavailable. All notes are still accessible.")).toBeVisible();
  await expect(sidebar.getByRole("button",{name:"New folder",exact:true})).toBeDisabled();
  await expect(sidebar.getByRole("button",{name:"Work",exact:true})).toHaveCount(0);
  await expect(page.locator(".wiki-overview").getByRole("button",{name:/Open Independent research/})).toBeVisible();
  await page.locator(".wiki-overview").getByRole("button",{name:/Open Fixture architecture/}).click();
  await page.getByRole("button",{name:"Page actions",exact:true}).click();
  await page.getByRole("menuitem",{name:"Move to folder",exact:true}).click();
  const dialog = page.getByRole("dialog",{name:"Move to folder",exact:true});
  await expect(dialog.getByRole("status")).toHaveText("Folder browsing is unavailable. All notes are still accessible.");
  await expect(dialog.getByRole("button",{name:"Move",exact:true})).toBeDisabled();
  await dialog.getByRole("button",{name:"Cancel",exact:true}).click();
  await expect(page.getByRole("heading",{name:"Fixture architecture",exact:true})).toBeVisible();
  expect(errors.pageErrors).toEqual([]); expect(errors.consoleErrors).toEqual([]);
});

test("actual file-save failure survives Main navigation and safely replays the same publish", async ({ page }) => {
  const runtime = await installTauriMock(page, { locale: "en", fixture: createReviewDecisionFixture("wiki-folders"), pageScenario:{projectionPendingOnce:true} });
  await page.goto("/");
  await page.locator(".notes-list-panel").getByRole("button",{name:"Work",exact:true}).click();
  await page.locator(".wiki-overview").getByRole("button",{name:"New page",exact:true}).click();
  await page.getByRole("textbox",{name:"Title",exact:true}).fill("Saved even when folder is unavailable");
  await page.getByRole("textbox",{name:"Content",exact:true}).fill("This note is safely persisted before its file can be written.");
  await page.getByRole("button",{name:"Publish",exact:true}).click();
  await expect(page.getByText("Your note is saved, but its file could not be saved in the chosen folder.",{exact:true})).toBeVisible();
  const initial = runtime.calls().find(call=>call.command==="publish_page_draft");
  const args = initial?.args as { id:string;expectedVersion:number };
  const saved = await page.evaluate(async id => window.__TAURI_INTERNALS__.invoke("get_page",{id}),args.id);
  expect(saved).toMatchObject({id:args.id,status:"active",storage_path:null,content:"This note is safely persisted before its file can be written."});
  expect(saved).not.toHaveProperty("projection_error");
  await page.getByRole("button",{name:"Retry saving the file",exact:true}).click();
  await expect(page.getByText("Your note is saved, but its file could not be saved in the chosen folder.",{exact:true})).toHaveCount(0);
  const publishes = runtime.calls().filter(call=>call.command==="publish_page_draft");
  expect(publishes).toHaveLength(2); expect(publishes[1].args).toEqual(initial?.args);
  expect(await page.evaluate(async id => window.__TAURI_INTERNALS__.invoke("get_page",{id}),args.id)).toMatchObject({id:args.id,version:args.expectedVersion+1,storage_path:`Work/${args.id}.md`});
});
