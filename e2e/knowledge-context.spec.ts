// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test } from "@playwright/test";
import { installTauriMock, collectBrowserErrors } from "./tauriMock";
const copies = [
  {locale:"en",open:"Open Fixture architecture",info:"Page info",sources:"Source memories",context:"Memory context",used:"Used as a source in",recent:"Recent memories",close:"Close"},
  {locale:"zh-Hant",open:"開啟 Fixture architecture",info:"頁面資訊",sources:"來源記憶",context:"記憶脈絡",used:"作為這些頁面的來源",recent:"最近記憶",close:"關閉"},
  {locale:"zh-Hans",open:"打开 Fixture architecture",info:"页面信息",sources:"来源记忆",context:"记忆脉络",used:"作为这些页面的来源",recent:"最近记忆",close:"关闭"},
] as const;
for (const copy of copies) for (const width of [1280,375]) {
  test(`context roundtrip ${copy.locale} ${width}`,async({page},testInfo)=>{
    const errors=collectBrowserErrors(page); await page.setViewportSize({width,height:900});
    await installTauriMock(page,{locale:copy.locale,rawActions:[]});await page.goto("/");
    await page.locator("main").getByRole("button",{name:copy.open,exact:true}).click();
    await page.getByRole("button",{name:copy.info,exact:true}).click();
    const role = width>=1100 ? "complementary" : "dialog";
    const wiki=page.getByRole(role,{name:copy.info});await expect(wiki).toBeVisible();
    await wiki.getByRole("region",{name:copy.sources,exact:true}).getByRole("button",{name:"Fixture architecture",exact:true}).press("Enter");
    await expect(page.locator(".memory-detail-reading")).toContainText("Typed fixtures");
    await expect(page.locator(".memory-detail-reading summary")).toHaveCount(0);
    if(width>=1100) await expect(page.getByRole("region",{name:copy.recent,exact:true})).toBeVisible();
    await page.getByRole("button",{name:copy.context,exact:true}).click();
    const memory=page.getByRole(role,{name:copy.context});
    await expect(memory.getByRole("region",{name:copy.used})).toBeVisible();
    await page.screenshot({path:testInfo.outputPath("memory-context.png")});
    await memory.getByRole("region",{name:copy.used}).getByRole("button",{name:"Fixture architecture",exact:true}).press("Enter");
    await expect(page.locator(".page-detail")).toContainText("Fixture architecture");
    await expect(page.locator(".memory-detail-reading")).toHaveCount(0);
    expect(await page.evaluate(()=>document.documentElement.scrollWidth-innerWidth)).toBeLessThanOrEqual(1);
    expect(errors.pageErrors).toEqual([]);
  });
}
test("memory inventory switching protects an unfinished edit and discards only by choice",async({page})=>{
  await page.setViewportSize({width:1280,height:900});await installTauriMock(page,{locale:"en",rawActions:[]});await page.goto("/");
  await page.locator("main").getByRole("button",{name:"Open Fixture architecture",exact:true}).click();
  await page.getByRole("button",{name:"Page info",exact:true}).click();
  await page.getByRole("region",{name:"Source memories",exact:true}).getByRole("button",{name:"Fixture architecture",exact:true}).click();
  await page.getByRole("button",{name:"Edit memory",exact:true}).click();
  await page.locator(".memory-detail-editor").fill("Unfinished memory draft");
  const other=page.getByRole("region",{name:"Recent memories",exact:true}).getByRole("button",{name:"Open memory: Fixture memory 1",exact:true});
  page.once("dialog",dialog=>dialog.dismiss());await other.click();
  await expect(page.locator(".memory-detail-editor")).toHaveValue("Unfinished memory draft");
  page.once("dialog",dialog=>dialog.accept());await other.click();
  await expect(other).toHaveAttribute("aria-current","page");
  await expect(page.locator(".memory-detail-editor")).toHaveCount(0);
  await expect(page.locator(".memory-detail-reading")).not.toContainText("Unfinished memory draft");
});


test("page action Escape leaves editing intact and narrow context tabs reach the graph", async ({page}) => {
  await page.setViewportSize({width:375,height:812});
  await installTauriMock(page,{locale:"en",rawActions:[]}); await page.goto("/");
  await page.locator("main").getByRole("button",{name:"Open Fixture architecture",exact:true}).click();
  const editor=page.getByRole("textbox",{name:"Page editor",exact:true});
  await expect(editor).toBeVisible();
  await page.getByRole("button",{name:"Page actions",exact:true}).click();
  await page.getByRole("menuitem",{name:"Page info",exact:true}).press("Escape");
  await expect(page.getByRole("menu",{name:"Page actions",exact:true})).toHaveCount(0);
  await expect(editor).toBeVisible();
  await page.getByRole("button",{name:"Page info",exact:true}).click();
  await page.getByRole("dialog",{name:"Page info",exact:true}).getByRole("button",{name:"Close",exact:true}).press("Tab");
  await expect(page.locator("summary",{hasText:"Local graph"})).toBeFocused();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("group",{name:"Local graph",exact:true}).getByRole("button").first()).toBeFocused();
});
