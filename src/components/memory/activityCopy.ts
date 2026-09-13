// SPDX-License-Identifier: AGPL-3.0-only
//
// Locale literals for the background-activity surfaces: the toolbar Activity
// button, its summary popover, and the Now section on the Activity page.
// Imported by src/i18n/resources.ts into the `activityStatus` namespace of
// every locale. NOT `activity`: that namespace already exists and belongs to
// the Activity feed (actions, groups, filters, relative times). A second
// `activity:` key in the same object literal silently REPLACES the first, so
// reusing the name would delete the feed's copy with no error at the wiring
// site — only a wall of t() type errors in ActivityFeed.tsx. Relative times
// come from the existing `activity.relative.*`; there is one such vocabulary. The three objects must keep identical key sets (parity is enforced
// by src/i18n/resources.test.ts).
//
// Register note: the spec drafts said "on this Mac". Wenlan ships on Windows
// and Linux too, so the lane copy says "this machine", matching the wording
// already used by setup.privacyTitle and setup.intelligence.deviceNote.
// Vendor names (Anthropic, OpenAI) stay literal in every locale.
//
// No Retry copy. The spec asks for a Retry that reruns only the failed steps,
// and no such capability exists: failed enrichment rows are re-selected by the
// scheduler automatically until ENRICHMENT_MAX_ATTEMPTS and then become
// `abandoned`, which is terminal with no reset path (crates/wenlan-core/src/
// post_ingest.rs:107-119). There is no daemon route and no Tauri command for
// it. Shipping a button that cannot do what it says would be worse than
// leaving it out, so Retry waits for the daemon work.

