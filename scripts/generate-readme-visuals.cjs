#!/usr/bin/env node

const fs = require("node:fs");
const path = require("node:path");

const ROOT = path.resolve(__dirname, "..");
const ASSET_DIR = path.join(ROOT, "docs", "assets");

const VIEWPORTS = {
  banner: {
    desktop: { width: 1280, height: 440 },
    mobile: { width: 720, height: 300 },
  },
  overview: {
    desktop: { width: 1600, height: 1040 },
    mobile: { width: 720, height: 1700 },
  },
  lifecycle: {
    desktop: { width: 1800, height: 1260 },
    mobile: { width: 720, height: 2380 },
  },
};

const TOKENS = {
  dark: "#101024",
  darkPanel: "#1A1A2E",
  darkStroke: "#2F3769",
  white: "#F7F8FF",
  cyan: "#93E3F2",
  blue: "#5BA3E6",
  violet: "#6C63FF",
  aqua: "#4AC8E8",
  canvas: "#F7F8FC",
  surface: "#FFFFFF",
  ink: "#17182E",
  text: "#25283B",
  muted: "#62677D",
  line: "#D9DCE8",
  blueSoft: "#EAF4FF",
  blueInk: "#245F9C",
  blueLine: "#A8D0F3",
  violetSoft: "#F0EEFF",
  violetInk: "#5048C8",
  violetLine: "#C8C2FA",
  green: "#3CB878",
  greenSoft: "#EAF8F2",
  greenInk: "#187347",
  greenLine: "#A9DDC4",
  amber: "#F0A43A",
  amberSoft: "#FFF5E5",
  amberInk: "#9B5A0A",
  amberLine: "#F0D3A8",
  red: "#D75A62",
  redSoft: "#FFF0F1",
  latin: 'Arial, "Helvetica Neue", sans-serif',
  cjk: '"PingFang SC", "PingFang TC", "Hiragino Sans GB", Arial, sans-serif',
};

const REQUIRED_BANNER_COPY = [
  "WENLAN",
  "Your source-backed knowledge base,",
  "built to compound.",
];

const COPY = {
  en: {
    overviewTitle: "From scattered work to knowledge you can use again",
    overviewSubtitle: "Sources, what your work teaches you, and the pages they support stay connected.",
    overviewMobileTitle: ["From scattered work to knowledge", "you can use again"],
    overviewMobileSubtitle: ["Sources, memories, and maintained pages", "stay connected."],
    sources: "SOURCES",
    sourcesLead: "What you already have",
    sourcesItems: ["Documents and PDFs", "Obsidian and Markdown notes", "Past AI conversations"],
    memories: "MEMORIES",
    memoriesLead: "What ongoing work teaches you",
    memoryItems: ["Decisions and lessons", "Corrections and preferences", "Project context worth reusing"],
    memoryNote: "Selective knowledge, not another transcript",
    evidence: "TRACEABLE EVIDENCE",
    evidenceLead: "Every claim keeps its source and links",
    page: "ONE MAINTAINED PAGE",
    pageTitle: "Launch strategy",
    current: "CURRENT",
    sourceCited: "SOURCE-CITED",
    pageLead: "What we know now",
    pageLines: ["Early users need continuity, not more notes.", "Lead with work that returns across AI tools.", "Ask only when sources or intent conflict."],
    sourcesUsed: "SOURCES USED",
    sourceRows: ["Product brief", "Past AI conversation", "Project decision"],
    linked: "LINKED",
    linkedItems: "beta users · launch plan · pricing",
    reuse: "USE IT AGAIN",
    reuseLead: "Current knowledge returns to the work",
    reuseSecond: "Available wherever you work",
    reuseItems: ["Brief + Recall", "Pages you can inspect", "Claude · Codex · Cursor · MCP"],
    routine: "Wenlan handles routine maintenance",
    routineLead: "Organize · connect · cite · refresh",
    noUpkeep: "No manual wiki upkeep loop",
    judgment: "You decide when judgment is needed",
    judgmentLead: "Conflicting sources · changes to your writing",
    authority: "Wenlan proposes; you keep authority",
    workflow: "DAILY WORKFLOW",
    workflowItems: ["Brief", "Work + Capture", "Handoff", "Refine + Distill", "Current knowledge"],
  },
  "zh-Hans": {
    overviewTitle: "让散落的工作，变成下次真正用得上的知识",
    overviewSubtitle: "来源、工作中累积的记忆，以及它们支持的页面，始终连在一起。",
    overviewMobileTitle: ["让散落的工作，变成下次", "真正用得上的知识"],
    overviewMobileSubtitle: ["来源、记忆与持续维护的页面，", "始终连在一起。"],
    sources: "来源",
    sourcesLead: "你已经拥有的材料",
    sourcesItems: ["文档与 PDF", "Obsidian 与 Markdown 笔记", "过去的 AI 对话"],
    memories: "记忆",
    memoriesLead: "持续工作中值得留下的事",
    memoryItems: ["决策与经验", "修正与偏好", "值得复用的项目脉络"],
    memoryNote: "只保留重要知识，不是再存一份聊天记录",
    evidence: "可追溯依据",
    evidenceLead: "每个结论都保留来源与关联",
    page: "一篇持续维护的页面",
    pageTitle: "发布策略",
    current: "当前",
    sourceCited: "有来源引用",
    pageLead: "现在已知的事",
    pageLines: ["早期用户需要的是延续，而不是更多笔记。", "重点是让成果跨 AI 工具再次出现。", "只有来源或意图冲突时才需要询问。"],
    sourcesUsed: "引用来源",
    sourceRows: ["产品简报", "过去的 AI 对话", "项目决策"],
    linked: "相关联",
    linkedItems: "测试用户 · 发布计划 · 定价",
    reuse: "再次用起来",
    reuseLead: "最新知识回到实际工作里",
    reuseSecond: "跨工具回到你正在做的事",
    reuseItems: ["Brief + Recall", "可检查的 Pages", "Claude · Codex · Cursor · MCP"],
    routine: "日常维护交给 Wenlan",
    routineLead: "整理 · 连接 · 引用 · 更新",
    noUpkeep: "不用自己反复维护整套知识库",
    judgment: "需要判断时由你决定",
    judgmentLead: "来源冲突 · 改动你的文字",
    authority: "Wenlan 提出建议，最终由你做主",
    workflow: "日常流程",
    workflowItems: ["Brief", "工作 + Capture", "Handoff", "Refine + Distill", "最新知识"],
  },
  "zh-Hant": {
    overviewTitle: "讓散落的工作，變成下次真正用得上的知識",
    overviewSubtitle: "來源、工作中累積的記憶，以及它們支持的頁面，始終連在一起。",
    overviewMobileTitle: ["讓散落的工作，變成下次", "真正用得上的知識"],
    overviewMobileSubtitle: ["來源、記憶與持續維護的頁面，", "始終連在一起。"],
    sources: "來源",
    sourcesLead: "你已經擁有的材料",
    sourcesItems: ["文件與 PDF", "Obsidian 與 Markdown 筆記", "過去的 AI 對話"],
    memories: "記憶",
    memoriesLead: "持續工作中值得留下的事",
    memoryItems: ["決策與經驗", "修正與偏好", "值得複用的專案脈絡"],
    memoryNote: "只保留重要知識，不是再存一份聊天記錄",
    evidence: "可追溯依據",
    evidenceLead: "每個結論都保留來源與關聯",
    page: "一篇持續維護的頁面",
    pageTitle: "發布策略",
    current: "目前",
    sourceCited: "有來源引用",
    pageLead: "現在已知的事",
    pageLines: ["早期使用者需要的是延續，而不是更多筆記。", "重點是讓成果跨 AI 工具再次出現。", "只有來源或意圖衝突時才需要詢問。"],
    sourcesUsed: "引用來源",
    sourceRows: ["產品簡報", "過去的 AI 對話", "專案決策"],
    linked: "相關聯",
    linkedItems: "測試使用者 · 發布計畫 · 定價",
    reuse: "再次用起來",
    reuseLead: "最新知識回到實際工作裡",
    reuseSecond: "跨工具回到你正在做的事",
    reuseItems: ["Brief + Recall", "可檢查的 Pages", "Claude · Codex · Cursor · MCP"],
    routine: "日常維護交給 Wenlan",
    routineLead: "整理 · 連接 · 引用 · 更新",
    noUpkeep: "不用自己反覆維護整套知識庫",
    judgment: "需要判斷時由你決定",
    judgmentLead: "來源衝突 · 改動你的文字",
    authority: "Wenlan 提出建議，最終由你做主",
    workflow: "日常流程",
    workflowItems: ["Brief", "工作 + Capture", "Handoff", "Refine + Distill", "最新知識"],
  },
};

