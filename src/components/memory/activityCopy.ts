// SPDX-License-Identifier: AGPL-3.0-only
//
// Locale literals for the background-activity surfaces: the sidebar status
// line, its summary popover, and the Now section on the Activity page.
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
  // ── Tier 0, the sidebar status line ──────────────────────────────────
  state: {
    up_to_date: "Up to date",
    organizing: "Organizing",
    blocked: "Blocked",
  },
  statusLabel: "Background activity",

  // ── Tier 1, the summary popover ───────────────────────────────────────
  headline: {
    up_to_date: "Everything you have given Wenlan is organized.",
    organizing: "Wenlan is organizing what you have given it.",
    blocked: "Some organizing is waiting on you.",
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
  assetRunning: {
    memories: "{{done}} of {{total}} summarized and linked",
    entities: "{{done}} of {{total}} checked",
    pages: "{{done}} of {{total}} pages written",
  },
  assetBlockedNoModel: {
    memories:
      "{{count}} waiting: no everyday model is loaded. Choose one in Settings, Intelligence.",
    entities:
      "{{count}} waiting: no everyday model is loaded. Choose one in Settings, Intelligence.",
    pages:
      "{{count}} waiting: no page-writing model is loaded. Choose one in Settings, Intelligence.",
  },
  assetBlockedFailed: {
    memories_one:
      "{{count}} of {{total}} blocked: summarizing failed. All {{total}} are still searchable.",
    memories_other:
      "{{count}} of {{total}} blocked: summarizing failed. All {{total}} are still searchable.",
    entities:
      "{{count}} of {{total}} blocked: detection failed. Your memories are unaffected.",
    pages:
      "{{count}} of {{total}} blocked: page writing failed. Your sources are unaffected.",
  },
  openActivity: "Open Activity",
  jobSeparator: " and ",
  lastActivity: "Last activity {{time}}",
  neverActive: "No background work yet",

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
  stepCount: "{{done}} of {{total}}",
  lane: {
    on_device: "on this machine",
    external: "on your local server",
    anthropic: "on Anthropic",
    basic: "built in, no model",
    none: "no model loaded",
  },
  models:
    "Everyday work: {{everyday}} {{everydayLane}}. Page writing: {{synthesis}} {{synthesisLane}}.",
  modelsNone: "No model is loaded for {{job}}.",
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
    "{{count}} memory stored and searchable now. Wenlan keeps organizing it in the background. Follow along from the status line at the bottom of the sidebar.",
  importHandoff_other:
    "{{count}} memories stored and searchable now. Wenlan keeps organizing them in the background. Follow along from the status line at the bottom of the sidebar.",

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
    organizing: "整理中",
    blocked: "已暂停",
  },
  statusLabel: "后台工作",
  headline: {
    up_to_date: "你交给文澜的内容都已整理完毕。",
    organizing: "文澜正在整理你交给它的内容。",
    blocked: "部分整理工作在等你处理。",
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
    entities: "{{total}} 个中已核对 {{done}} 个",
    pages: "{{total}} 个页面中已写好 {{done}} 个",
  },
  assetBlockedNoModel: {
    memories: "{{count}} 条在等待：未加载日常模型。请在「设置 - 智能」中选择一个。",
    entities: "{{count}} 个在等待：未加载日常模型。请在「设置 - 智能」中选择一个。",
    pages: "{{count}} 个在等待：未加载写页面的模型。请在「设置 - 智能」中选择一个。",
  },
  assetBlockedFailed: {
    memories_one: "{{total}} 条中有 {{count}} 条暂停：生成摘要失败。全部 {{total}} 条仍可搜索。",
    memories_other: "{{total}} 条中有 {{count}} 条暂停：生成摘要失败。全部 {{total}} 条仍可搜索。",
    entities: "{{total}} 个中有 {{count}} 个暂停：识别失败。你的记忆不受影响。",
    pages: "{{total}} 个中有 {{count}} 个暂停：写页面失败。你的来源不受影响。",
  },
  openActivity: "打开动态",
  jobSeparator: "和",
  lastActivity: "最近活动于{{time}}",
  neverActive: "还没有后台工作",
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
  stepCount: "{{total}} 中已完成 {{done}}",
  lane: {
    on_device: "在本机",
    external: "在你的本地服务器",
    anthropic: "在 Anthropic",
    basic: "内置，无需模型",
    none: "未加载模型",
  },
  models: "日常工作：{{everyday}}，{{everydayLane}}。写页面：{{synthesis}}，{{synthesisLane}}。",
  modelsNone: "{{job}}尚未加载模型。",
  job: {
    everyday: "日常工作",
    synthesis: "写页面",
  },
  openIntelligence: "设置 - 智能",
  trustLocal: "在本机运行，数据不会离开你的设备。",
  trustCloud: "{{jobs}}在 {{vendor}} 上运行，该步骤的文本会离开你的设备。其余全部留在本机。",
  importHandoff_one:
    "已存入 {{count}} 条记忆，现在即可搜索。文澜会在后台继续整理。可从侧栏底部的状态行查看进度。",
  importHandoff_other:
    "已存入 {{count}} 条记忆，现在即可搜索。文澜会在后台继续整理。可从侧栏底部的状态行查看进度。",
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
    organizing: "整理中",
    blocked: "已暫停",
  },
  statusLabel: "背景工作",
  headline: {
    up_to_date: "你交給文瀾的內容都已整理完畢。",
    organizing: "文瀾正在整理你交給它的內容。",
    blocked: "部分整理工作在等你處理。",
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
    entities: "{{total}} 個中已核對 {{done}} 個",
    pages: "{{total}} 個頁面中已寫好 {{done}} 個",
  },
  assetBlockedNoModel: {
    memories: "{{count}} 則在等待：未載入日常模型。請在「設定 - 智慧」中選擇一個。",
    entities: "{{count}} 個在等待：未載入日常模型。請在「設定 - 智慧」中選擇一個。",
    pages: "{{count}} 個在等待：未載入寫頁面的模型。請在「設定 - 智慧」中選擇一個。",
  },
  assetBlockedFailed: {
    memories_one: "{{total}} 則中有 {{count}} 則暫停：產生摘要失敗。全部 {{total}} 則仍可搜尋。",
    memories_other: "{{total}} 則中有 {{count}} 則暫停：產生摘要失敗。全部 {{total}} 則仍可搜尋。",
    entities: "{{total}} 個中有 {{count}} 個暫停：辨識失敗。你的記憶不受影響。",
    pages: "{{total}} 個中有 {{count}} 個暫停：寫頁面失敗。你的來源不受影響。",
  },
  openActivity: "開啟動態",
  jobSeparator: "和",
  lastActivity: "最近活動於{{time}}",
  neverActive: "還沒有背景工作",
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
  stepCount: "{{total}} 中已完成 {{done}}",
  lane: {
    on_device: "在本機",
    external: "在你的本機伺服器",
    anthropic: "在 Anthropic",
    basic: "內建，不需模型",
    none: "未載入模型",
  },
  models: "日常工作：{{everyday}}，{{everydayLane}}。寫頁面：{{synthesis}}，{{synthesisLane}}。",
  modelsNone: "{{job}}尚未載入模型。",
  job: {
    everyday: "日常工作",
    synthesis: "寫頁面",
  },
  openIntelligence: "設定 - 智慧",
  trustLocal: "在本機執行，資料不會離開你的裝置。",
  trustCloud: "{{jobs}}在 {{vendor}} 上執行，該步驟的文字會離開你的裝置。其餘全部留在本機。",
  importHandoff_one:
    "已存入 {{count}} 則記憶，現在即可搜尋。文瀾會在背景繼續整理。可從側欄底部的狀態行查看進度。",
  importHandoff_other:
    "已存入 {{count}} 則記憶，現在即可搜尋。文瀾會在背景繼續整理。可從側欄底部的狀態行查看進度。",
  layoutSetting: "動態摘要的位置",
  layoutSettingHelp: "「目前」摘要在動態頁面中的位置。僅儲存在本裝置。",
  layout: {
    rail: "右側欄",
    card: "資訊流上方的卡片",
    timeline: "嵌入資訊流",
  },
};