export const enActivityStatus = {
  // ── Tier 0, the toolbar Activity button ────────────────────────────────
  state: {
    up_to_date: "Up to date",
    organizing: "Steeping",
    blocked: "Blocked",
  },
  statusLabel: "Background activity",
  // The toolbar button's accessible name: its visible word plus the state,
  // which sighted users read from the dot and the tooltip.
  buttonLabel: "Activity, {{state}}",

  // ── Tier 1, the summary popover ───────────────────────────────────────
  headline: {
    up_to_date: "Everything you have given Wenlan has steeped.",
    organizing: "Wenlan is steeping what you have given it. It works while your computer is quiet, so it can pause while you use it.",
    blocked: "Some steeping is waiting on you.",
  },
  asset: {
    memories: "Memories",
    entities: "Entities",
    pages: "Pages",
  },
  assetDone: {
    memories_one: "{{count}} stored, summarized and linked",
    memories_other: "{{count}} stored, all summarized and linked",
    entities: "{{count}} found, {{confirmed}} confirmed in the Wiki",
    pages_one: "{{count}} page written, current",
    pages_other: "{{count}} pages written, all current",
  },
  assetEmpty: {
    memories: "Nothing stored yet",
    entities: "Nothing found yet",
    pages: "No pages yet",
  },
  // Every number names its unit. Entities' running and blocked counts are
  // MEMORIES scanned for entities, not entities: one memory can name several
  // entities or none, so "8" beside "Entities" would be a wrong number.
  assetRunning: {
    memories: "{{done}} of {{total}} summarized and linked",
    entities_one: "{{done}} of {{total}} memory scanned for entities",
    entities_other: "{{done}} of {{total}} memories scanned for entities",
    pages: "{{done}} of {{total}} pages current, the rest updating",
  },
  // Rows say only how many are waiting, in their own unit. The cause and the
  // fix are said ONCE, per missing model, by blockedCause: Memories and
  // Entities share the everyday model, so a per-row cause printed the same
  // sentence twice.
  assetBlockedNoModel: {
    memories: "{{count}} not yet summarized. All are searchable now.",
    entities_one: "{{count}} memory not yet scanned for entities",
    entities_other: "{{count}} memories not yet scanned for entities",
    pages_one: "{{count}} page waiting to be updated",
    pages_other: "{{count}} pages waiting to be updated",
  },
  // The popover opens on this, with turnOnModel beside it, so it names what
  // stopped and why; the button carries the fix. turnOnModel repeats Home's
  // empty-state button word for word: one action, one name.
  blockedCause: {
    everyday: "Steeping is paused: no everyday model is chosen.",
    synthesis: "Page writing is paused: no page-writing model is chosen.",
  },
  // A model was chosen but is not serving: still loading, or its server is
  // down. The fix is the same page, so turnOnModel still applies.
  blockedCauseUnavailable: {
    everyday: "Steeping is paused: the everyday model is not available.",
    synthesis: "Page writing is paused: the page-writing model is not available.",
  },
  turnOnModel: "Turn on a model",
  assetBlockedFailed: {
    memories_one:
      "{{count}} of {{total}} blocked: summarizing failed. All {{total}} are still searchable.",
    memories_other:
      "{{count}} of {{total}} blocked: summarizing failed. All {{total}} are still searchable.",
    entities:
      "{{count}} of {{total}} memories blocked: entity detection failed. Your memories are unaffected.",
    pages:
      "{{count}} of {{total}} pages blocked: page writing failed. Your sources are unaffected.",
  },
  openActivity: "See all activity",
  jobSeparator: " and ",
  lastActivity: "Last activity {{time}}",
  // Pairs with lastActivity, so it reports when work last RAN. It must not say
  // there is no background work: this string shows exactly when nothing has
  // finished, which is also when a blocked library has the most work waiting.
  neverActive: "Nothing has run yet",

  // ── Tier 2, the Now section on the Activity page ───────────────────────
  nowTitle: "Now",
  showSteps: "Show steps",
  hideSteps: "Hide steps",
  step: {
    store: "Store",
    summarize: "Summarize",
    link: "Link",
    detect: "Detect",
    confirm: "Confirm",
    write: "Write",
  },
  stepValue: {
    store: "saved and searchable right away",
    summarize: "summary and tags so recall is accurate",
    link: "connected to related memories and entities",
    detect: "people, projects and tools you mention",
    confirm: "earned enough substance to appear in the Wiki",
    write: "pages written and refreshed from your sources",
  },
  stepState: {
    idle: "Idle",
    running: "Running",
    blocked: "Blocked",
  },
  // Per step, in that step's unit: Detect counts memories, Confirm entities.
  stepCount: {
    memories_one: "{{done}} of {{count}} memory",
    memories_other: "{{done}} of {{count}} memories",
    entities_one: "{{done}} of {{count}} entity",
    entities_other: "{{done}} of {{count}} entities",
    pages_one: "{{done}} of {{count}} page",
    pages_other: "{{done}} of {{count}} pages",
  },
  lane: {
    on_device: "on this machine",
    external: "on your local server",
    anthropic: "on Anthropic",
    basic: "built in, no model",
    none: "no model in use",
  },
  // One sentence per route. A route with no model prints its lane alone:
  // interpolating a "no model" placeholder AND the lane read "no model in use
  // built in, no model".
  route: "{{job}}: {{model}} {{lane}}.",
  routeNoModel: "{{job}}: {{lane}}.",
  jobTitle: {
    everyday: "Everyday work",
    synthesis: "Page writing",
  },
  job: {
    everyday: "everyday work",
    synthesis: "page writing",
  },
  openIntelligence: "Settings, Intelligence",
  trustLocal: "Runs on this machine. Nothing leaves your device.",
  trustCloud:
    "{{jobs}} runs on {{vendor}}; the text for that step leaves your device. Everything else stays on this machine.",

  // ── Import handoff ────────────────────────────────────────────────────
  importHandoff_one:
    "{{count}} memory stored and searchable now. Wenlan keeps steeping it in the background. Follow along from Activity in the toolbar.",
  importHandoff_other:
    "{{count}} memories stored and searchable now. Wenlan keeps steeping them in the background. Follow along from Activity in the toolbar.",

  // ── Settings, Appearance ──────────────────────────────────────────────
  layoutSetting: "Activity summary placement",
  layoutSettingHelp:
    "Where the Now summary sits on the Activity page. Saved for this device.",
  layout: {
    rail: "Side rail",
    card: "Card above the feed",
    timeline: "In the feed",
  },
};