const LIFECYCLE_COPY = {
  en: {
    title: "How Wenlan keeps knowledge current without erasing its history",
    subtitle: "Evidence changes, pages respond, and only ambiguous decisions reach you.",
    legend: { core: "Core", llm: "LLM-assisted", human: "Human judgment", optional: "Optional · default off" },
    short: { core: "CORE", llm: "LLM", human: "HUMAN", optional: "OPTIONAL" },
    evidenceLane: "1 · EVIDENCE + MEMORY",
    evidenceLead: "Keep what matters, where it came from, and what changed.",
    capture: ["CAPTURE / IMPORT", "Sources + live AI work"],
    classify: ["CLASSIFY + EXTRACT", "type · facts · importance"],
    enrich: ["ENRICH + TAG", "context · confidence · links"],
    entity: ["ENTITY LINK", "people · projects · concepts"],
    stability: ["STABILITY", "new → learned → confirmed"],
    stabilityNote: "Promote when uncontradicted; confirm by review",
    correct: ["CORRECT + SUPERSEDE", "Keep the old claim and its replacement linked"],
    conflict: ["CONTRADICTION REVIEW", "Protected collisions never auto-mutate"],
    dual: ["DUAL-POOL RESOLVE", "Near duplicates + same-subject contradictions"],
    dualNote: "Soft-suppress, never delete; protected knowledge stays untouched",
    affected: "Evidence changes mark affected pages stale",
    pagesLane: "2 · MAINTAINED PAGES",
    pagesLead: "Build from citations, refresh selectively, preserve authorship.",
    pageSteps: [
      ["ATTACH / CREATE / GROW", "Route evidence to the right page"],
      ["SOURCE LINKS + CITATIONS", "Claims stay inspectable"],
      ["STALE", "Know which sources changed"],
      ["RE-DISTILL + VERIFY", "Refresh only supported claims"],
      ["STABLE IDENTITY", "Version + changelog history"],
    ],
    humanPage: ["HUMAN-OWNED PAGE", "Machine changes become a pending revision"],
    decision: ["ACCEPT / DISMISS", "Your prose is never silently overwritten"],
    currentRetrieval: "Accepted knowledge returns through Brief · Recall · Pages · MCP",
    refineryLane: "3 · BACKGROUND REFINERY",
    refineryLead: "The right maintenance runs at the right moment; backstop catches missed work.",
    burstTitle: "AFTER WORK",
    burstItems: ["Recaps", "Refinement queue"],
    idleTitle: "WHEN IDLE",
    idleItems: ["Community detection", "Detect page candidates", "Emergence", "Summary rollup¹", "Re-distill", "Overview", "Decision logs"],
    dailyTitle: "DAILY",
    dailyItems: ["Confidence decay", "Promote", "Reweave links", "Re-embed", "Entity extraction", "Overview", "Prune rejections", "Eviction²", "KG rethink"],
    backstopTitle: "BACKSTOP",
    backstopItems: ["All phases", "Periodic safety net"],
    backstopNote: "Runs the complete phase set",
    refineryFootnote: "¹ Summary rollup: optional, default off   ·   ² Eviction: optional, default off; archive, never delete",
  },
  "zh-Hans": {
    title: "Wenlan 如何让知识持续更新，又不抹掉历史",
    subtitle: "依据改变，页面随之更新；只有无法安全判断的事才会交给你。",
    legend: { core: "核心流程", llm: "LLM 辅助", human: "人工判断", optional: "可选 · 默认关闭" },
    short: { core: "核心", llm: "LLM", human: "人工", optional: "可选" },
    evidenceLane: "1 · 依据与记忆",
    evidenceLead: "保留重要知识、它的来源，以及后来发生的变化。",
    capture: ["捕获 / 导入", "来源资料 + 进行中的 AI 工作"],
    classify: ["分类 + 抽取", "类型 · 事实 · 重要性"],
    enrich: ["丰富 + 标签", "脉络 · 可信度 · 关联"],
    entity: ["实体关联", "人物 · 项目 · 概念"],
    stability: ["稳定度", "new → learned → confirmed"],
    stabilityNote: "没有矛盾时晋级；经检查后确认",
    correct: ["更正 + 取代", "旧说法与替代它的新说法保持关联"],
    conflict: ["矛盾审查", "受保护内容冲突时绝不自动改写"],
    dual: ["双池解析", "近似重复 + 同主题矛盾"],
    dualNote: "只软隐藏、不删除；受保护知识保持不动",
    affected: "依据改变后，受影响的页面会被标记为过时",
    pagesLane: "2 · 持续维护的页面",
    pagesLead: "从引用构建，按需更新，同时保护作者身份。",
    pageSteps: [
      ["附加 / 新建 / 扩写", "把依据送到正确页面"],
      ["来源链接 + 引用", "每个结论都可检查"],
      ["过时", "知道哪些来源变了"],
      ["重新蒸馏 + 验证", "只更新有依据的结论"],
      ["稳定身份", "版本 + 变更历史"],
    ],
    humanPage: ["人工编辑的页面", "机器改动会变成待审修订"],
    decision: ["接受 / 忽略", "你的文字绝不会被静默覆盖"],
    currentRetrieval: "通过 Brief · Recall · Pages · MCP 回到后续工作",
    refineryLane: "3 · 后台精炼",
    refineryLead: "在合适时机做合适维护；Backstop 补上遗漏的工作。",
    burstTitle: "工作结束后",
    burstItems: ["工作回顾 (Recaps)", "精炼队列"],
    idleTitle: "空闲时",
    idleItems: ["社群侦测", "页面候选侦测", "新页面涌现", "摘要汇总¹", "重新蒸馏", "Overview 更新", "决策日志"],
    dailyTitle: "每天",
    dailyItems: ["信心衰减", "晋级", "重织关联", "重新嵌入", "实体抽取", "Overview 更新", "清理拒绝记录", "封存²", "KG 重思"],
    backstopTitle: "BACKSTOP",
    backstopItems: ["全部阶段", "周期性安全网"],
    backstopNote: "运行完整阶段集合",
    refineryFootnote: "¹ Summary rollup：可选、默认关闭   ·   ² Eviction：可选、默认关闭；只封存，绝不删除",
  },
  "zh-Hant": {
    title: "Wenlan 如何讓知識持續更新，又不抹掉歷史",
    subtitle: "依據改變，頁面隨之更新；只有無法安全判斷的事才會交給你。",
    legend: { core: "核心流程", llm: "LLM 輔助", human: "人工判斷", optional: "可選 · 預設關閉" },
    short: { core: "核心", llm: "LLM", human: "人工", optional: "可選" },
    evidenceLane: "1 · 依據與記憶",
    evidenceLead: "保留重要知識、它的來源，以及後來發生的變化。",
    capture: ["捕獲 / 匯入", "來源資料 + 進行中的 AI 工作"],
    classify: ["分類 + 抽取", "類型 · 事實 · 重要性"],
    enrich: ["豐富 + 標籤", "脈絡 · 可信度 · 關聯"],
    entity: ["實體關聯", "人物 · 專案 · 概念"],
    stability: ["穩定度", "new → learned → confirmed"],
    stabilityNote: "沒有矛盾時晉級；經檢查後確認",
    correct: ["更正 + 取代", "舊說法與取代它的新說法保持關聯"],
    conflict: ["矛盾審查", "受保護內容衝突時絕不自動改寫"],
    dual: ["雙池解析", "近似重複 + 同主題矛盾"],
    dualNote: "只軟隱藏、不刪除；受保護知識保持不動",
    affected: "依據改變後，受影響的頁面會被標記為過時",
    pagesLane: "2 · 持續維護的頁面",
    pagesLead: "從引用建立，按需更新，同時保護作者身分。",
    pageSteps: [
      ["附加 / 新建 / 擴寫", "把依據送到正確頁面"],
      ["來源連結 + 引用", "每個結論都可檢查"],
      ["過時", "知道哪些來源變了"],
      ["重新蒸餾 + 驗證", "只更新有依據的結論"],
      ["穩定身分", "版本 + 變更歷史"],
    ],
    humanPage: ["人工編輯的頁面", "機器改動會變成待審修訂"],
    decision: ["接受 / 忽略", "你的文字絕不會被靜默覆蓋"],
    currentRetrieval: "透過 Brief · Recall · Pages · MCP 回到後續工作",
    refineryLane: "3 · 後台精煉",
    refineryLead: "在合適時機做合適維護；Backstop 補上遺漏的工作。",
    burstTitle: "工作結束後",
    burstItems: ["工作回顧 (Recaps)", "精煉佇列"],
    idleTitle: "閒置時",
    idleItems: ["社群偵測", "頁面候選偵測", "新頁面湧現", "摘要彙總¹", "重新蒸餾", "Overview 更新", "決策日誌"],
    dailyTitle: "每天",
    dailyItems: ["信心衰減", "晉級", "重織關聯", "重新嵌入", "實體抽取", "Overview 更新", "清理拒絕記錄", "封存²", "KG 重思"],
    backstopTitle: "BACKSTOP",
    backstopItems: ["全部階段", "週期性安全網"],
    backstopNote: "執行完整階段集合",
    refineryFootnote: "¹ Summary rollup：可選、預設關閉   ·   ² Eviction：可選、預設關閉；只封存，絕不刪除",
  },
};