export const hansActivityStatus = {
  state: {
    up_to_date: "已就绪",
    organizing: "沉淀中",
    blocked: "已暂停",
  },
  statusLabel: "后台工作",
  buttonLabel: "活动，{{state}}",
  headline: {
    up_to_date: "你交给文澜的内容都已沉淀完毕。",
    organizing: "文澜正在沉淀你交给它的内容。它在电脑空闲时工作，你使用电脑时可能会暂停。",
    blocked: "部分沉淀工作在等你处理。",
  },
  asset: {
    memories: "记忆",
    entities: "实体",
    pages: "页面",
  },
  assetDone: {
    memories_one: "已存 {{count}} 条，全部已生成摘要并建立关联",
    memories_other: "已存 {{count}} 条，全部已生成摘要并建立关联",
    entities: "发现 {{count}} 个，其中 {{confirmed}} 个已收入维基",
    pages_one: "已写 {{count}} 个页面，全部为最新",
    pages_other: "已写 {{count}} 个页面，全部为最新",
  },
  assetEmpty: {
    memories: "还没有存入内容",
    entities: "还没有发现实体",
    pages: "还没有页面",
  },
  assetRunning: {
    memories: "{{total}} 条中已完成 {{done}} 条的摘要与关联",
    entities_one: "{{total}} 条记忆中已有 {{done}} 条完成实体扫描",
    entities_other: "{{total}} 条记忆中已有 {{done}} 条完成实体扫描",
    pages: "{{total}} 个页面中 {{done}} 个为最新，其余正在更新",
  },
  assetBlockedNoModel: {
    memories: "{{count}} 条尚未生成摘要。全部已可搜索。",
    entities_one: "{{count}} 条记忆尚未扫描实体",
    entities_other: "{{count}} 条记忆尚未扫描实体",
    pages_one: "{{count}} 个页面等待更新",
    pages_other: "{{count}} 个页面等待更新",
  },
  blockedCause: {
    everyday: "沉淀已暂停：尚未选择日常模型。",
    synthesis: "页面撰写已暂停：尚未选择写页面的模型。",
  },
  blockedCauseUnavailable: {
    everyday: "沉淀已暂停：日常模型目前无法使用。",
    synthesis: "页面撰写已暂停：写页面的模型目前无法使用。",
  },
  turnOnModel: "启用模型",
  assetBlockedFailed: {
    memories_one: "{{total}} 条中有 {{count}} 条暂停：生成摘要失败。全部 {{total}} 条仍可搜索。",
    memories_other: "{{total}} 条中有 {{count}} 条暂停：生成摘要失败。全部 {{total}} 条仍可搜索。",
    entities: "{{total}} 条记忆中有 {{count}} 条暂停：实体识别失败。你的记忆不受影响。",
    pages: "{{total}} 个页面中有 {{count}} 个暂停：写页面失败。你的来源不受影响。",
  },
  openActivity: "查看全部活动",
  jobSeparator: "和",
  lastActivity: "最近活动于{{time}}",
  neverActive: "尚未运行过",
  nowTitle: "当前",
  showSteps: "展开步骤",
  hideSteps: "收起步骤",
  step: {
    store: "存入",
    summarize: "摘要",
    link: "关联",
    detect: "识别",
    confirm: "确认",
    write: "撰写",
  },
  stepValue: {
    store: "存入后立即可搜索",
    summarize: "生成摘要与标签，让回忆更准确",
    link: "与相关记忆和实体建立关联",
    detect: "你提到的人、项目和工具",
    confirm: "内容足够充实，可收入维基",
    write: "依据你的来源撰写并更新页面",
  },
  stepState: {
    idle: "待机",
    running: "进行中",
    blocked: "已暂停",
  },
  stepCount: {
    memories_one: "{{count}} 条记忆中已完成 {{done}} 条",
    memories_other: "{{count}} 条记忆中已完成 {{done}} 条",
    entities_one: "{{count}} 个实体中已完成 {{done}} 个",
    entities_other: "{{count}} 个实体中已完成 {{done}} 个",
    pages_one: "{{count}} 个页面中已完成 {{done}} 个",
    pages_other: "{{count}} 个页面中已完成 {{done}} 个",
  },
  lane: {
    on_device: "在本机",
    external: "在你的本地服务器",
    anthropic: "在 Anthropic",
    basic: "内置，无需模型",
    none: "未使用模型",
  },
  route: "{{job}}：{{model}}，{{lane}}。",
  routeNoModel: "{{job}}：{{lane}}。",
  jobTitle: {
    everyday: "日常工作",
    synthesis: "写页面",
  },
  job: {
    everyday: "日常工作",
    synthesis: "写页面",
  },
  openIntelligence: "设置 - 智能",
  trustLocal: "在本机运行，数据不会离开你的设备。",
  trustCloud: "{{jobs}}在 {{vendor}} 上运行，该步骤的文本会离开你的设备。其余全部留在本机。",
  importHandoff_one:
    "已存入 {{count}} 条记忆，现在即可搜索。文澜会在后台继续沉淀。可从工具栏的「活动」查看进度。",
  importHandoff_other:
    "已存入 {{count}} 条记忆，现在即可搜索。文澜会在后台继续沉淀。可从工具栏的「活动」查看进度。",
  layoutSetting: "动态摘要的位置",
  layoutSettingHelp: "「当前」摘要在动态页面中的位置。仅保存在本设备。",
  layout: {
    rail: "右侧栏",
    card: "信息流上方的卡片",
    timeline: "嵌入信息流",
  },
};

export const hantActivityStatus = {
  state: {
    up_to_date: "已就緒",
    organizing: "沉澱中",
    blocked: "已暫停",
  },
  statusLabel: "背景工作",
  buttonLabel: "活動，{{state}}",
  headline: {
    up_to_date: "你交給文瀾的內容都已沉澱完畢。",
    organizing: "文瀾正在沉澱你交給它的內容。它在電腦閒置時工作，你使用電腦時可能會暫停。",
    blocked: "部分沉澱工作在等你處理。",
  },
  asset: {
    memories: "記憶",
    entities: "實體",
    pages: "頁面",
  },
  assetDone: {
    memories_one: "已存 {{count}} 則，全部已產生摘要並建立關聯",
    memories_other: "已存 {{count}} 則，全部已產生摘要並建立關聯",
    entities: "發現 {{count}} 個，其中 {{confirmed}} 個已收入維基",
    pages_one: "已寫 {{count}} 個頁面，全部為最新",
    pages_other: "已寫 {{count}} 個頁面，全部為最新",
  },
  assetEmpty: {
    memories: "還沒有存入內容",
    entities: "還沒有發現實體",
    pages: "還沒有頁面",
  },
  assetRunning: {
    memories: "{{total}} 則中已完成 {{done}} 則的摘要與關聯",
    entities_one: "{{total}} 則記憶中已有 {{done}} 則完成實體掃描",
    entities_other: "{{total}} 則記憶中已有 {{done}} 則完成實體掃描",
    pages: "{{total}} 個頁面中 {{done}} 個為最新，其餘正在更新",
  },
  assetBlockedNoModel: {
    memories: "{{count}} 則尚未產生摘要。全部已可搜尋。",
    entities_one: "{{count}} 則記憶尚未掃描實體",
    entities_other: "{{count}} 則記憶尚未掃描實體",
    pages_one: "{{count}} 個頁面等待更新",
    pages_other: "{{count}} 個頁面等待更新",
  },
  blockedCause: {
    everyday: "沉澱已暫停：尚未選擇日常模型。",
    synthesis: "頁面撰寫已暫停：尚未選擇寫頁面的模型。",
  },
  blockedCauseUnavailable: {
    everyday: "沉澱已暫停：日常模型目前無法使用。",
    synthesis: "頁面撰寫已暫停：寫頁面的模型目前無法使用。",
  },
  turnOnModel: "啟用模型",
  assetBlockedFailed: {
    memories_one: "{{total}} 則中有 {{count}} 則暫停：產生摘要失敗。全部 {{total}} 則仍可搜尋。",
    memories_other: "{{total}} 則中有 {{count}} 則暫停：產生摘要失敗。全部 {{total}} 則仍可搜尋。",
    entities: "{{total}} 則記憶中有 {{count}} 則暫停：實體辨識失敗。你的記憶不受影響。",
    pages: "{{total}} 個頁面中有 {{count}} 個暫停：寫頁面失敗。你的來源不受影響。",
  },
  openActivity: "查看全部活動",
  jobSeparator: "和",
  lastActivity: "最近活動於{{time}}",
  neverActive: "尚未執行過",
  nowTitle: "目前",
  showSteps: "展開步驟",
  hideSteps: "收合步驟",
  step: {
    store: "存入",
    summarize: "摘要",
    link: "關聯",
    detect: "辨識",
    confirm: "確認",
    write: "撰寫",
  },
  stepValue: {
    store: "存入後立即可搜尋",
    summarize: "產生摘要與標籤，讓回想更準確",
    link: "與相關記憶和實體建立關聯",
    detect: "你提到的人、專案和工具",
    confirm: "內容足夠充實，可收入維基",
    write: "依據你的來源撰寫並更新頁面",
  },
  stepState: {
    idle: "待機",
    running: "進行中",
    blocked: "已暫停",
  },
  stepCount: {
    memories_one: "{{count}} 則記憶中已完成 {{done}} 則",
    memories_other: "{{count}} 則記憶中已完成 {{done}} 則",
    entities_one: "{{count}} 個實體中已完成 {{done}} 個",
    entities_other: "{{count}} 個實體中已完成 {{done}} 個",
    pages_one: "{{count}} 個頁面中已完成 {{done}} 個",
    pages_other: "{{count}} 個頁面中已完成 {{done}} 個",
  },
  lane: {
    on_device: "在本機",
    external: "在你的本機伺服器",
    anthropic: "在 Anthropic",
    basic: "內建，不需模型",
    none: "未使用模型",
  },
  route: "{{job}}：{{model}}，{{lane}}。",
  routeNoModel: "{{job}}：{{lane}}。",
  jobTitle: {
    everyday: "日常工作",
    synthesis: "寫頁面",
  },
  job: {
    everyday: "日常工作",
    synthesis: "寫頁面",
  },
  openIntelligence: "設定 - 智慧",
  trustLocal: "在本機執行，資料不會離開你的裝置。",
  trustCloud: "{{jobs}}在 {{vendor}} 上執行，該步驟的文字會離開你的裝置。其餘全部留在本機。",
  importHandoff_one:
    "已存入 {{count}} 則記憶，現在即可搜尋。文瀾會在背景繼續沉澱。可從工具列的「活動」查看進度。",
  importHandoff_other:
    "已存入 {{count}} 則記憶，現在即可搜尋。文瀾會在背景繼續沉澱。可從工具列的「活動」查看進度。",
  layoutSetting: "動態摘要的位置",
  layoutSettingHelp: "「目前」摘要在動態頁面中的位置。僅儲存在本裝置。",
  layout: {
    rail: "右側欄",
    card: "資訊流上方的卡片",
    timeline: "嵌入資訊流",
  },
};