function esc(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function fontFor(locale) {
  return locale === "en" ? TOKENS.latin : TOKENS.cjk;
}

function textMarkup({
  x,
  y,
  text,
  size,
  weight = 600,
  fill = TOKENS.text,
  family = TOKENS.latin,
  anchor = "start",
  letterSpacing = 0,
}) {
  return `<text x="${x}" y="${y}" fill="${fill}" font-family="${esc(family)}" font-size="${size}" font-weight="${weight}" text-anchor="${anchor}" letter-spacing="${letterSpacing}">${esc(text)}</text>`;
}

function multilineMarkup({ x, y, lines, size, lineHeight, weight = 600, fill = TOKENS.text, family = TOKENS.latin }) {
  return lines
    .map((line, index) =>
      textMarkup({ x, y: y + index * lineHeight, text: line, size, weight, fill, family }),
    )
    .join("\n");
}

function panelMarkup({ x, y, width, height, fill = TOKENS.surface, stroke = TOKENS.line, radius = 14, strokeWidth = 2 }) {
  return `<rect x="${x}" y="${y}" width="${width}" height="${height}" rx="${radius}" fill="${fill}" stroke="${stroke}" stroke-width="${strokeWidth}"/>`;
}

function pillMarkup({ x, y, width, height, label, fill, stroke, color, family, size = 17, dashed = false }) {
  return `${panelMarkup({ x, y, width, height, fill, stroke, radius: height / 2, strokeWidth: 1.5 })
    .replace("/>", dashed ? ' stroke-dasharray="7 5"/>' : "/>")}
  ${textMarkup({ x: x + width / 2, y: y + height / 2 + size * 0.35, text: label, size, weight: 720, fill: color, family, anchor: "middle" })}`;
}

function bulletRows({ x, y, width, items, fill, stroke, dot, family, size = 21, rowHeight = 58 }) {
  return items
    .map((item, index) => {
      const rowY = y + index * rowHeight;
      return `${panelMarkup({ x, y: rowY, width, height: rowHeight - 10, fill, stroke, radius: 8, strokeWidth: 1.5 })}
      <circle cx="${x + 24}" cy="${rowY + (rowHeight - 10) / 2}" r="7" fill="${dot}"/>
      ${textMarkup({ x: x + 44, y: rowY + (rowHeight - 10) / 2 + size * 0.34, text: item, size, weight: 620, fill: TOKENS.text, family })}`;
    })
    .join("\n");
}

function arrowPath({ d, marker, color = "#8B90A4", width = 3, dashed = false }) {
  return `<path d="${d}" fill="none" stroke="${color}" stroke-width="${width}" stroke-linecap="round" stroke-linejoin="round"${dashed ? ' stroke-dasharray="8 7"' : ""} marker-end="url(#${marker})"/>`;
}

function arrowMarker(id, color = "#8B90A4") {
  return `<marker id="${id}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse">
      <path d="M 0 0 L 10 5 L 0 10 z" fill="${color}"/>
    </marker>`;
}

function statusStyle(kind) {
  return {
    core: { fill: TOKENS.greenSoft, stroke: TOKENS.greenLine, color: TOKENS.greenInk, dot: TOKENS.green },
    llm: { fill: TOKENS.violetSoft, stroke: TOKENS.violetLine, color: TOKENS.violetInk, dot: TOKENS.violet },
    human: { fill: TOKENS.amberSoft, stroke: TOKENS.amberLine, color: TOKENS.amberInk, dot: TOKENS.amber },
    optional: { fill: "#F4F5F8", stroke: "#8B90A4", color: TOKENS.muted, dot: "#8B90A4" },
  }[kind];
}

function statusBadge({ x, y, label, kind, family, compact = true }) {
  const style = statusStyle(kind);
  const width = compact ? Math.max(58, label.length * (label.includes("LLM") ? 8 : 9) + 24) : Math.max(88, label.length * 9 + 34);
  return {
    width,
    markup: pillMarkup({
      x,
      y,
      width,
      height: compact ? 28 : 34,
      label,
      fill: style.fill,
      stroke: style.stroke,
      color: style.color,
      family,
      size: compact ? 12 : 14,
      dashed: kind === "optional",
    }),
  };
}

function processCard({ x, y, width, height, title, sub, kind, labels, family, titleSize = 18, subSize = 14 }) {
  const style = statusStyle(kind);
  const badge = statusBadge({ x: 0, y: 0, label: labels[kind], kind, family, compact: true });
  const badgeX = x + width - badge.width - 14;
  const subLines = Array.isArray(sub) ? sub : [sub];
  return `${panelMarkup({ x, y, width, height, fill: TOKENS.surface, stroke: style.stroke, radius: 10, strokeWidth: 1.8 })}
  ${textMarkup({ x: x + 18, y: y + 31, text: title, size: titleSize, weight: 740, fill: style.color, family, letterSpacing: 0.3 })}
  ${multilineMarkup({ x: x + 18, y: y + 58, lines: subLines, size: subSize, lineHeight: subSize + 5, weight: 560, fill: TOKENS.text, family })}
  ${statusBadge({ x: badgeX, y: y + height - 38, label: labels[kind], kind, family, compact: true }).markup}`;
}

function wrapShort(text, locale, maxEnglish = 34, maxCjk = 16) {
  const limit = locale === "en" ? maxEnglish : maxCjk;
  if (text.length <= limit) return [text];
  if (locale !== "en") return [text.slice(0, limit), text.slice(limit)];
  const lines = [];
  let current = "";
  for (const word of text.split(" ")) {
    const next = current ? `${current} ${word}` : word;
    if (next.length > limit && current) {
      lines.push(current);
      current = word;
    } else {
      current = next;
    }
  }
  if (current) lines.push(current);
  return lines;
}

function phaseRowsMarkup({ x, y, items, kinds, family, size = 16, lineHeight = 23 }) {
  return items
    .map((item, index) => {
      const kind = kinds[index];
      const style = statusStyle(kind);
      const rowY = y + index * lineHeight;
      const optionalRing = kind === "optional"
        ? `<circle cx="${x}" cy="${rowY - 5}" r="6" fill="none" stroke="${style.dot}" stroke-width="2" stroke-dasharray="3 2"/>`
        : `<circle cx="${x}" cy="${rowY - 5}" r="6" fill="${style.dot}"/>`;
      return `${optionalRing}
      ${textMarkup({ x: x + 18, y: rowY, text: item, size, weight: 600, fill: TOKENS.text, family })}`;
    })
    .join("\n");
}

function logoDefs(prefix) {
  return `<linearGradient id="${prefix}-ring" x1="96" y1="256" x2="416" y2="256" gradientUnits="userSpaceOnUse">
      <stop stop-color="#6C63FF"/>
      <stop offset="0.5" stop-color="#5BA3E6"/>
      <stop offset="1" stop-color="#4AC8E8"/>
    </linearGradient>
    <radialGradient id="${prefix}-orb" cx="0" cy="0" r="1" gradientUnits="userSpaceOnUse" gradientTransform="translate(310 144) rotate(52.125) scale(76.0263)">
      <stop stop-color="#FFFFFF"/>
      <stop offset="0.45" stop-color="#A5C4F7"/>
      <stop offset="1" stop-color="#4AC8E8"/>
    </radialGradient>`;
}

function logoMarkup({ x, y, size, prefix }) {
  const scale = size / 512;
  return `<g transform="translate(${x} ${y}) scale(${scale})">
    <rect width="512" height="512" rx="112" fill="#1A1A2E"/>
    <circle cx="256" cy="256" r="160" fill="none" stroke="url(#${prefix}-ring)" stroke-width="76"/>
    <circle cx="322" cy="160" r="42" fill="url(#${prefix}-orb)"/>
  </g>`;
}

function makeBanner(viewport) {
  const { width, height } = VIEWPORTS.banner[viewport];
  const mobile = viewport === "mobile";
  const prefix = `banner-${viewport}`;
  const content = mobile
    ? `${logoMarkup({ x: 294, y: 30, size: 132, prefix })}
  <text x="360" y="190" fill="${TOKENS.cyan}" font-family="${esc(TOKENS.latin)}" font-size="28" font-weight="700" text-anchor="middle" letter-spacing="4">WENLAN</text>
  <text x="360" y="232" fill="${TOKENS.white}" font-family="${esc(TOKENS.latin)}" font-size="34" font-weight="700" text-anchor="middle">Your source-backed knowledge base,</text>
  <text x="360" y="270" fill="${TOKENS.white}" font-family="${esc(TOKENS.latin)}" font-size="34" font-weight="700" text-anchor="middle">built to compound.</text>`
    : `${logoMarkup({ x: 144, y: 104, size: 232, prefix })}
  <text x="440" y="174" fill="${TOKENS.cyan}" font-family="${esc(TOKENS.latin)}" font-size="34" font-weight="700" letter-spacing="4">WENLAN</text>
  <text x="440" y="235" fill="${TOKENS.white}" font-family="${esc(TOKENS.latin)}" font-size="42" font-weight="700">Your source-backed knowledge base,</text>
  <text x="440" y="283" fill="${TOKENS.white}" font-family="${esc(TOKENS.latin)}" font-size="42" font-weight="700">built to compound.</text>`;

  const frame = mobile
    ? '<rect x="24" y="16" width="672" height="268" rx="24" fill="#101024" stroke="#2F3769"/>'
    : '<rect x="64" y="28" width="1152" height="384" rx="30" fill="#101024" stroke="#2F3769"/>';

  const svg = `<svg width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" fill="none" xmlns="http://www.w3.org/2000/svg" role="img" aria-labelledby="title desc">
  <title id="title">Wenlan README banner</title>
  <desc id="desc">Wenlan: your source-backed knowledge base, built to compound.</desc>
  <rect width="${width}" height="${height}" fill="${TOKENS.dark}"/>
  ${frame}
  ${content}
  <defs>
    ${logoDefs(prefix)}
  </defs>
</svg>
`;

  return {
    group: "banner",
    name: mobile ? "readme-banner-mobile" : "readme-banner",
    width,
    height,
    background: TOKENS.dark,
    requiredCopy: REQUIRED_BANNER_COPY,
    svg,
  };
}

function overviewDesktop(c, locale, prefix) {
  const family = fontFor(locale);
  const marker = `${prefix}-arrow`;
  const sourceRows = bulletRows({
    x: 110,
    y: 282,
    width: 290,
    items: c.sourcesItems,
    fill: TOKENS.surface,
    stroke: TOKENS.blueLine,
    dot: TOKENS.blue,
    family,
    size: locale === "en" ? 18 : 19,
    rowHeight: 50,
  });
  const memoryRows = bulletRows({
    x: 110,
    y: 548,
    width: 290,
    items: c.memoryItems,
    fill: TOKENS.surface,
    stroke: TOKENS.violetLine,
    dot: TOKENS.violet,
    family,
    size: 19,
    rowHeight: 50,
  });
  const pageLines = c.pageLines
    .map(
      (line, index) => `<circle cx="520" cy="${449 + index * 36}" r="7" fill="${TOKENS.green}"/>
      ${textMarkup({ x: 540, y: 456 + index * 36, text: line, size: 18, weight: 570, fill: TOKENS.text, family })}`,
    )
    .join("\n");
  const citationWidth = 148;
  const citations = c.sourceRows
    .map((row, index) =>
      pillMarkup({
        x: 520 + index * (citationWidth + 14),
        y: 605,
        width: citationWidth,
        height: 42,
        label: `${index + 1} · ${row}`,
        fill: TOKENS.blueSoft,
        stroke: TOKENS.blueLine,
        color: TOKENS.blueInk,
        family,
        size: locale === "en" ? 14 : 16,
      }),
    )
    .join("\n");
  const reuseRows = bulletRows({
    x: 1130,
    y: 322,
    width: 360,
    items: c.reuseItems,
    fill: TOKENS.surface,
    stroke: TOKENS.amberLine,
    dot: TOKENS.amber,
    family,
    size: locale === "en" ? 19 : 18,
    rowHeight: 70,
  });
  const workflowWidths = [142, 210, 150, 210, 210];
  const workflowColors = [
    [TOKENS.blueSoft, TOKENS.blueLine, TOKENS.blueInk],
    [TOKENS.violetSoft, TOKENS.violetLine, TOKENS.violetInk],
    [TOKENS.amberSoft, TOKENS.amberLine, TOKENS.amberInk],
    [TOKENS.greenSoft, TOKENS.greenLine, TOKENS.greenInk],
    [TOKENS.blueSoft, TOKENS.blueLine, TOKENS.blueInk],
  ];
  let workflowX = 300;
  const workflowItems = c.workflowItems
    .map((item, index) => {
      const width = workflowWidths[index];
      const [fill, stroke, color] = workflowColors[index];
      const pill = pillMarkup({ x: workflowX, y: 956, width, height: 48, label: item, fill, stroke, color, family, size: 16 });
      const arrow = index < c.workflowItems.length - 1
        ? arrowPath({ d: `M ${workflowX + width + 8} 980 H ${workflowX + width + 38}`, marker, color: "#8B90A4", width: 2.2 })
        : "";
      workflowX += width + 48;
      return `${pill}\n${arrow}`;
    })
    .join("\n");

  return `${logoMarkup({ x: 80, y: 48, size: 72, prefix })}
  ${textMarkup({ x: 180, y: 80, text: c.overviewTitle, size: locale === "en" ? 40 : 38, weight: 740, fill: TOKENS.ink, family })}
  ${textMarkup({ x: 180, y: 119, text: c.overviewSubtitle, size: 21, weight: 520, fill: TOKENS.muted, family })}

  ${arrowPath({ d: "M 430 310 C 470 310 466 215 500 215", marker, color: TOKENS.blue, width: 3 })}
  ${arrowPath({ d: "M 430 590 C 470 590 466 235 500 235", marker, color: TOKENS.violet, width: 3 })}
  ${arrowPath({ d: "M 740 250 V 278", marker, color: TOKENS.green, width: 3 })}
  ${arrowPath({ d: "M 1040 500 H 1090", marker, color: TOKENS.amber, width: 3 })}

  ${panelMarkup({ x: 80, y: 180, width: 350, height: 260, fill: TOKENS.blueSoft, stroke: TOKENS.blue, radius: 14 })}
  ${textMarkup({ x: 110, y: 220, text: c.sources, size: 18, weight: 760, fill: TOKENS.blueInk, family, letterSpacing: locale === "en" ? 1.4 : 0 })}
  ${textMarkup({ x: 110, y: 258, text: c.sourcesLead, size: 25, weight: 700, fill: TOKENS.ink, family })}
  ${sourceRows}

  ${panelMarkup({ x: 80, y: 460, width: 350, height: 260, fill: TOKENS.violetSoft, stroke: TOKENS.violet, radius: 14 })}
  ${textMarkup({ x: 110, y: 500, text: c.memories, size: 18, weight: 760, fill: TOKENS.violetInk, family, letterSpacing: locale === "en" ? 1.4 : 0 })}
  ${textMarkup({ x: 110, y: 538, text: c.memoriesLead, size: locale === "en" ? 23 : 24, weight: 700, fill: TOKENS.ink, family })}
  ${memoryRows}
  ${textMarkup({ x: 110, y: 704, text: c.memoryNote, size: locale === "en" ? 16 : 17, weight: 600, fill: TOKENS.violetInk, family })}

  ${panelMarkup({ x: 500, y: 180, width: 480, height: 70, fill: TOKENS.greenSoft, stroke: TOKENS.green, radius: 35 })}
  <circle cx="535" cy="215" r="13" fill="${TOKENS.green}"/>
  <path d="M529 215L534 220L543 209" fill="none" stroke="#FFFFFF" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
  ${textMarkup({ x: 560, y: 210, text: c.evidence, size: 17, weight: 760, fill: TOKENS.greenInk, family, letterSpacing: locale === "en" ? 1.2 : 0 })}
  ${textMarkup({ x: 560, y: 234, text: c.evidenceLead, size: 16, weight: 560, fill: TOKENS.text, family })}

  ${panelMarkup({ x: 480, y: 280, width: 560, height: 440, fill: TOKENS.surface, stroke: TOKENS.green, radius: 12, strokeWidth: 2.5 })}
  ${textMarkup({ x: 520, y: 320, text: c.page, size: 17, weight: 760, fill: TOKENS.greenInk, family, letterSpacing: locale === "en" ? 1.1 : 0 })}
  ${pillMarkup({ x: 838, y: 296, width: 92, height: 36, label: c.current, fill: TOKENS.greenSoft, stroke: TOKENS.greenLine, color: TOKENS.greenInk, family, size: 14 })}
  ${pillMarkup({ x: 938, y: 296, width: 82, height: 36, label: c.sourceCited, fill: TOKENS.blueSoft, stroke: TOKENS.blueLine, color: TOKENS.blueInk, family, size: locale === "en" ? 10 : 13 })}
  ${textMarkup({ x: 520, y: 374, text: c.pageTitle, size: 34, weight: 740, fill: TOKENS.ink, family })}
  ${textMarkup({ x: 520, y: 414, text: c.pageLead, size: 19, weight: 700, fill: TOKENS.muted, family })}
  ${pageLines}
  <line x1="520" y1="562" x2="1000" y2="562" stroke="${TOKENS.line}" stroke-width="2"/>
  ${textMarkup({ x: 520, y: 590, text: c.sourcesUsed, size: 14, weight: 760, fill: TOKENS.muted, family, letterSpacing: locale === "en" ? 1 : 0 })}
  ${citations}
  ${textMarkup({ x: 520, y: 687, text: c.linked, size: 14, weight: 760, fill: TOKENS.muted, family, letterSpacing: locale === "en" ? 1 : 0 })}
  ${textMarkup({ x: 598, y: 687, text: c.linkedItems, size: 17, weight: 600, fill: TOKENS.violetInk, family })}

  ${panelMarkup({ x: 1100, y: 180, width: 420, height: 540, fill: TOKENS.amberSoft, stroke: TOKENS.amber, radius: 14 })}
  ${textMarkup({ x: 1130, y: 225, text: c.reuse, size: 18, weight: 760, fill: TOKENS.amberInk, family, letterSpacing: locale === "en" ? 1.4 : 0 })}
  ${multilineMarkup({ x: 1130, y: 270, lines: locale === "en" ? ["Current knowledge returns", "to the work"] : [c.reuseLead], size: 27, lineHeight: 34, weight: 720, fill: TOKENS.ink, family })}
  ${reuseRows}
  ${textMarkup({ x: 1130, y: 576, text: locale === "en" ? "One local knowledge base" : c.reuseLead, size: 18, weight: 650, fill: TOKENS.amberInk, family })}
  ${textMarkup({ x: 1130, y: 610, text: c.reuseSecond, size: 18, weight: 560, fill: TOKENS.muted, family })}
  <path d="M1160 655H1460" stroke="${TOKENS.amberLine}" stroke-width="2"/>
  ${textMarkup({ x: 1310, y: 690, text: "Brief · Recall · Pages · MCP", size: 18, weight: 700, fill: TOKENS.amberInk, family, anchor: "middle" })}

  ${panelMarkup({ x: 80, y: 760, width: 1440, height: 150, fill: TOKENS.dark, stroke: TOKENS.dark, radius: 14 })}
  <line x1="800" y1="790" x2="800" y2="880" stroke="#343858" stroke-width="2"/>
  <circle cx="125" cy="807" r="17" fill="${TOKENS.green}"/>
  <path d="M117 807L123 813L134 800" fill="none" stroke="#FFFFFF" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
  ${textMarkup({ x: 160, y: 814, text: c.routine, size: 25, weight: 700, fill: TOKENS.white, family })}
  ${textMarkup({ x: 160, y: 855, text: c.routineLead, size: 20, weight: 600, fill: TOKENS.cyan, family })}
  ${textMarkup({ x: 160, y: 884, text: c.noUpkeep, size: 17, weight: 520, fill: "#A9AEC5", family })}
  <circle cx="845" cy="807" r="17" fill="${TOKENS.amber}"/>
  <path d="M845 798V810" stroke="#FFFFFF" stroke-width="3" stroke-linecap="round"/>
  <circle cx="845" cy="817" r="2" fill="#FFFFFF"/>
  ${textMarkup({ x: 880, y: 814, text: c.judgment, size: 25, weight: 700, fill: TOKENS.white, family })}
  ${textMarkup({ x: 880, y: 855, text: c.judgmentLead, size: 20, weight: 600, fill: "#FFD89B", family })}
  ${textMarkup({ x: 880, y: 884, text: c.authority, size: 17, weight: 520, fill: "#A9AEC5", family })}

  ${panelMarkup({ x: 80, y: 940, width: 1440, height: 80, fill: TOKENS.surface, stroke: TOKENS.line, radius: 12 })}
  ${textMarkup({ x: 108, y: 986, text: c.workflow, size: 16, weight: 760, fill: TOKENS.muted, family, letterSpacing: locale === "en" ? 1 : 0 })}
  ${workflowItems}`;
}

function overviewMobile(c, locale, prefix) {
  const family = fontFor(locale);
  const marker = `${prefix}-arrow`;
  const titleLines = c.overviewMobileTitle;
  const subtitleLines = c.overviewMobileSubtitle;
  const sourceRows = bulletRows({
    x: 70,
    y: 252,
    width: 580,
    items: c.sourcesItems,
    fill: TOKENS.surface,
    stroke: TOKENS.blueLine,
    dot: TOKENS.blue,
    family,
    size: 20,
    rowHeight: 45,
  });
  const memoryRows = bulletRows({
    x: 70,
    y: 497,
    width: 580,
    items: c.memoryItems,
    fill: TOKENS.surface,
    stroke: TOKENS.violetLine,
    dot: TOKENS.violet,
    family,
    size: 20,
    rowHeight: 40,
  });
  const pageLines = c.pageLines
    .map(
      (line, index) => `<circle cx="82" cy="${900 + index * 36}" r="7" fill="${TOKENS.green}"/>
      ${textMarkup({ x: 102, y: 907 + index * 36, text: line, size: locale === "en" ? 17 : 19, weight: 570, fill: TOKENS.text, family })}`,
    )
    .join("\n");
  const citations = c.sourceRows
    .map((row, index) =>
      pillMarkup({
        x: 70 + index * 194,
        y: 1026,
        width: 178,
        height: 44,
        label: `${index + 1} · ${row}`,
        fill: TOKENS.blueSoft,
        stroke: TOKENS.blueLine,
        color: TOKENS.blueInk,
        family,
        size: locale === "en" ? 13 : 16,
      }),
    )
    .join("\n");
  const reuseRows = bulletRows({
    x: 70,
    y: 1227,
    width: 580,
    items: c.reuseItems,
    fill: TOKENS.surface,
    stroke: TOKENS.amberLine,
    dot: TOKENS.amber,
    family,
    size: locale === "en" ? 19 : 18,
    rowHeight: 44,
  });
  const workflowWidth = 112;
  const workflowItems = c.workflowItems
    .map((item, index) => {
      const x = 60 + index * 126;
      const colors = [
        [TOKENS.blueSoft, TOKENS.blueLine, TOKENS.blueInk],
        [TOKENS.violetSoft, TOKENS.violetLine, TOKENS.violetInk],
        [TOKENS.amberSoft, TOKENS.amberLine, TOKENS.amberInk],
        [TOKENS.greenSoft, TOKENS.greenLine, TOKENS.greenInk],
        [TOKENS.blueSoft, TOKENS.blueLine, TOKENS.blueInk],
      ][index];
      const [fill, stroke, color] = colors;
      const arrow = index < 4
        ? arrowPath({ d: `M ${x + workflowWidth + 3} 1631 H ${x + workflowWidth + 12}`, marker, color: "#8B90A4", width: 1.8 })
        : "";
      const size = locale === "en" ? (item.length > 13 ? 10.5 : 12) : 14;
      return `${pillMarkup({ x, y: 1608, width: workflowWidth, height: 46, label: item, fill, stroke, color, family, size })}\n${arrow}`;
    })
    .join("\n");

  return `${logoMarkup({ x: 40, y: 38, size: 76, prefix })}
  ${multilineMarkup({ x: 138, y: 67, lines: titleLines, size: 29, lineHeight: 35, weight: 740, fill: TOKENS.ink, family })}
  ${multilineMarkup({ x: 138, y: 135, lines: subtitleLines, size: 17, lineHeight: 24, weight: 520, fill: TOKENS.muted, family })}

  ${arrowPath({ d: "M 360 385 V 405", marker, color: TOKENS.blue, width: 3 })}
  ${arrowPath({ d: "M 360 640 V 660", marker, color: TOKENS.violet, width: 3 })}
  ${arrowPath({ d: "M 360 737 V 765", marker, color: TOKENS.green, width: 3 })}
  ${arrowPath({ d: "M 360 1120 V 1140", marker, color: TOKENS.amber, width: 3 })}

  ${panelMarkup({ x: 40, y: 175, width: 640, height: 210, fill: TOKENS.blueSoft, stroke: TOKENS.blue, radius: 14 })}
  ${textMarkup({ x: 70, y: 216, text: c.sources, size: 19, weight: 760, fill: TOKENS.blueInk, family, letterSpacing: locale === "en" ? 1.2 : 0 })}
  ${textMarkup({ x: 70, y: 248, text: c.sourcesLead, size: 24, weight: 700, fill: TOKENS.ink, family })}
  ${sourceRows}

  ${panelMarkup({ x: 40, y: 410, width: 640, height: 230, fill: TOKENS.violetSoft, stroke: TOKENS.violet, radius: 14 })}
  ${textMarkup({ x: 70, y: 451, text: c.memories, size: 19, weight: 760, fill: TOKENS.violetInk, family, letterSpacing: locale === "en" ? 1.2 : 0 })}
  ${textMarkup({ x: 70, y: 487, text: c.memoriesLead, size: 23, weight: 700, fill: TOKENS.ink, family })}
  ${memoryRows}
  ${textMarkup({ x: 70, y: 626, text: c.memoryNote, size: locale === "en" ? 16 : 17, weight: 600, fill: TOKENS.violetInk, family })}

  ${panelMarkup({ x: 80, y: 665, width: 560, height: 72, fill: TOKENS.greenSoft, stroke: TOKENS.green, radius: 36 })}
  <circle cx="116" cy="701" r="13" fill="${TOKENS.green}"/>
  <path d="M110 701L115 706L124 695" fill="none" stroke="#FFFFFF" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
  ${textMarkup({ x: 142, y: 695, text: c.evidence, size: 18, weight: 760, fill: TOKENS.greenInk, family })}
  ${textMarkup({ x: 142, y: 720, text: c.evidenceLead, size: 16, weight: 560, fill: TOKENS.text, family })}

  ${panelMarkup({ x: 40, y: 770, width: 640, height: 350, fill: TOKENS.surface, stroke: TOKENS.green, radius: 12, strokeWidth: 2.5 })}
  ${textMarkup({ x: 70, y: 811, text: c.page, size: 18, weight: 760, fill: TOKENS.greenInk, family })}
  ${pillMarkup({ x: 494, y: 786, width: 72, height: 36, label: c.current, fill: TOKENS.greenSoft, stroke: TOKENS.greenLine, color: TOKENS.greenInk, family, size: 13 })}
  ${pillMarkup({ x: 574, y: 786, width: 86, height: 36, label: c.sourceCited, fill: TOKENS.blueSoft, stroke: TOKENS.blueLine, color: TOKENS.blueInk, family, size: locale === "en" ? 9 : 12 })}
  ${textMarkup({ x: 70, y: 858, text: c.pageTitle, size: 32, weight: 740, fill: TOKENS.ink, family })}
  ${textMarkup({ x: 70, y: 887, text: c.pageLead, size: 18, weight: 700, fill: TOKENS.muted, family })}
  ${pageLines}
  <line x1="70" y1="997" x2="650" y2="997" stroke="${TOKENS.line}" stroke-width="2"/>
  ${textMarkup({ x: 70, y: 1018, text: c.sourcesUsed, size: 14, weight: 760, fill: TOKENS.muted, family })}
  ${citations}
  ${textMarkup({ x: 70, y: 1100, text: `${c.linked} · ${c.linkedItems}`, size: 17, weight: 620, fill: TOKENS.violetInk, family })}

  ${panelMarkup({ x: 40, y: 1145, width: 640, height: 210, fill: TOKENS.amberSoft, stroke: TOKENS.amber, radius: 14 })}
  ${textMarkup({ x: 70, y: 1185, text: c.reuse, size: 19, weight: 760, fill: TOKENS.amberInk, family })}
  ${textMarkup({ x: 70, y: 1217, text: c.reuseLead, size: 23, weight: 700, fill: TOKENS.ink, family })}
  ${reuseRows}

  ${panelMarkup({ x: 40, y: 1380, width: 640, height: 165, fill: TOKENS.dark, stroke: TOKENS.dark, radius: 14 })}
  <circle cx="72" cy="1418" r="14" fill="${TOKENS.green}"/>
  <path d="M66 1418L71 1423L80 1412" fill="none" stroke="#FFFFFF" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>
  ${textMarkup({ x: 98, y: 1425, text: c.routine, size: 22, weight: 700, fill: TOKENS.white, family })}
  ${textMarkup({ x: 98, y: 1453, text: c.routineLead, size: 17, weight: 600, fill: TOKENS.cyan, family })}
  <line x1="70" y1="1470" x2="650" y2="1470" stroke="#343858" stroke-width="2"/>
  <circle cx="72" cy="1500" r="14" fill="${TOKENS.amber}"/>
  <path d="M72 1493V1502" stroke="#FFFFFF" stroke-width="2.5" stroke-linecap="round"/>
  <circle cx="72" cy="1508" r="1.8" fill="#FFFFFF"/>
  ${textMarkup({ x: 98, y: 1507, text: c.judgment, size: 22, weight: 700, fill: TOKENS.white, family })}
  ${textMarkup({ x: 98, y: 1533, text: c.judgmentLead, size: 17, weight: 600, fill: "#FFD89B", family })}

  ${panelMarkup({ x: 40, y: 1570, width: 640, height: 100, fill: TOKENS.surface, stroke: TOKENS.line, radius: 12 })}
  ${textMarkup({ x: 60, y: 1598, text: c.workflow, size: 14, weight: 760, fill: TOKENS.muted, family })}
  ${workflowItems}`;
}

function makeOverview(locale, viewport) {
  const c = COPY[locale];
  const { width, height } = VIEWPORTS.overview[viewport];
  const prefix = `overview-${locale}-${viewport}`;
  const body = viewport === "mobile" ? overviewMobile(c, locale, prefix) : overviewDesktop(c, locale, prefix);
  const suffix = locale === "en" ? "" : `-${locale}`;
  const name = `wenlan-system${suffix}${viewport === "mobile" ? "-mobile" : ""}`;
  const svg = `<svg width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" fill="none" xmlns="http://www.w3.org/2000/svg" role="img" aria-labelledby="title desc">
  <title id="title">${esc(c.overviewTitle)}</title>
  <desc id="desc">${esc(c.overviewSubtitle)}</desc>
  <rect width="${width}" height="${height}" fill="${TOKENS.canvas}"/>
  ${body}
  <defs>
    ${logoDefs(prefix)}
    ${arrowMarker(`${prefix}-arrow`)}
  </defs>
</svg>
`;
  return {
    group: "overview",
    name,
    width,
    height,
    background: TOKENS.canvas,
    requiredCopy: [c.sources, c.memories, c.evidence, c.page, c.reuse, c.routine, c.judgment, ...c.workflowItems],
    svg,
  };
}

function lifecycleDesktop(c, locale, prefix) {
  const family = fontFor(locale);
  const marker = `${prefix}-arrow`;
  const titleLines = locale === "en"
    ? ["How Wenlan keeps knowledge current", "without erasing its history"]
    : [c.title];
  const legendKinds = ["core", "llm", "human", "optional"];
  let legendX = 1000;
  const legend = legendKinds
    .map((kind) => {
      const badge = statusBadge({ x: legendX, y: 54, label: c.legend[kind], kind, family, compact: false });
      legendX += badge.width + 14;
      return badge.markup;
    })
    .join("\n");

  const evidenceCards = [
    [300, c.capture, "core"],
    [575, c.classify, "llm"],
    [850, c.enrich, "llm"],
    [1125, c.entity, "llm"],
    [1400, c.stability, "core"],
  ]
    .map(([x, copy, kind]) =>
      processCard({
        x,
        y: 215,
        width: 250,
        height: 105,
        title: copy[0],
        sub: copy[1],
        kind,
        labels: c.short,
        family,
        titleSize: locale === "en" ? 16 : 18,
        subSize: locale === "en" ? 13 : 14,
      }),
    )
    .join("\n");
  const evidenceArrows = [550, 825, 1100, 1375]
    .map((x) => arrowPath({ d: `M ${x} 267 H ${x + 20}`, marker, color: "#8B90A4", width: 2.5 }))
    .join("\n");

  const pageKinds = ["llm", "core", "core", "llm", "core"];
  const pageCards = c.pageSteps
    .map((copy, index) =>
      processCard({
        x: 300 + index * 272,
        y: 615,
        width: 252,
        height: 105,
        title: copy[0],
        sub: copy[1],
        kind: pageKinds[index],
        labels: c.short,
        family,
        titleSize: locale === "en" ? 15 : 17,
        subSize: locale === "en" ? 13 : 14,
      }),
    )
    .join("\n");
  const pageArrows = [552, 824, 1096, 1368]
    .map((x) => arrowPath({ d: `M ${x} 667 H ${x + 18}`, marker, color: "#8B90A4", width: 2.5 }))
    .join("\n");

  const burstKinds = ["llm", "llm"];
  const idleKinds = ["core", "core", "llm", "optional", "llm", "llm", "llm"];
  const dailyKinds = ["core", "core", "core", "core", "llm", "llm", "core", "optional", "llm"];
  const backstopKinds = ["core", "core"];

  return `${logoMarkup({ x: 70, y: 42, size: 72, prefix })}
  ${multilineMarkup({ x: 170, y: locale === "en" ? 62 : 76, lines: titleLines, size: locale === "en" ? 32 : 38, lineHeight: 36, weight: 740, fill: TOKENS.ink, family })}
  ${textMarkup({ x: 170, y: locale === "en" ? 130 : 116, text: c.subtitle, size: 20, weight: 520, fill: TOKENS.muted, family })}
  ${legend}

  ${panelMarkup({ x: 60, y: 160, width: 1680, height: 350, fill: "#F4F1FF", stroke: TOKENS.violetLine, radius: 16 })}
  ${textMarkup({ x: 90, y: 205, text: c.evidenceLane, size: 22, weight: 780, fill: TOKENS.violetInk, family })}
  ${multilineMarkup({ x: 90, y: 240, lines: wrapShort(c.evidenceLead, locale, 24, 10), size: 17, lineHeight: 24, weight: 560, fill: TOKENS.muted, family })}
  ${evidenceArrows}
  ${evidenceCards}
  ${textMarkup({ x: 1400, y: 343, text: c.stabilityNote, size: 14, weight: 600, fill: TOKENS.greenInk, family })}

  ${arrowPath({ d: "M 1525 320 C 1525 340 515 334 515 350", marker, color: TOKENS.green, width: 2.5 })}
  ${arrowPath({ d: "M 1525 320 C 1525 340 960 334 960 350", marker, color: TOKENS.red, width: 2.5 })}
  ${processCard({ x: 300, y: 360, width: 420, height: 125, title: c.correct[0], sub: c.correct[1], kind: "core", labels: c.short, family, titleSize: 18, subSize: 14 })}
  ${processCard({ x: 745, y: 360, width: 420, height: 125, title: c.conflict[0], sub: c.conflict[1], kind: "human", labels: c.short, family, titleSize: 18, subSize: 14 })}
  ${panelMarkup({ x: 1190, y: 360, width: 490, height: 125, fill: "#F8F8FA", stroke: "#8B90A4", radius: 10, strokeWidth: 1.8 }).replace("/>", ' stroke-dasharray="9 7"/>')}
  ${textMarkup({ x: 1210, y: 392, text: c.dual[0], size: 18, weight: 740, fill: TOKENS.muted, family })}
  ${textMarkup({ x: 1210, y: 421, text: c.dual[1], size: 14, weight: 560, fill: TOKENS.text, family })}
  ${textMarkup({ x: 1210, y: 448, text: c.dualNote, size: 13, weight: 600, fill: TOKENS.muted, family })}
  ${statusBadge({ x: 1560, y: 372, label: c.short.optional, kind: "optional", family, compact: true }).markup}

  ${arrowPath({ d: "M 900 510 V 566", marker, color: TOKENS.green, width: 3 })}
  ${pillMarkup({ x: 610, y: 521, width: 580, height: 42, label: c.affected, fill: TOKENS.greenSoft, stroke: TOKENS.greenLine, color: TOKENS.greenInk, family, size: 16 })}

  ${panelMarkup({ x: 60, y: 570, width: 1680, height: 310, fill: "#EFF8F4", stroke: TOKENS.greenLine, radius: 16 })}
  ${textMarkup({ x: 90, y: 615, text: c.pagesLane, size: 22, weight: 780, fill: TOKENS.greenInk, family })}
  ${multilineMarkup({ x: 90, y: 650, lines: wrapShort(c.pagesLead, locale, 25, 11), size: 17, lineHeight: 24, weight: 560, fill: TOKENS.muted, family })}
  ${pageArrows}
  ${pageCards}
  ${arrowPath({ d: "M 1242 720 C 1242 735 790 730 790 740", marker, color: TOKENS.amber, width: 2.5 })}
  ${processCard({ x: 540, y: 740, width: 520, height: 105, title: c.humanPage[0], sub: c.humanPage[1], kind: "human", labels: c.short, family, titleSize: 18, subSize: 14 })}
  ${arrowPath({ d: "M 1060 792 H 1090", marker, color: TOKENS.amber, width: 2.5 })}
  ${processCard({ x: 1110, y: 740, width: 570, height: 105, title: c.decision[0], sub: c.decision[1], kind: "human", labels: c.short, family, titleSize: 18, subSize: 14 })}
  ${textMarkup({ x: 1110, y: 868, text: c.currentRetrieval, size: 15, weight: 650, fill: TOKENS.greenInk, family })}

  ${panelMarkup({ x: 60, y: 900, width: 1680, height: 320, fill: TOKENS.blueSoft, stroke: TOKENS.blueLine, radius: 16 })}
  ${multilineMarkup({ x: 90, y: 940, lines: wrapShort(c.refineryLane, locale, 16, 8), size: 21, lineHeight: 26, weight: 780, fill: TOKENS.blueInk, family })}
  ${multilineMarkup({ x: 90, y: locale === "en" ? 1003 : 978, lines: wrapShort(c.refineryLead, locale, 22, 10), size: 16, lineHeight: 23, weight: 560, fill: TOKENS.muted, family })}

  ${panelMarkup({ x: 300, y: 935, width: 250, height: 250, fill: TOKENS.surface, stroke: TOKENS.violetLine, radius: 12 })}
  ${textMarkup({ x: 325, y: 974, text: c.burstTitle, size: 18, weight: 760, fill: TOKENS.violetInk, family })}
  ${phaseRowsMarkup({ x: 327, y: 1010, items: c.burstItems, kinds: burstKinds, family, size: locale === "en" ? 16 : 17, lineHeight: 28 })}

  ${panelMarkup({ x: 570, y: 935, width: 350, height: 250, fill: TOKENS.surface, stroke: TOKENS.greenLine, radius: 12 })}
  ${textMarkup({ x: 595, y: 974, text: c.idleTitle, size: 18, weight: 760, fill: TOKENS.greenInk, family })}
  ${phaseRowsMarkup({ x: 597, y: 1007, items: c.idleItems, kinds: idleKinds, family, size: locale === "en" ? 15 : 16, lineHeight: 24 })}

  ${panelMarkup({ x: 940, y: 935, width: 430, height: 250, fill: TOKENS.surface, stroke: TOKENS.blueLine, radius: 12 })}
  ${textMarkup({ x: 965, y: 974, text: c.dailyTitle, size: 18, weight: 760, fill: TOKENS.blueInk, family })}
  ${phaseRowsMarkup({ x: 967, y: 1002, items: c.dailyItems, kinds: dailyKinds, family, size: locale === "en" ? 14 : 16, lineHeight: 21 })}

  ${panelMarkup({ x: 1390, y: 935, width: 300, height: 250, fill: TOKENS.surface, stroke: TOKENS.amberLine, radius: 12 })}
  ${textMarkup({ x: 1415, y: 974, text: c.backstopTitle, size: 18, weight: 760, fill: TOKENS.amberInk, family })}
  ${phaseRowsMarkup({ x: 1417, y: 1010, items: c.backstopItems, kinds: backstopKinds, family, size: 16, lineHeight: 28 })}
  ${textMarkup({ x: 1415, y: 1115, text: c.backstopNote, size: 15, weight: 600, fill: TOKENS.muted, family })}
  ${textMarkup({ x: 300, y: 1212, text: c.refineryFootnote, size: locale === "en" ? 14 : 15, weight: 600, fill: TOKENS.muted, family })}`;
}

function lifecycleMobile(c, locale, prefix) {
  const family = fontFor(locale);
  const marker = `${prefix}-arrow`;
  const titleLines = locale === "en"
    ? ["How Wenlan keeps knowledge current", "without erasing its history"]
    : locale === "zh-Hans"
      ? ["Wenlan 如何让知识持续更新，", "又不抹掉历史"]
      : ["Wenlan 如何讓知識持續更新，", "又不抹掉歷史"];
  const subtitleLines = locale === "en"
    ? ["Evidence changes, pages respond, and only", "ambiguous decisions reach you."]
    : locale === "zh-Hans"
      ? ["依据改变，页面随之更新；只有无法安全判断的事", "才会交给你。"]
      : ["依據改變，頁面隨之更新；只有無法安全判斷的事", "才會交給你。"];
  const legendPositions = {
    core: [40, 174],
    llm: [360, 174],
    human: [40, 218],
    optional: [360, 218],
  };
  const legend = ["core", "llm", "human", "optional"]
    .map((kind) => {
      const [x, y] = legendPositions[kind];
      return statusBadge({ x, y, label: c.legend[kind], kind, family, compact: false }).markup;
    })
    .join("\n");
  const mainCards = [
    [40, 380, c.capture, "core"],
    [380, 380, c.classify, "llm"],
    [40, 500, c.enrich, "llm"],
    [380, 500, c.entity, "llm"],
  ]
    .map(([x, y, copy, kind]) =>
      processCard({
        x,
        y,
        width: 300,
        height: 105,
        title: copy[0],
        sub: copy[1],
        kind,
        labels: c.short,
        family,
        titleSize: locale === "en" ? 15 : 17,
        subSize: locale === "en" ? 12 : 13,
      }),
    )
    .join("\n");
  const pageKinds = ["llm", "core", "core", "llm", "core"];
  const pagePositions = [
    [40, 1090],
    [380, 1090],
    [40, 1210],
    [380, 1210],
    [40, 1330],
  ];
  const pageCards = c.pageSteps
    .map((copy, index) => {
      const [x, y] = pagePositions[index];
      return processCard({
        x,
        y,
        width: 300,
        height: 105,
        title: copy[0],
        sub: copy[1],
        kind: pageKinds[index],
        labels: c.short,
        family,
        titleSize: locale === "en" ? 14 : 16,
        subSize: locale === "en" ? 11.5 : 13,
      });
    })
    .join("\n");
  const burstKinds = ["llm", "llm"];
  const idleKinds = ["core", "core", "llm", "optional", "llm", "llm", "llm"];
  const dailyKinds = ["core", "core", "core", "core", "llm", "llm", "core", "optional", "llm"];
  const backstopKinds = ["core", "core"];

  return `${logoMarkup({ x: 40, y: 38, size: 76, prefix })}
  ${multilineMarkup({ x: 138, y: 68, lines: titleLines, size: 28, lineHeight: 34, weight: 740, fill: TOKENS.ink, family })}
  ${multilineMarkup({ x: 138, y: 136, lines: subtitleLines, size: locale === "en" ? 16 : 17, lineHeight: 23, weight: 520, fill: TOKENS.muted, family })}
  ${legend}

  ${panelMarkup({ x: 24, y: 270, width: 672, height: 690, fill: "#F4F1FF", stroke: TOKENS.violetLine, radius: 16 })}
  ${textMarkup({ x: 40, y: 313, text: c.evidenceLane, size: 22, weight: 780, fill: TOKENS.violetInk, family })}
  ${textMarkup({ x: 40, y: 346, text: c.evidenceLead, size: locale === "en" ? 15 : 16, weight: 560, fill: TOKENS.muted, family })}
  ${arrowPath({ d: "M 340 432 H 375", marker, color: "#8B90A4", width: 2.5 })}
  ${arrowPath({ d: "M 530 485 C 530 493 190 493 190 497", marker, color: "#8B90A4", width: 2.5 })}
  ${arrowPath({ d: "M 340 552 H 375", marker, color: "#8B90A4", width: 2.5 })}
  ${mainCards}
  ${arrowPath({ d: "M 360 605 V 620", marker, color: TOKENS.green, width: 2.5 })}
  ${processCard({ x: 40, y: 625, width: 640, height: 95, title: c.stability[0], sub: [c.stability[1], c.stabilityNote], kind: "core", labels: c.short, family, titleSize: 17, subSize: locale === "en" ? 13 : 14 })}
  ${processCard({ x: 40, y: 740, width: 300, height: 105, title: c.correct[0], sub: wrapShort(c.correct[1], locale, 32, 14), kind: "core", labels: c.short, family, titleSize: 16, subSize: locale === "en" ? 12 : 13 })}
  ${processCard({ x: 380, y: 740, width: 300, height: 105, title: c.conflict[0], sub: wrapShort(c.conflict[1], locale, 31, 14), kind: "human", labels: c.short, family, titleSize: 16, subSize: locale === "en" ? 12 : 13 })}
  ${panelMarkup({ x: 40, y: 865, width: 640, height: 72, fill: "#F8F8FA", stroke: "#8B90A4", radius: 10, strokeWidth: 1.8 }).replace("/>", ' stroke-dasharray="9 7"/>')}
  ${textMarkup({ x: 58, y: 895, text: `${c.dual[0]} · ${c.dual[1]}`, size: locale === "en" ? 14 : 16, weight: 700, fill: TOKENS.muted, family })}
  ${textMarkup({ x: 58, y: 920, text: c.dualNote, size: locale === "en" ? 12 : 14, weight: 560, fill: TOKENS.muted, family })}
  ${statusBadge({ x: 575, y: 887, label: c.short.optional, kind: "optional", family, compact: true }).markup}

  ${arrowPath({ d: "M 360 960 V 990", marker, color: TOKENS.green, width: 3 })}
  ${pillMarkup({ x: 105, y: 968, width: 510, height: 42, label: c.affected, fill: TOKENS.greenSoft, stroke: TOKENS.greenLine, color: TOKENS.greenInk, family, size: locale === "en" ? 14 : 15 })}

  ${panelMarkup({ x: 24, y: 1010, width: 672, height: 525, fill: "#EFF8F4", stroke: TOKENS.greenLine, radius: 16 })}
  ${textMarkup({ x: 40, y: 1052, text: c.pagesLane, size: 22, weight: 780, fill: TOKENS.greenInk, family })}
  ${textMarkup({ x: 40, y: 1080, text: c.pagesLead, size: locale === "en" ? 14 : 15, weight: 560, fill: TOKENS.muted, family })}
  ${arrowPath({ d: "M 340 1142 H 375", marker, color: "#8B90A4", width: 2.5 })}
  ${arrowPath({ d: "M 530 1195 C 530 1203 190 1203 190 1207", marker, color: "#8B90A4", width: 2.5 })}
  ${arrowPath({ d: "M 340 1262 H 375", marker, color: "#8B90A4", width: 2.5 })}
  ${arrowPath({ d: "M 530 1315 C 530 1323 190 1323 190 1327", marker, color: "#8B90A4", width: 2.5 })}
  ${pageCards}
  ${arrowPath({ d: "M 340 1382 H 375", marker, color: TOKENS.amber, width: 2.5 })}
  ${processCard({ x: 380, y: 1330, width: 300, height: 105, title: c.humanPage[0], sub: wrapShort(c.humanPage[1], locale, 31, 14), kind: "human", labels: c.short, family, titleSize: 16, subSize: locale === "en" ? 12 : 13 })}
  ${arrowPath({ d: "M 530 1435 V 1445", marker, color: TOKENS.amber, width: 2.5 })}
  ${panelMarkup({ x: 40, y: 1450, width: 640, height: 64, fill: TOKENS.amberSoft, stroke: TOKENS.amberLine, radius: 10 })}
  ${textMarkup({ x: 60, y: 1478, text: c.decision[0], size: 17, weight: 740, fill: TOKENS.amberInk, family })}
  ${textMarkup({ x: 60, y: 1502, text: c.currentRetrieval, size: locale === "en" ? 12.5 : 14, weight: 600, fill: TOKENS.greenInk, family })}
  ${statusBadge({ x: 578, y: 1467, label: c.short.human, kind: "human", family, compact: true }).markup}

  ${panelMarkup({ x: 24, y: 1560, width: 672, height: 790, fill: TOKENS.blueSoft, stroke: TOKENS.blueLine, radius: 16 })}
  ${textMarkup({ x: 40, y: 1602, text: c.refineryLane, size: 22, weight: 780, fill: TOKENS.blueInk, family })}
  ${textMarkup({ x: 40, y: 1630, text: c.refineryLead, size: locale === "en" ? 13 : 14, weight: 560, fill: TOKENS.muted, family })}

  ${panelMarkup({ x: 40, y: 1650, width: 640, height: 105, fill: TOKENS.surface, stroke: TOKENS.violetLine, radius: 12 })}
  ${textMarkup({ x: 60, y: 1683, text: c.burstTitle, size: 18, weight: 760, fill: TOKENS.violetInk, family })}
  ${phaseRowsMarkup({ x: 62, y: 1713, items: c.burstItems, kinds: burstKinds, family, size: locale === "en" ? 15 : 16, lineHeight: 25 })}

  ${panelMarkup({ x: 40, y: 1770, width: 640, height: 185, fill: TOKENS.surface, stroke: TOKENS.greenLine, radius: 12 })}
  ${textMarkup({ x: 60, y: 1803, text: c.idleTitle, size: 18, weight: 760, fill: TOKENS.greenInk, family })}
  ${phaseRowsMarkup({ x: 62, y: 1831, items: c.idleItems, kinds: idleKinds, family, size: locale === "en" ? 14 : 15, lineHeight: 18 })}

  ${panelMarkup({ x: 40, y: 1970, width: 640, height: 215, fill: TOKENS.surface, stroke: TOKENS.blueLine, radius: 12 })}
  ${textMarkup({ x: 60, y: 2003, text: c.dailyTitle, size: 18, weight: 760, fill: TOKENS.blueInk, family })}
  ${phaseRowsMarkup({ x: 62, y: 2031, items: c.dailyItems, kinds: dailyKinds, family, size: locale === "en" ? 14 : 15, lineHeight: 18 })}

  ${panelMarkup({ x: 40, y: 2200, width: 640, height: 100, fill: TOKENS.surface, stroke: TOKENS.amberLine, radius: 12 })}
  ${textMarkup({ x: 60, y: 2233, text: c.backstopTitle, size: 18, weight: 760, fill: TOKENS.amberInk, family })}
  ${phaseRowsMarkup({ x: 62, y: 2263, items: c.backstopItems, kinds: backstopKinds, family, size: 15, lineHeight: 24 })}
  ${multilineMarkup({ x: 40, y: 2323, lines: wrapShort(c.refineryFootnote, locale, 72, 32), size: locale === "en" ? 11.5 : 13, lineHeight: 19, weight: 600, fill: TOKENS.muted, family })}`;
}

function makeLifecycle(locale, viewport) {
  const c = LIFECYCLE_COPY[locale];
  const { width, height } = VIEWPORTS.lifecycle[viewport];
  const prefix = `lifecycle-${locale}-${viewport}`;
  const body = viewport === "mobile" ? lifecycleMobile(c, locale, prefix) : lifecycleDesktop(c, locale, prefix);
  const suffix = locale === "en" ? "" : `-${locale}`;
  const name = `wenlan-lifecycle${suffix}${viewport === "mobile" ? "-mobile" : ""}`;
  const svg = `<svg width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" fill="none" xmlns="http://www.w3.org/2000/svg" role="img" aria-labelledby="title desc">
  <title id="title">${esc(c.title)}</title>
  <desc id="desc">${esc(c.subtitle)}</desc>
  <rect width="${width}" height="${height}" fill="${TOKENS.canvas}"/>
  ${body}
  <defs>
    ${logoDefs(prefix)}
    ${arrowMarker(`${prefix}-arrow`)}
  </defs>
</svg>
`;
  return {
    group: "lifecycle",
    name,
    width,
    height,
    background: TOKENS.canvas,
    requiredCopy: [
      c.evidenceLane,
      c.capture[0],
      c.classify[0],
      c.enrich[0],
      c.entity[0],
      c.stability[0],
      c.correct[0],
      c.conflict[0],
      c.dual[0],
      c.pagesLane,
      c.pageSteps[1][0],
      c.pageSteps[2][0],
      c.pageSteps[3][0],
      c.humanPage[0],
      c.decision[0],
      ...wrapShort(c.refineryLane, locale, 16, 8),
      ...c.burstItems,
      ...c.idleItems,
      ...c.dailyItems,
      ...c.backstopItems,
    ],
    svg,
  };
}

function selectedAssets(only) {
  const banner = [makeBanner("desktop"), makeBanner("mobile")];
  const overview = ["en", "zh-Hans", "zh-Hant"].flatMap((locale) => [
    makeOverview(locale, "desktop"),
    makeOverview(locale, "mobile"),
  ]);
  const lifecycle = ["en", "zh-Hans", "zh-Hant"].flatMap((locale) => [
    makeLifecycle(locale, "desktop"),
    makeLifecycle(locale, "mobile"),
  ]);
  if (only === "banner") return banner;
  if (only === "overview") return overview;
  if (only === "lifecycle") return lifecycle;
  return [...banner, ...overview, ...lifecycle];
}

async function renderSvgToPng(asset, pngPath) {
  const { chromium } = require("playwright");
  const browser = await chromium.launch({ headless: true });
  try {
    const context = await browser.newContext({
      viewport: { width: asset.width, height: asset.height },
      deviceScaleFactor: 1,
    });
    const page = await context.newPage();
    await page.setContent(
      `<style>html,body{margin:0;width:${asset.width}px;height:${asset.height}px;overflow:hidden}</style>${asset.svg}`,
      { waitUntil: "load" },
    );
    await page.evaluate(() => document.fonts.ready);
    await page.locator("svg").screenshot({ path: pngPath, omitBackground: false });
    await context.close();
  } finally {
    await browser.close();
  }
}

async function writeAsset(asset) {
  fs.mkdirSync(ASSET_DIR, { recursive: true });
  const svgPath = path.join(ASSET_DIR, `${asset.name}.svg`);
  const pngPath = path.join(ASSET_DIR, `${asset.name}.png`);
  fs.writeFileSync(svgPath, asset.svg, "utf8");
  await renderSvgToPng(asset, pngPath);
}

function parseHex(hex) {
  const value = hex.replace("#", "");
  return [0, 2, 4].map((index) => Number.parseInt(value.slice(index, index + 2), 16));
}

async function checkPng(asset, pngPath) {
  const sharp = require("sharp");
  const errors = [];
  if (!fs.existsSync(pngPath)) {
    return [`missing ${path.relative(ROOT, pngPath)}`];
  }
  const image = sharp(pngPath);
  const metadata = await image.metadata();
  if (metadata.width !== asset.width || metadata.height !== asset.height) {
    errors.push(
      `${path.relative(ROOT, pngPath)} is ${metadata.width}x${metadata.height}; expected ${asset.width}x${asset.height}`,
    );
    return errors;
  }
  const { data, info } = await image.removeAlpha().raw().toBuffer({ resolveWithObject: true });
  const expected = parseHex(asset.background);
  const points = [
    [0, 0],
    [info.width - 1, 0],
    [0, info.height - 1],
    [info.width - 1, info.height - 1],
  ];
  for (const [x, y] of points) {
    const offset = (y * info.width + x) * info.channels;
    const actual = Array.from(data.subarray(offset, offset + 3));
    if (actual.some((channel, index) => channel !== expected[index])) {
      errors.push(`${path.relative(ROOT, pngPath)} corner ${x},${y} is ${actual.join(",")}; expected ${expected.join(",")}`);
    }
  }
  return errors;
}

async function checkAsset(asset) {
  const errors = [];
  const svgPath = path.join(ASSET_DIR, `${asset.name}.svg`);
  const pngPath = path.join(ASSET_DIR, `${asset.name}.png`);
  if (!fs.existsSync(svgPath)) {
    errors.push(`missing ${path.relative(ROOT, svgPath)}`);
  } else {
    const current = fs.readFileSync(svgPath, "utf8");
    if (current !== asset.svg) {
      errors.push(`${path.relative(ROOT, svgPath)} is not current`);
    }
    for (const required of asset.requiredCopy) {
      if (!current.includes(required)) {
        errors.push(`${path.relative(ROOT, svgPath)} is missing ${JSON.stringify(required)}`);
      }
    }
  }
  errors.push(...(await checkPng(asset, pngPath)));
  return errors;
}

function parseArgs(argv) {
  const mode = argv.includes("--write") ? "write" : argv.includes("--check") ? "check" : null;
  const onlyIndex = argv.indexOf("--only");
  const only = onlyIndex >= 0 ? argv[onlyIndex + 1] : "all";
  if (!mode) {
    throw new Error("Use --write or --check");
  }
  if (!["all", "banner", "overview", "lifecycle"].includes(only)) {
    throw new Error(`Unknown --only value: ${only}`);
  }
  return { mode, only };
}

async function main() {
  const { mode, only } = parseArgs(process.argv.slice(2));
  const assets = selectedAssets(only);
  if (mode === "write") {
    for (const asset of assets) {
      await writeAsset(asset);
    }
    console.log(`${assets.length * 2} assets generated`);
    return;
  }

  const errors = [];
  for (const asset of assets) {
    errors.push(...(await checkAsset(asset)));
  }
  if (errors.length > 0) {
    for (const error of errors) {
      console.error(`- ${error}`);
    }
    process.exitCode = 1;
    return;
  }
  console.log(`${only} assets are current`);
}

main().catch((error) => {
  console.error(error.stack || error.message);
  process.exitCode = 1;
});
