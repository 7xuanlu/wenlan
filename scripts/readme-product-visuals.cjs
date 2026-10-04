const VIEWPORTS = {
  overview: {
    desktop: { width: 1600, height: 980 },
    mobile: { width: 720, height: 1780 },
  },
  lifecycle: {
    desktop: { width: 1800, height: 1120 },
    mobile: { width: 720, height: 2160 },
  },
  network: {
    desktop: { width: 1800, height: 1120 },
    mobile: { width: 720, height: 1680 },
  },
};

const C = {
  paper: "#FCFCFB",
  surface: "#FFFFFF",
  raised: "#F7F8FA",
  ink: "#1A1A2E",
  secondary: "#586174",
  tertiary: "#8B93A3",
  border: "#E3E7EE",
  indigo: "#5E58C8",
  indigoSoft: "#F3F2FC",
  sage: "#6F8F76",
  sageDark: "#4F7558",
  sageSoft: "#F2F7F3",
  amber: "#B5842E",
  amberDark: "#8C5F16",
  amberSoft: "#FBF6EA",
  warm: "#B46A3A",
  warmDark: "#9F5C32",
  warmSoft: "#FAF2ED",
};

// Use the app's actual typography tokens, including its system CJK fallbacks.
// Keeping a second set of stacks here caused the diagrams to drift from the app.
const appCss = require("node:fs").readFileSync(require("node:path").join(__dirname, "../src/index.css"), "utf8");
const FONTS = Object.fromEntries(["heading", "body", "mono"].map((kind) => {
  const value = appCss.match(new RegExp(`--mem-font-${kind}:\\s*([^;]+);`))?.[1];
  if (!value) throw new Error(`Missing app typography token: ${kind}`);
  return [kind, value];
}));

// This is a reusable working note, not a definition or a product description.
// [1] Editorial takeaway from Anthropic's environmental-feedback guidance:
// https://www.anthropic.com/engineering/building-effective-agents (Agents).
// [2] Project-owned diagram-review lesson: passing generator tests did not
// catch text crossed by graph edges. The rendered fixes are in 061fbc6a.
// The composition is illustrative, not a customer testimonial or evidence
// that Wenlan autonomously generated, stored, or reused this example page.
const OVERVIEW_COPY = {
  en: {
    description: "An illustrative Agent Loop note combines retry guidance with a lesson from a UI fix: inspect desktop and mobile views even when tests pass.",
    eyebrow: "WENLAN KNOWLEDGE SYSTEM",
    title: [
      "Your sources and working knowledge become",
      "a knowledge base that stays current.",
    ],
    mobileTitle: [
      "Your sources and working knowledge",
      "become a knowledge base",
      "that stays current.",
    ],
    sources: "Sources",
    sourcesLead: "What you already have",
    mobileSourcesLead: ["What you already have"],
    sourcesDescription: "Documents / notes / AI conversations",
    mobileSourcesDescription: ["Documents / notes", "AI conversations"],
    sourceTags: ["PDF", "MARKDOWN", "AI CHAT"],
    memories: "Memories",
    memoriesLead: "What ongoing work teaches you",
    mobileMemoriesLead: ["What ongoing work", "teaches you"],
    memoriesDescription: "Decisions / corrections / context",
    mobileMemoriesDescription: ["Decisions / corrections", "Context"],
    memoryTags: ["DECISION", "LESSON", "CONTEXT"],
    pageLabel: "MAINTAINED PAGE",
    current: "CURRENT",
    pageTitle: "Agent Loop",
    revised: "Illustrative example",
    records: "2 references",
    synthesis: "WHEN THE LOOP STALLS",
    exampleLines: ["A repeated error is a reason to investigate,", "not to run the same step again. [1]"],
    workflow: "ADDED AFTER A UI FIX",
    workflowLines: ["Tests passed; a line still covered a label.", "Before calling a UI fix done, inspect", "the desktop and mobile views. [2]"],
    sourceRef: "Agent design guide [1]",
    memoryRef: "Diagram review note [2]",
    linked: "LINKED KNOWLEDGE",
    linkedTags: ["Completion checks", "Failed attempts", "Handoffs"],
    pageTraits: ["Plain Markdown", "Inspectable citations", "Change log"],
    mobilePageTraits: ["Markdown", "Citations", "Change log"],
    backLabel: "NEXT TASK · Claude / Codex",
    backWords: ["Read it.", "Ask it.", "Reuse it."],
    backSteps: [
      "Claude / Codex reads this page",
      "Reuses its completion checks",
      "Checks more than green tests",
    ],
    changed: "When a Page is stale",
    changedLead: "It rebuilds from current support.",
    upkeep: "Use your AI, or enable upkeep",
    upkeepLead: "Background upkeep needs a configured model.",
    authority: "You keep authority",
    authorityLead: "Automatic refresh proposes changes to pages you edited.",
  },
  "zh-Hans": {
    description: "Agent Loop 笔记示例：结合重试原则与一次界面修正的教训，将桌面和手机画面检查补进完成条件。",
    eyebrow: "WENLAN 知识系统",
    title: ["你的资料与工作经验，成为", "持续更新的知识库。"],
    mobileTitle: ["你的资料与工作经验，", "成为持续更新的知识库。"],
    sources: "来源",
    sourcesLead: "你已经拥有的材料",
    mobileSourcesLead: ["你已经拥有的材料"],
    sourcesDescription: "文档 / 笔记 / AI 对话",
    mobileSourcesDescription: ["文档 / 笔记 / AI 对话"],
    sourceTags: ["PDF", "MARKDOWN", "AI 对话"],
    memories: "记忆",
    memoriesLead: "工作中值得留下的知识",
    mobileMemoriesLead: ["工作中值得留下的知识"],
    memoriesDescription: "决策 / 修正 / 脉络",
    mobileMemoriesDescription: ["决策 / 修正 / 脉络"],
    memoryTags: ["决策", "经验", "脉络"],
    pageLabel: "持续维护的页面",
    current: "当前",
    pageTitle: "Agent Loop",
    revised: "示例内容",
    records: "2 个引用示例",
    synthesis: "卡住时怎么做",
    exampleLines: ["同一错误再次出现，先查原因；", "没有新证据，不原样重试。[1]"],
    workflow: "一次实践后补上的规则",
    workflowLines: ["测试全过，手机标签仍被连线遮住。", "新增验收：桌面与手机画面都要看。[2]"],
    sourceRef: "Agent 设计指南 [1]",
    memoryRef: "版面检查笔记 [2]",
    linked: "相关知识",
    linkedTags: ["验收清单", "失败与重试", "工作交接"],
    pageTraits: ["纯 Markdown", "引用可检查", "变更记录"],
    mobilePageTraits: ["Markdown", "引用", "变更记录"],
    backLabel: "下次用 Claude / Codex 工作",
    backWords: ["阅读。", "提问。", "继续使用。"],
    backSteps: ["Claude / Codex 先读这页", "沿用已有的验收条件", "不再只看测试是否通过"],
    changed: "页面过时后",
    changedLead: "依当前依据重新构建。",
    upkeep: "用现有 AI，或开启后台维护",
    upkeepLead: "后台整理与更新需要配置模型。",
    authority: "你保留决定权",
    authorityLead: "自动更新你编辑过的页面时，先提出修订。",
  },
  "zh-Hant": {
    description: "Agent Loop 筆記示例：結合重試原則與一次介面修正的教訓，將桌面和手機畫面檢查補進完成條件。",
    eyebrow: "WENLAN 知識系統",
    title: ["你的資料與工作經驗，成為", "持續更新的知識庫。"],
    mobileTitle: ["你的資料與工作經驗，", "成為持續更新的知識庫。"],
    sources: "來源",
    sourcesLead: "你已經擁有的材料",
    mobileSourcesLead: ["你已經擁有的材料"],
    sourcesDescription: "文件 / 筆記 / AI 對話",
    mobileSourcesDescription: ["文件 / 筆記 / AI 對話"],
    sourceTags: ["PDF", "MARKDOWN", "AI 對話"],
    memories: "記憶",
    memoriesLead: "工作中值得留下的知識",
    mobileMemoriesLead: ["工作中值得留下的知識"],
    memoriesDescription: "決策 / 修正 / 脈絡",
    mobileMemoriesDescription: ["決策 / 修正 / 脈絡"],
    memoryTags: ["決策", "經驗", "脈絡"],
    pageLabel: "持續維護的頁面",
    current: "目前",
    pageTitle: "Agent Loop",
    revised: "示例內容",
    records: "2 個引用示例",
    synthesis: "卡住時怎麼做",
    exampleLines: ["同一錯誤再次出現，先查原因；", "沒有新證據，不原樣重試。[1]"],
    workflow: "一次實作後補上的規則",
    workflowLines: ["測試全過，手機標籤仍被連線遮住。", "新增驗收：桌面與手機畫面都要看。[2]"],
    sourceRef: "Agent 設計指南 [1]",
    memoryRef: "版面檢查筆記 [2]",
    linked: "相關知識",
    linkedTags: ["驗收清單", "失敗與重試", "工作交接"],
    pageTraits: ["純 Markdown", "引用可檢查", "變更紀錄"],
    mobilePageTraits: ["Markdown", "引用", "變更紀錄"],
    backLabel: "下次用 Claude / Codex 工作",
    backWords: ["閱讀。", "提問。", "繼續使用。"],
    backSteps: ["Claude / Codex 先讀這頁", "沿用已有的驗收條件", "不再只看測試是否通過"],
    changed: "頁面過時後",
    changedLead: "依目前依據重新構建。",
    upkeep: "用現有 AI，或開啟背景維護",
    upkeepLead: "背景整理與更新需要設定模型。",
    authority: "你保留決定權",
    authorityLead: "自動更新你編輯過的頁面時，先提出修訂。",
  },
};

const LIFECYCLE_COPY = {
  en: {
    eyebrow: "TWO LINKED LIFECYCLES",
    title: "Knowledge changes. History stays.",
    mobileTitle: ["Knowledge changes.", "History stays."],
    subtitle: "A stale Page rebuilds from current evidence; superseded knowledge remains traceable.",
    mobileSubtitle: [
      "A stale Page rebuilds from current evidence.",
      "Superseded knowledge remains traceable.",
    ],
    memoryLabel: "MEMORY LIFECYCLE",
    memoryTitle: "Superseded, not erased.",
    earlierMemory: "EARLIER MEMORY",
    earlierState: "LEARNED",
    correctedMemory: "REPLACEMENT MEMORY",
    correctedState: "CONFIRMED",
    correct: "REVISE",
    supersedes: "SUPERSEDES",
    oldLinked: "Old claim remains linked",
    enrich: "ENRICH",
    enrichDetail: "facts / confidence",
    connect: "CONNECT",
    connectDetail: "entities / relations",
    sourceChanged: "SOURCE SUPPORT",
    memoryCorrected: "MEMORY SUPPORT",
    refinery: "REFINERY",
    maintain: ["Rebuild", "from current support"],
    ring: {
      understand: "UNDERSTAND",
      connect: "CONNECT",
      reconcile: "RECONCILE",
      verify: "VERIFY",
    },
    contradiction: "OPTIONAL CONFLICT REVIEW",
    wait: "Protected conflicts wait.",
    affectedClaim: "STALE PAGE",
    pageLabel: "PAGE LIFECYCLE",
    pageTitle: "Rebuilt, changes recorded.",
    pageVersion: "PAGE v12",
    current: "CURRENT",
    maintainedPage: "Maintained Page",
    pageMeta: "v12 / 6 supporting records",
    verified: "CITATION CHECK PASSED",
    prior: "Recent changes remain inspectable",
    versions: "v10 / v11 / v12",
    humanPage: "AUTOMATIC REFRESH · EDITED PAGE",
    humanLead: "Proposed changes wait for your review.",
    mobileHumanLead: ["Proposed changes wait for your review."],
    background: "BACKGROUND UPKEEP",
    modelNote: "Model steps need a configured model; conflict reconciliation is off by default.",
    mobileModelNote: ["Model steps need a configured model.", "Conflict reconciliation is off by default."],
    stale: "STALE",
    phases: ["Enrich", "Link", "Reconcile", "Verify"],
    runs: "RECONCILE",
    schedule: "Configured model / off by default / opt-in",
    archive: "Supersession keeps the earlier memory.",
  },
  "zh-Hans": {
    eyebrow: "两套相连的生命周期",
    title: "知识会改变，历史仍会保留。",
    mobileTitle: ["知识会改变，", "历史仍会保留。"],
    subtitle: "过时页面依当前依据重建；被取代的知识仍可追溯。",
    mobileSubtitle: ["过时页面依当前依据重建；", "被取代的知识仍可追溯。"],
    memoryLabel: "记忆生命周期",
    memoryTitle: "被取代，但不会被删除。",
    earlierMemory: "较早的记忆",
    earlierState: "已学习",
    correctedMemory: "取代它的新记忆",
    correctedState: "已确认",
    correct: "修订",
    supersedes: "取代",
    oldLinked: "旧说法仍保留关联",
    enrich: "丰富",
    enrichDetail: "事实 / 可信度",
    connect: "连接",
    connectDetail: "实体 / 关系",
    sourceChanged: "来源依据",
    memoryCorrected: "记忆依据",
    refinery: "精炼",
    maintain: ["依当前依据", "重新构建"],
    ring: {
      understand: "理解",
      connect: "连接",
      reconcile: "校正",
      verify: "验证",
    },
    contradiction: "可选冲突审核",
    wait: "受保护内容等待判断",
    affectedClaim: "待刷新的页面",
    pageLabel: "页面生命周期",
    pageTitle: "重新构建，变化留有记录。",
    pageVersion: "页面 v12",
    current: "当前",
    maintainedPage: "持续维护的页面",
    pageMeta: "v12 / 6 条支撑记录",
    verified: "引用检查通过",
    prior: "近期变化仍可检查",
    versions: "v10 / v11 / v12",
    humanPage: "自动更新 · 你编辑过的页面",
    humanLead: "修订提案等你审核后再应用。",
    mobileHumanLead: ["修订提案等你审核后再应用。"],
    background: "后台维护",
    modelNote: "模型步骤需配置模型；冲突校正默认关闭。",
    mobileModelNote: ["模型步骤需配置模型；", "冲突校正默认关闭。"],
    stale: "待更新",
    phases: ["丰富", "连接", "校正", "验证"],
    runs: "校正",
    schedule: "已配置模型 / 默认关闭 / 明确启用",
    archive: "取代旧记忆时，保留前后关联。",
  },
  "zh-Hant": {
    eyebrow: "兩套相連的生命週期",
    title: "知識會改變，歷史仍會保留。",
    mobileTitle: ["知識會改變，", "歷史仍會保留。"],
    subtitle: "過時頁面依目前依據重建；被取代的知識仍可追溯。",
    mobileSubtitle: ["過時頁面依目前依據重建；", "被取代的知識仍可追溯。"],
    memoryLabel: "記憶生命週期",
    memoryTitle: "被取代，但不會被刪除。",
    earlierMemory: "較早的記憶",
    earlierState: "已學習",
    correctedMemory: "取代它的新記憶",
    correctedState: "已確認",
    correct: "修訂",
    supersedes: "取代",
    oldLinked: "舊說法仍保留關聯",
    enrich: "豐富",
    enrichDetail: "事實 / 可信度",
    connect: "連接",
    connectDetail: "實體 / 關係",
    sourceChanged: "來源依據",
    memoryCorrected: "記憶依據",
    refinery: "精煉",
    maintain: ["依目前依據", "重新構建"],
    ring: {
      understand: "理解",
      connect: "連接",
      reconcile: "校正",
      verify: "驗證",
    },
    contradiction: "可選衝突審核",
    wait: "受保護內容等待判斷",
    affectedClaim: "待刷新的頁面",
    pageLabel: "頁面生命週期",
    pageTitle: "重新構建，變化留有紀錄。",
    pageVersion: "頁面 v12",
    current: "目前",
    maintainedPage: "持續維護的頁面",
    pageMeta: "v12 / 6 條支撐紀錄",
    verified: "引用檢查通過",
    prior: "近期變化仍可檢查",
    versions: "v10 / v11 / v12",
    humanPage: "自動更新 · 你編輯過的頁面",
    humanLead: "修訂提案等你審核後再套用。",
    mobileHumanLead: ["修訂提案等你審核後再套用。"],
    background: "背景維護",
    modelNote: "模型步驟需設定模型；衝突校正預設關閉。",
    mobileModelNote: ["模型步驟需設定模型；", "衝突校正預設關閉。"],
    stale: "待更新",
    phases: ["豐富", "連接", "校正", "驗證"],
    runs: "校正",
    schedule: "已設定模型 / 預設關閉 / 明確啟用",
    archive: "取代舊記憶時，保留前後關聯。",
  },
};

const NETWORK_COPY = {
  en: {
    description: "Pages cite sources and memories. Memories link to entities, which connect through typed relations.",
    eyebrow: "KNOWLEDGE GRAPH",
    title: "Connected like a graph. Readable like a wiki.",
    mobileTitle: ["Connected like a graph.", "Readable like a wiki."],
    subtitle: "Pages cite sources and memories. Memories link to entities and their connections.",
    mobileSubtitle: [
      "Pages cite sources and memories.",
      "Memories link to entities",
      "and their connections.",
    ],
    knowledgePage: "KNOWLEDGE PAGE",
    entity: "ENTITY",
    sourcePage: "SOURCE PAGE",
    memory: "MEMORY",
    current: "CURRENT",
    heroTitle: "Wenlan positioning",
    heroMeta: "v7 / 12 supporting records",
    sourceOne: "User interviews",
    sourceTwo: "LLM-wiki notes",
    entityCommunity: "ENTITY COMMUNITY",
    communityMethod: "GROUPED BY RELATION DENSITY",
    entityOne: "Wenlan",
    entityTwo: "Obsidian",
    entityThree: "LLM-wiki",
    entityFour: "Knowledge graph",
    project: "PROJECT",
    tool: "TECHNOLOGY",
    method: "CONCEPT",
    concept: "CONCEPT",
    linkedPage: "Launch strategy",
    edgeLabels: ["CITES", "SUPPORTS", "ABOUT", "REFINES", "RELATED TO · 0.82", "PART OF"],
    footer: "Conceptual example · Pages, evidence, and connections",
  },
  "zh-Hans": {
    description: "页面引用来源与记忆；记忆连接实体，实体之间保留明确关系。",
    eyebrow: "知识图谱",
    title: "像图谱一样相连，像 Wiki 一样可读。",
    mobileTitle: ["像图谱一样相连，", "像 Wiki 一样可读。"],
    subtitle: "页面引用来源与记忆；记忆连接实体，实体之间保留明确关系。",
    mobileSubtitle: [
      "页面引用来源与记忆；",
      "记忆连接实体，实体之间保留明确关系。",
    ],
    knowledgePage: "知识页面",
    entity: "实体",
    sourcePage: "来源页面",
    memory: "记忆",
    current: "当前",
    heroTitle: "Wenlan 定位",
    heroMeta: "v7 / 12 条支撑记录",
    sourceOne: "用户访谈",
    sourceTwo: "LLM-wiki 笔记",
    entityCommunity: "实体群组",
    communityMethod: "按关系密度分组",
    entityOne: "Wenlan",
    entityTwo: "Obsidian",
    entityThree: "LLM-wiki",
    entityFour: "知识图谱",
    project: "项目",
    tool: "技术",
    method: "概念",
    concept: "概念",
    linkedPage: "发布策略",
    edgeLabels: ["引用", "支撑", "关于", "延伸", "相关 · 0.82", "属于"],
    footer: "概念示意 · 页面、依据与关联",
  },
  "zh-Hant": {
    description: "頁面引用來源與記憶；記憶連接實體，實體之間保留明確關係。",
    eyebrow: "知識圖譜",
    title: "像圖譜一樣相連，像 Wiki 一樣可讀。",
    mobileTitle: ["像圖譜一樣相連，", "像 Wiki 一樣可讀。"],
    subtitle: "頁面引用來源與記憶；記憶連接實體，實體之間保留明確關係。",
    mobileSubtitle: [
      "頁面引用來源與記憶；",
      "記憶連接實體，實體之間保留明確關係。",
    ],
    knowledgePage: "知識頁面",
    entity: "實體",
    sourcePage: "來源頁面",
    memory: "記憶",
    current: "目前",
    heroTitle: "Wenlan 定位",
    heroMeta: "v7 / 12 條支撐紀錄",
    sourceOne: "使用者訪談",
    sourceTwo: "LLM-wiki 筆記",
    entityCommunity: "實體群組",
    communityMethod: "依關係密度分組",
    entityOne: "Wenlan",
    entityTwo: "Obsidian",
    entityThree: "LLM-wiki",
    entityFour: "知識圖譜",
    project: "專案",
    tool: "技術",
    method: "概念",
    concept: "概念",
    linkedPage: "發布策略",
    edgeLabels: ["引用", "支撐", "關於", "延伸", "相關 · 0.82", "屬於"],
    footer: "概念示意 · 頁面、依據與關聯",
  },
};

function esc(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function family(locale, kind) {
  return FONTS[kind];
}

function text({
  locale,
  x,
  y,
  value,
  size,
  kind = "body",
  weight = 400,
  fill = C.ink,
  anchor = "start",
}) {
  return `<text x="${x}" y="${y}" font-family="${esc(family(locale, kind))}" font-size="${size}" font-weight="${weight}" fill="${fill}" text-anchor="${anchor}">${esc(value)}</text>`;
}

function lines({
  locale,
  x,
  y,
  values,
  size,
  lineHeight,
  kind = "body",
  weight = 400,
  fill = C.ink,
  anchor = "start",
}) {
  return values
    .map((value, index) => text({
      locale,
      x,
      y: y + index * lineHeight,
      value,
      size,
      kind,
      weight,
      fill,
      anchor,
    }))
    .join("\n");
}

function region({ id, x, y, width, height, content, checkOverlap = true }) {
  return `<g data-fit-region="${esc(id)}" data-fit-x="${x}" data-fit-y="${y}" data-fit-width="${width}" data-fit-height="${height}" data-check-overlap="${checkOverlap}">
    ${content}
  </g>`;
}

function approximateWidth(label, locale, mono, size) {
  const chars = Array.from(label);
  const width = chars.reduce((total, char) => {
    if (/[^\u0000-\u00ff]/u.test(char)) return total + size;
    if (char === " ") return total + size * 0.38;
    return total + size * (mono ? 0.61 : 0.53);
  }, 0);
  return Math.ceil(width);
}

function chip({
  locale,
  x,
  y,
  label,
  fill = C.raised,
  stroke = C.border,
  color = C.secondary,
  width,
  height = 32,
  mono = false,
  size = 14,
}) {
  const computedWidth = width ?? Math.max(66, approximateWidth(label, locale, mono, size) + 28);
  return {
    width: computedWidth,
    markup: `<g>
      <rect x="${x}" y="${y}" width="${computedWidth}" height="${height}" rx="${height / 2}" fill="${fill}" stroke="${stroke}"/>
      ${text({
        locale,
        x: x + computedWidth / 2,
        y: y + height / 2 + size * 0.38,
        value: label,
        size,
        kind: mono ? "mono" : "body",
        weight: mono ? 500 : 600,
        fill: color,
        anchor: "middle",
      })}
    </g>`,
  };
}

function chipRow({
  locale,
  x,
  y,
  labels,
  gap = 10,
  height = 32,
  mono = false,
  size = 14,
  fill,
  stroke,
  color,
}) {
  let cursor = x;
  const markup = labels.map((label) => {
    const item = chip({
      locale,
      x: cursor,
      y,
      label,
      height,
      mono,
      size,
      fill,
      stroke,
      color,
    });
    cursor += item.width + gap;
    return item.markup;
  }).join("\n");
  return { markup, width: cursor - x - gap };
}

function dotSeparated({
  locale,
  x,
  y,
  labels,
  size,
  kind = "body",
  weight = 600,
  fill = C.secondary,
  gap = 14,
}) {
  let cursor = x;
  const chunks = [];
  labels.forEach((label, index) => {
    chunks.push(text({ locale, x: cursor, y, value: label, size, kind, weight, fill }));
    cursor += approximateWidth(label, locale, kind === "mono", size);
    if (index < labels.length - 1) {
      cursor += gap;
      chunks.push(`<circle cx="${cursor}" cy="${y - size * 0.3}" r="2.5" fill="${C.border}"/>`);
      cursor += gap;
    }
  });
  return chunks.join("\n");
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

function logo({ x, y, size, prefix }) {
  const scale = size / 512;
  return `<g transform="translate(${x} ${y}) scale(${scale})">
    <rect width="512" height="512" rx="112" fill="#1A1A2E"/>
    <circle cx="256" cy="256" r="160" fill="none" stroke="url(#${prefix}-ring)" stroke-width="76"/>
    <circle cx="322" cy="160" r="42" fill="url(#${prefix}-orb)"/>
  </g>`;
}

function arrowMarker(id, color = C.indigo) {
  return `<marker id="${id}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <path d="M0 0L10 5L0 10Z" fill="${color}"/>
  </marker>`;
}

function documentGlyph({ x, y, color = C.secondary }) {
  return `<g fill="none" stroke="${color}" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
    <path d="M${x + 3} ${y + 1}h11l6 6v19H${x + 3}z"/>
    <path d="M${x + 14} ${y + 1}v7h6"/>
    <path d="M${x + 7} ${y + 14}h9M${x + 7} ${y + 19}h9"/>
  </g>`;
}

function memoryGlyph({ x, y, color = C.indigo }) {
  // Match the memory navigation icon in PrimaryNavigation.tsx. A regression test
  // compares these paths with the app so the illustration cannot drift silently.
  const paths = [
    "M15.5 13a3.5 3.5 0 0 0 -3.5 3.5v1a3.5 3.5 0 0 0 7 0v-1.8",
    "M8.5 13a3.5 3.5 0 0 1 3.5 3.5v1a3.5 3.5 0 0 1 -7 0v-1.8",
    "M17.5 16a3.5 3.5 0 0 0 0 -7h-.5",
    "M19 9.3v-2.8a3.5 3.5 0 0 0 -7 0",
    "M6.5 16a3.5 3.5 0 0 1 0 -7h.5",
    "M5 9.3v-2.8a3.5 3.5 0 0 1 7 0v10",
  ];
  return `<g data-icon="app-memory-brain" transform="translate(${x} ${y}) scale(${28 / 24})" fill="none" stroke="${color}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
    ${paths.map((geometry) => `<path d="${geometry}"/>`).join("\n")}
  </g>`;
}

function overviewInput({
  locale,
  id,
  x,
  y,
  title,
  lead,
  description,
  tags,
  glyph,
}) {
  const tagRow = chipRow({ locale, x: x + 24, y: y + 121, labels: tags, mono: true, size: 13 });
  return region({
    id,
    x,
    y: y - 16,
    width: 350,
    height: 186,
    content: `
      <line x1="${x}" y1="${y + 2}" x2="${x}" y2="${y + 150}" stroke="${C.indigo}" stroke-width="3"/>
      ${glyph({ x: x + 24, y: y + 8 })}
      ${text({ locale, x: x + 64, y: y + 30, value: title, size: 31, kind: "heading", weight: 600 })}
      ${text({ locale, x: x + 24, y: y + 72, value: lead, size: locale === "en" ? 18 : 19, weight: 600, fill: C.secondary })}
      ${text({ locale, x: x + 24, y: y + 102, value: description, size: locale === "en" ? 17 : 18, fill: C.secondary })}
      ${tagRow.markup}
    `,
  });
}

function overviewPage({
  locale,
  c,
  x,
  y,
  width,
  height,
  prefix,
  mobile = false,
}) {
  const pad = mobile ? 34 : 48;
  const titleSize = mobile ? (locale === "en" ? 34 : 36) : (locale === "en" ? 38 : 40);
  const labelSize = mobile ? 20 : 13;
  const chipHeight = mobile ? 38 : 30;
  const currentChip = chip({
    locale,
    x: x + width - (mobile ? 140 : 142),
    y: y + 24,
    label: c.current,
    width: mobile ? 108 : 102,
    height: chipHeight,
    fill: C.sageSoft,
    stroke: "#C7DACB",
    color: C.sageDark,
    mono: true,
    size: mobile ? 18 : 13,
  }).markup;
  const linkedRow = chipRow({
    locale,
    x: x + pad,
    y: y + (mobile ? 606 : 508),
    labels: c.linkedTags,
    gap: mobile ? 8 : 12,
    height: mobile ? 40 : 34,
    size: mobile ? 18 : 14,
  }).markup;
  const sourceReference = chip({
    locale,
    x: x + pad,
    y: y + (mobile ? 310 : 272),
    label: c.sourceRef,
    height: mobile ? 38 : 30,
    size: mobile ? 20 : 16,
  }).markup;
  const memoryReference = chip({
    locale,
    x: x + pad,
    y: y + (mobile ? 500 : 436),
    label: c.memoryRef,
    height: mobile ? 38 : 30,
    fill: C.indigoSoft,
    stroke: "#D2CFF0",
    color: C.indigo,
    size: mobile ? 20 : 16,
  }).markup;
  const traitsY = y + height - (mobile ? 42 : 36);
  const traits = text({
    locale, x: x + pad, y: traitsY,
    value: (mobile ? c.mobilePageTraits : c.pageTraits).join(" · "),
    size: mobile ? (locale === "en" ? 20 : 21) : 15,
    weight: 600, fill: C.secondary,
  });

  return region({
    id: `${prefix}-page`,
    x,
    y,
    width,
    height,
    checkOverlap: false,
    content: `
      <g filter="url(#${prefix}-page-shadow)">
        <rect x="${x}" y="${y}" width="${width}" height="${height}" rx="8" fill="${C.surface}" stroke="${C.border}"/>
      </g>
      ${text({ locale, x: x + pad, y: y + 46, value: c.pageLabel, size: labelSize, kind: "mono", weight: 500, fill: C.tertiary })}
      ${currentChip}
      ${text({ locale, x: x + pad, y: y + (mobile ? 118 : 108), value: c.pageTitle, size: titleSize, kind: "heading", weight: 600 })}
      ${text({ locale, x: x + pad, y: y + (mobile ? 158 : 144), value: `${c.revised} · ${c.records}`, size: mobile ? 20 : 15, fill: C.tertiary })}
      ${text({ locale, x: x + pad, y: y + (mobile ? 220 : 190), value: c.synthesis, size: mobile ? 20 : 15, kind: "mono", weight: 500, fill: C.tertiary })}
      ${lines({ locale, x: x + pad, y: y + (mobile ? 254 : 222), values: c.exampleLines, size: mobile ? 25 : 23, lineHeight: mobile ? 32 : 30, fill: C.ink })}
      ${sourceReference}
      ${text({ locale, x: x + pad, y: y + (mobile ? 390 : 330), value: c.workflow, size: mobile ? 20 : 15, kind: "mono", weight: 500, fill: C.tertiary })}
      ${lines({ locale, x: x + pad, y: y + (mobile ? 422 : 358), values: c.workflowLines, size: mobile ? 25 : 23, lineHeight: 30, fill: C.ink })}
      ${memoryReference}
      <circle cx="${x + width - (mobile ? 20 : 56)}" cy="${y + (mobile ? 506 : 440)}" r="6" fill="${C.amber}"/>
      <line x1="${x + width - (mobile ? 20 : 56)}" y1="${y + (mobile ? 514 : 448)}" x2="${x + width - (mobile ? 20 : 56)}" y2="${y + (mobile ? 544 : 470)}" stroke="${C.amber}" stroke-width="1.5"/>
      ${text({ locale, x: x + pad, y: y + (mobile ? 580 : 490), value: c.linked, size: mobile ? 20 : 15, kind: "mono", weight: 500, fill: C.tertiary })}
      ${linkedRow}
      ${traits}
    `,
  });
}

function overviewDesktop(c, locale, prefix) {
  const marker = `${prefix}-arrow`;
  const sources = overviewInput({
    locale,
    id: `${prefix}-sources`,
    x: 72,
    y: 276,
    title: c.sources,
    lead: c.sourcesLead,
    description: c.sourcesDescription,
    tags: c.sourceTags,
    glyph: documentGlyph,
  });
  const memories = overviewInput({
    locale,
    id: `${prefix}-memories`,
    x: 72,
    y: 534,
    title: c.memories,
    lead: c.memoriesLead,
    description: c.memoriesDescription,
    tags: c.memoryTags,
    glyph: memoryGlyph,
  });
  const page = overviewPage({
    locale,
    c,
    x: 470,
    y: 228,
    width: 650,
    height: 600,
    prefix,
  });
  const output = region({
    id: `${prefix}-back-to-work`,
    x: 1208,
    y: 308,
    width: 326,
    height: 390,
    content: `
      ${text({ locale, x: 1208, y: 328, value: c.backLabel, size: 13, kind: "mono", weight: 500, fill: C.tertiary })}
      ${lines({ locale, x: 1208, y: 382, values: c.backWords, size: locale === "en" ? 34 : 36, lineHeight: 50, kind: "heading", weight: 600 })}
      ${c.backSteps.map((step, index) => {
        const cy = 566 + index * 48;
        return `<circle cx="1221" cy="${cy}" r="12" fill="${C.indigoSoft}" stroke="#D2CFF0"/>
        ${text({ locale, x: 1221, y: cy + 5, value: String(index + 1), size: 13, kind: "mono", weight: 500, fill: C.indigo, anchor: "middle" })}
        ${text({ locale, x: 1248, y: cy + 5, value: step, size: locale === "en" ? 17 : 18, weight: 600 })}`;
      }).join("\n")}
    `,
  });
  const footer = region({
    id: `${prefix}-authority`,
    x: 64,
    y: 842,
    width: 1472,
    height: 100,
    content: `
      <circle cx="84" cy="885" r="7" fill="${C.sage}"/>
      ${text({ locale, x: 106, y: 882, value: c.upkeep, size: locale === "en" ? 20 : 21, kind: "heading", weight: 600 })}
      ${text({ locale, x: 106, y: 912, value: c.upkeepLead, size: locale === "en" ? 16 : 17, fill: C.secondary })}
      <line x1="802" y1="868" x2="802" y2="925" stroke="${C.border}"/>
      <circle cx="844" cy="885" r="7" fill="${C.warm}"/>
      ${text({ locale, x: 866, y: 882, value: c.authority, size: locale === "en" ? 20 : 21, kind: "heading", weight: 600 })}
      ${text({ locale, x: 866, y: 912, value: c.authorityLead, size: locale === "en" ? 16 : 17, fill: C.secondary })}
    `,
  });

  return `
    ${logo({ x: 64, y: 62, size: 58, prefix })}
    ${region({
      id: `${prefix}-heading`,
      x: 144,
      y: 58,
      width: 1392,
      height: 142,
      checkOverlap: false,
      content: `
        ${text({ locale, x: 144, y: 84, value: c.eyebrow, size: 13, kind: "mono", weight: 500, fill: C.tertiary })}
        ${lines({ locale, x: 144, y: 132, values: c.title, size: locale === "en" ? 42 : 44, lineHeight: 46, kind: "heading", weight: 600 })}
      `,
    })}
    <path d="M412 352 C438 352 448 392 470 410" fill="none" stroke="${C.indigo}" stroke-width="2" marker-end="url(#${marker})"/>
    <path d="M412 610 C438 610 448 572 470 554" fill="none" stroke="${C.indigo}" stroke-width="2" marker-end="url(#${marker})"/>
    <path d="M1120 474 C1160 474 1176 474 1194 474" fill="none" stroke="${C.indigo}" stroke-width="2" marker-end="url(#${marker})"/>
    ${sources}
    ${memories}
    ${page}
    ${region({
      id: `${prefix}-changed-support`,
      x: 1124,
      y: 704,
      width: 390,
      height: 80,
      content: `
        <path d="M1064 698 C1064 718 1110 716 1142 716" fill="none" stroke="${C.amber}" stroke-width="1.5"/>
        ${text({ locale, x: 1154, y: 724, value: c.changed, size: 13, kind: "mono", weight: 500, fill: C.amberDark })}
        ${text({ locale, x: 1154, y: 754, value: c.changedLead, size: locale === "en" ? 17 : 18, weight: 600, fill: C.secondary })}
      `,
    })}
    ${output}
    ${footer}
  `;
}

function mobileInput({
  locale,
  id,
  x,
  y,
  title,
  lead,
  description,
  tags,
  glyph,
}) {
  const tagLines = tags.length === 3
    ? [tags.slice(0, 2), tags.slice(2)]
    : [tags];
  const firstRow = chipRow({
    locale,
    x: x + 24,
    y: y + 210,
    labels: tagLines[0],
    gap: 8,
    height: 38,
    mono: true,
    size: 18,
  }).markup;
  const secondRow = tagLines[1]
    ? chipRow({
      locale,
      x: x + 24,
      y: y + 256,
      labels: tagLines[1],
      gap: 8,
      height: 38,
      mono: true,
      size: 18,
    }).markup
    : "";
  return region({
    id,
    x,
    y,
    width: 304,
    height: 300,
    content: `
      <rect x="${x}" y="${y}" width="304" height="300" rx="8" fill="${C.surface}" stroke="${C.border}"/>
      <line x1="${x}" y1="${y + 18}" x2="${x}" y2="${y + 282}" stroke="${C.indigo}" stroke-width="4"/>
      ${glyph({ x: x + 24, y: y + 26 })}
      ${text({ locale, x: x + 64, y: y + 48, value: title, size: locale === "en" ? 30 : 32, kind: "heading", weight: 600 })}
      ${lines({
        locale,
        x: x + 24,
        y: y + 98,
        values: Array.isArray(lead) ? lead : [lead],
        size: locale === "en" ? 22 : 23,
        lineHeight: 26,
        weight: 600,
        fill: C.secondary,
      })}
      ${lines({
        locale,
        x: x + 24,
        y: y + 158,
        values: Array.isArray(description) ? description : [description],
        size: locale === "en" ? 22 : 23,
        lineHeight: 26,
        fill: C.secondary,
      })}
      ${firstRow}
      ${secondRow}
    `,
  });
}

function overviewMobile(c, locale, prefix) {
  const marker = `${prefix}-arrow`;
  const page = overviewPage({
    locale,
    c,
    x: 40,
    y: 580,
    width: 640,
    height: 720,
    prefix,
    mobile: true,
  });
  const sources = mobileInput({
    locale,
    id: `${prefix}-sources`,
    x: 40,
    y: 230,
    title: c.sources,
    lead: c.mobileSourcesLead,
    description: c.mobileSourcesDescription,
    tags: c.sourceTags,
    glyph: documentGlyph,
  });
  const memories = mobileInput({
    locale,
    id: `${prefix}-memories`,
    x: 376,
    y: 230,
    title: c.memories,
    lead: c.mobileMemoriesLead,
    description: c.mobileMemoriesDescription,
    tags: c.memoryTags,
    glyph: memoryGlyph,
  });
  const outputWords = c.backWords.map((word, index) => {
    const x = 142 + index * 220;
    return `${text({
      locale,
      x,
      y: 1464,
      value: word,
      size: locale === "en" ? 30 : 32,
      kind: "heading",
      weight: 600,
      anchor: "middle",
    })}`;
  }).join("\n");

  return `
    ${logo({ x: 40, y: 46, size: 58, prefix })}
    ${region({
      id: `${prefix}-heading`,
      x: 122,
      y: 44,
      width: 558,
      height: 190,
      checkOverlap: false,
      content: `
        ${text({ locale, x: 122, y: 70, value: c.eyebrow, size: 20, kind: "mono", weight: 500, fill: C.tertiary })}
        ${lines({ locale, x: 122, y: 112, values: c.mobileTitle, size: locale === "en" ? 30 : 34, lineHeight: locale === "en" ? 36 : 43, kind: "heading", weight: 600 })}
      `,
    })}
    ${sources}
    ${memories}
    <path d="M192 530 C192 552 284 552 330 570" fill="none" stroke="${C.indigo}" stroke-width="2.2" marker-end="url(#${marker})"/>
    <path d="M528 530 C528 552 436 552 390 570" fill="none" stroke="${C.indigo}" stroke-width="2.2" marker-end="url(#${marker})"/>
    ${page}
    ${region({
      id: `${prefix}-changed-support`,
      x: 254,
      y: 1310,
      width: 426,
      height: 72,
      content: `
        <path d="M660 1124 C674 1210 674 1270 632 1318" fill="none" stroke="${C.amber}" stroke-width="1.5"/>
        ${text({ locale, x: 280, y: 1338, value: c.changed, size: 20, kind: "mono", weight: 500, fill: C.amberDark })}
        ${text({ locale, x: 280, y: 1372, value: c.changedLead, size: locale === "en" ? 24 : 25, weight: 600, fill: C.secondary })}
      `,
    })}
    ${region({
      id: `${prefix}-back-to-work`,
      x: 40,
      y: 1398,
      width: 640,
      height: 94,
      content: `
        ${text({ locale, x: 40, y: 1424, value: c.backLabel, size: 20, kind: "mono", weight: 500, fill: C.tertiary })}
        ${outputWords}
      `,
    })}
    ${region({
      id: `${prefix}-authority`,
      x: 40,
      y: 1508,
      width: 640,
      height: 238,
      content: `
        <circle cx="54" cy="1538" r="7" fill="${C.sage}"/>
        ${text({ locale, x: 78, y: 1536, value: c.upkeep, size: locale === "en" ? 26 : 28, kind: "heading", weight: 600 })}
        ${lines({
          locale,
          x: 78,
          y: 1574,
          values: locale === "en" ? ["Background upkeep needs", "a configured model."] : [c.upkeepLead],
          size: locale === "en" ? 24 : 25,
          lineHeight: 30,
          fill: C.secondary,
        })}
        <circle cx="54" cy="1650" r="7" fill="${C.warm}"/>
        ${text({ locale, x: 78, y: 1648, value: c.authority, size: locale === "en" ? 26 : 28, kind: "heading", weight: 600 })}
        ${lines({
          locale,
          x: 78,
          y: 1686,
          values: locale === "en"
            ? ["Automatic refresh proposes changes", "to pages you edited."]
            : [c.authorityLead],
          size: locale === "en" ? 24 : 25,
          lineHeight: 30,
          fill: C.secondary,
        })}
      `,
    })}
  `;
}

function makeOverview(locale, viewport) {
  const c = OVERVIEW_COPY[locale];
  if (!c) throw new Error(`Unknown overview locale: ${locale}`);
  const { width, height } = VIEWPORTS.overview[viewport];
  const prefix = `overview-${locale}-${viewport}`;
  const body = viewport === "mobile"
    ? overviewMobile(c, locale, prefix)
    : overviewDesktop(c, locale, prefix);
  const suffix = locale === "en" ? "" : `-${locale}`;
  const name = `wenlan-system${suffix}${viewport === "mobile" ? "-mobile" : ""}`;
  const svg = `<svg width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" xmlns="http://www.w3.org/2000/svg" role="img" aria-labelledby="title desc">
    <title id="title">${esc(c.title.join(" "))}</title>
    <desc id="desc">${esc(c.description)}</desc>
    <style>text { font-kerning: normal; }</style>
    <rect width="${width}" height="${height}" fill="${C.paper}"/>
    ${body}
    <defs>
      ${logoDefs(prefix)}
      ${arrowMarker(`${prefix}-arrow`)}
      <filter id="${prefix}-page-shadow" x="-20%" y="-20%" width="140%" height="150%">
        <feDropShadow dx="0" dy="14" stdDeviation="18" flood-color="#1A1A2E" flood-opacity="0.10"/>
        <feDropShadow dx="0" dy="2" stdDeviation="3" flood-color="#1A1A2E" flood-opacity="0.06"/>
      </filter>
    </defs>
  </svg>
`.replace(/[ \t]+$/gmu, "");
  return {
    group: "overview",
    name,
    width,
    height,
    background: C.paper,
    requiredCopy: [
      c.eyebrow,
      ...c.title,
      c.sources,
      c.memories,
      c.pageLabel,
      c.pageTitle,
      c.backLabel,
      c.changed,
      c.upkeep,
      c.authority,
    ],
    svg,
  };
}

function networkEdgeLabel({
  locale,
  x,
  y,
  value,
  size = 11,
}) {
  const width = approximateWidth(value, locale, true, size) + 18;
  return `<g>
    <rect x="${x - width / 2}" y="${y - size - 5}" width="${width}" height="${size + 12}" rx="4" fill="${C.paper}" fill-opacity="0.94"/>
    ${text({
      locale,
      x,
      y,
      value,
      size,
      kind: "mono",
      weight: 500,
      fill: C.secondary,
      anchor: "middle",
    })}
  </g>`;
}

function networkEntityNode({
  locale,
  c,
  prefix,
  cx,
  cy,
  radius,
  title,
  subtype,
}) {
  const mobile = prefix.endsWith("-mobile");
  const titleSize = locale === "en" && approximateWidth(title, locale, false, 21) > radius * 1.7
    ? 18
    : (locale === "en" ? 21 : 22);
  return region({
    id: `${prefix}-entity-${title}`,
    x: cx - radius,
    y: cy - radius,
    width: radius * 2,
    height: radius * 2,
    content: `
      <circle cx="${cx}" cy="${cy}" r="${radius}" fill="${C.indigoSoft}" stroke="#AAA5E5" stroke-width="1.5"/>
      ${text({ locale, x: cx, y: cy - 28, value: c.entity, size: mobile ? 15 : 10, kind: "mono", weight: 500, fill: C.indigo, anchor: "middle" })}
      ${text({ locale, x: cx, y: cy + 5, value: title, size: titleSize, weight: 600, anchor: "middle" })}
      ${text({ locale, x: cx, y: cy + 34, value: subtype, size: mobile ? 14 : 10, kind: "mono", weight: 500, fill: C.secondary, anchor: "middle" })}
    `,
  });
}

function networkSourceNode({
  locale,
  c,
  prefix,
  cx,
  cy,
  radius,
  title,
}) {
  const mobile = prefix.endsWith("-mobile");
  return region({
    id: `${prefix}-source-${title}`,
    x: cx - radius,
    y: cy - radius,
    width: radius * 2,
    height: radius * 2,
    content: `
      <circle cx="${cx}" cy="${cy}" r="${radius}" fill="${C.raised}" stroke="${C.border}" stroke-width="1.5"/>
      ${documentGlyph({ x: cx - 13, y: cy - 62, color: C.secondary })}
      ${text({ locale, x: cx, y: cy - 16, value: c.sourcePage, size: mobile ? 15 : 10, kind: "mono", weight: 500, fill: C.tertiary, anchor: "middle" })}
      ${text({ locale, x: cx, y: cy + 22, value: title, size: locale === "en" ? 19 : 20, weight: 600, anchor: "middle" })}
    `,
  });
}

function networkKnowledgeNode({
  locale,
  c,
  prefix,
  cx,
  cy,
  radius,
  title,
}) {
  const mobile = prefix.endsWith("-mobile");
  return region({
    id: `${prefix}-knowledge-${title}`,
    x: cx - radius,
    y: cy - radius,
    width: radius * 2,
    height: radius * 2,
    content: `
      <circle cx="${cx}" cy="${cy}" r="${radius}" fill="${C.surface}" stroke="${C.indigo}" stroke-width="1.6"/>
      ${text({ locale, x: cx, y: cy - 20, value: c.knowledgePage, size: mobile ? 15 : 10, kind: "mono", weight: 500, fill: C.indigo, anchor: "middle" })}
      ${text({ locale, x: cx, y: cy + 18, value: title, size: locale === "en" ? 20 : 22, weight: 600, anchor: "middle" })}
    `,
  });
}

function networkMemoryNode({
  locale,
  c,
  prefix,
  cx,
  cy,
  id,
  labelX = cx + 20,
  labelAnchor = "start",
  mobile = false,
}) {
  const width = mobile ? 150 : 132;
  const x = labelAnchor === "end" ? labelX - width : cx - 18;
  return region({
    id: `${prefix}-memory-${id}`,
    x,
    y: cy - 28,
    width,
    height: 58,
    content: `
      <circle cx="${cx}" cy="${cy}" r="${mobile ? 10 : 9}" fill="${C.indigo}"/>
      <circle cx="${cx}" cy="${cy}" r="${mobile ? 18 : 16}" fill="none" stroke="#D2CFF0"/>
      ${text({ locale, x: labelX, y: cy - 3, value: id, size: mobile ? 17 : 13, kind: "mono", weight: 500, fill: C.ink, anchor: labelAnchor })}
      ${text({ locale, x: labelX, y: cy + (mobile ? 22 : 17), value: c.memory, size: mobile ? 15 : 10, kind: "mono", weight: 500, fill: C.tertiary, anchor: labelAnchor })}
    `,
  });
}

function networkHero({
  locale,
  c,
  prefix,
  cx,
  cy,
  mobile = false,
}) {
  const radius = mobile ? 170 : 190;
  const width = mobile ? 300 : 330;
  const height = mobile ? 174 : 190;
  const x = cx - width / 2;
  const y = cy - height / 2;
  const pad = mobile ? 28 : 30;
  const currentWidth = locale === "en" ? (mobile ? 92 : 82) : (mobile ? 82 : 72);
  return region({
    id: `${prefix}-hero`,
    x: cx - radius,
    y: cy - radius,
    width: radius * 2,
    height: radius * 2,
    checkOverlap: false,
    content: `
      <circle cx="${cx}" cy="${cy}" r="${radius}" fill="${C.indigoSoft}" fill-opacity="0.72" stroke="#D2CFF0" stroke-width="1.5"/>
      <g filter="url(#${prefix}-page-shadow)">
        <rect x="${x}" y="${y}" width="${width}" height="${height}" rx="8" fill="${C.surface}" stroke="${C.border}"/>
      </g>
      ${text({ locale, x: x + pad, y: y + 36, value: c.knowledgePage, size: mobile ? 17 : 11, kind: "mono", weight: 500, fill: C.indigo })}
      ${chip({
        locale,
        x: x + width - currentWidth - pad,
        y: y + (mobile ? 18 : 16),
        label: c.current,
        width: currentWidth,
        height: mobile ? 34 : 28,
        fill: C.sageSoft,
        stroke: "#C7DACB",
        color: C.sageDark,
        mono: true,
        size: mobile ? 15 : 11,
      }).markup}
      ${text({ locale, x: x + pad, y: y + (mobile ? 88 : 82), value: c.heroTitle, size: mobile ? (locale === "en" ? 27 : 28) : (locale === "en" ? 30 : 31), kind: "heading", weight: 600 })}
      ${text({ locale, x: x + pad, y: y + (mobile ? 118 : 112), value: c.heroMeta, size: mobile ? 16 : 12, kind: "mono", weight: 500, fill: C.tertiary })}
      <rect x="${x + pad}" y="${y + (mobile ? 150 : 150)}" width="${mobile ? 168 : 184}" height="${mobile ? 7 : 8}" rx="4" fill="#D9DEE7"/>
      <rect x="${x + pad + (mobile ? 180 : 196)}" y="${y + (mobile ? 150 : 150)}" width="${mobile ? 56 : 70}" height="${mobile ? 7 : 8}" rx="4" fill="#E3E7EE"/>
    `,
  });
}

// Both layouts portray the same example. Coordinates may change; evidence and
// relation endpoints must not silently change when GitHub selects a mobile asset.
const NETWORK_EDGES = [
  ["sourceOne", "page", "citation", C.indigo, 1.5, 0.46],
  ["sourceTwo", "page", "citation", C.indigo, 1.5, 0.46],
  ["memoryOne", "page", "support", C.indigo, 1.8, 0.62],
  ["memoryTwo", "page", "support", C.indigo, 1.8, 0.62],
  ["memoryOne", "entityOne", "about", C.indigo, 1.8, 0.62],
  ["page", "linkedPage", "refines", C.indigo, 1.5, 0.46],
  ["sourceOne", "linkedPage", "citation", C.secondary, 1.2, 0.32],
  ["entityOne", "entityTwo", "related_to", C.sage, 2.2, 0.78],
  ["entityOne", "entityThree", "related_to", C.sage, 1.6, 0.68],
  ["entityFour", "entityOne", "part_of", C.sage, 3.4, 0.84],
  ["entityTwo", "entityThree", "related_to", C.sage, 1.4, 0.60],
  ["entityThree", "entityFour", "related_to", C.sage, 1.8, 0.66],
];

function networkEdges(prefix, paths) {
  if (paths.length !== NETWORK_EDGES.length) throw new Error("Incomplete network layout");
  return NETWORK_EDGES.map(([from, to, relation, color, width, opacity], index) => (
    `<path data-from="${from}" data-to="${to}" data-relation="${relation}" d="${paths[index]}" fill="none" stroke="${color}" stroke-width="${width}" stroke-opacity="${opacity}" stroke-linecap="round"${color === C.sage ? ` marker-end="url(#${prefix}-relation-arrow)"` : ""}/>`
  )).join("\n");
}

function networkDesktop(c, locale, prefix) {
  const labels = c.edgeLabels;

  return `
    ${logo({ x: 68, y: 60, size: 58, prefix })}
    ${region({
      id: `${prefix}-heading`,
      x: 148,
      y: 56,
      width: 1578,
      height: 138,
      checkOverlap: false,
      content: `
        ${text({ locale, x: 148, y: 84, value: c.eyebrow, size: 13, kind: "mono", weight: 500, fill: C.tertiary })}
        ${text({ locale, x: 148, y: 136, value: c.title, size: locale === "en" ? 44 : 43, kind: "heading", weight: 600 })}
        ${text({ locale, x: 148, y: 178, value: c.subtitle, size: locale === "en" ? 18 : 19, fill: C.secondary })}
      `,
    })}
    <g data-fit-region="${prefix}-graph-field" data-fit-x="80" data-fit-y="230" data-fit-width="1640" data-fit-height="780" data-check-overlap="true">
    <path d="M1090 292 C1248 214 1514 232 1662 354 C1768 474 1720 752 1560 890 C1420 1002 1170 944 1082 778 C1008 638 988 420 1090 292Z" fill="${C.sageSoft}" fill-opacity="0.76" stroke="#C7DACB" stroke-width="1.5"/>
    ${text({ locale, x: 1110, y: 298, value: c.entityCommunity, size: 11, kind: "mono", weight: 500, fill: C.sageDark })}
    ${text({ locale, x: 1110, y: 318, value: c.communityMethod, size: 9, kind: "mono", weight: 500, fill: C.tertiary })}
    ${networkEdges(prefix, [
      "M350 405 C454 428 520 492 590 540",
      "M448 778 C522 760 566 700 610 680",
      "M496 536 C542 540 558 566 580 580",
      "M520 900 C546 812 592 762 650 740",
      "M496 528 C700 300 956 314 1080 452",
      "M872 744 C902 792 932 812 966 826",
      "M328 450 C430 650 650 934 910 900",
      "M1260 420 C1312 382 1348 354 1360 344",
      "M1270 512 C1360 530 1408 560 1422 588",
      "M1302 724 C1270 704 1246 660 1224 576",
      "M1492 422 C1524 466 1538 504 1534 518",
      "M1460 680 C1436 700 1420 714 1412 724",
    ])}
    ${networkSourceNode({ locale, c, prefix, cx: 270, cy: 360, radius: 92, title: c.sourceOne })}
    ${networkSourceNode({ locale, c, prefix, cx: 360, cy: 800, radius: 94, title: c.sourceTwo })}
    ${networkHero({ locale, c, prefix, cx: 770, cy: 610 })}
    ${networkMemoryNode({ locale, c, prefix, cx: 480, cy: 535, id: "mem_42", labelX: 460, labelAnchor: "end" })}
    ${networkMemoryNode({ locale, c, prefix, cx: 520, cy: 915, id: "mem_77" })}
    ${networkKnowledgeNode({ locale, c, prefix, cx: 1000, cy: 890, radius: 90, title: c.linkedPage })}
    ${networkEntityNode({ locale, c, prefix, cx: 1180, cy: 480, radius: 100, title: c.entityOne, subtype: c.project })}
    ${networkEntityNode({ locale, c, prefix, cx: 1450, cy: 345, radius: 84, title: c.entityTwo, subtype: c.tool })}
    ${networkEntityNode({ locale, c, prefix, cx: 1510, cy: 610, radius: 88, title: c.entityThree, subtype: c.method })}
    ${networkEntityNode({ locale, c, prefix, cx: 1340, cy: 790, radius: 92, title: c.entityFour, subtype: c.concept })}
    ${networkEdgeLabel({ locale, x: 450, y: 444, value: labels[0] })}
    ${networkEdgeLabel({ locale, x: 548, y: 574, value: labels[1] })}
    ${networkEdgeLabel({ locale, x: 900, y: 365, value: labels[2] })}
    ${networkEdgeLabel({ locale, x: 910, y: 786, value: labels[3] })}
    ${networkEdgeLabel({ locale, x: 1324, y: 374, value: labels[4] })}
    ${networkEdgeLabel({ locale, x: 1262, y: 654, value: labels[5] })}
    </g>
    ${region({
      id: `${prefix}-footer`,
      x: 80,
      y: 1018,
      width: 1640,
      height: 58,
      checkOverlap: false,
      content: `
        ${text({ locale, x: 900, y: 1060, value: c.footer, size: locale === "en" ? 15 : 16, kind: "mono", weight: 500, fill: C.secondary, anchor: "middle" })}
      `,
    })}
  `;
}

function networkMobile(c, locale, prefix) {
  const labels = c.edgeLabels;

  return `
    ${logo({ x: 40, y: 48, size: 58, prefix })}
    ${region({
      id: `${prefix}-heading`,
      x: 122,
      y: 44,
      width: 558,
      height: 266,
      checkOverlap: false,
      content: `
        ${text({ locale, x: 122, y: 70, value: c.eyebrow, size: 20, kind: "mono", weight: 500, fill: C.tertiary })}
        ${lines({ locale, x: 122, y: 116, values: c.mobileTitle, size: locale === "en" ? 34 : 36, lineHeight: 42, kind: "heading", weight: 600 })}
        ${lines({ locale, x: 122, y: 210, values: c.mobileSubtitle, size: locale === "en" ? 23 : 24, lineHeight: 30, fill: C.secondary })}
      `,
    })}
    <g data-fit-region="${prefix}-graph-field" data-fit-x="40" data-fit-y="280" data-fit-width="640" data-fit-height="1290" data-check-overlap="true">
    <path d="M282 1020 C414 948 596 1060 648 1196 C710 1374 578 1540 390 1534 C182 1538 84 1392 94 1268 C106 1148 182 1062 282 1020Z" fill="${C.sageSoft}" fill-opacity="0.76" stroke="#C7DACB" stroke-width="1.8"/>
    ${text({ locale, x: 462, y: 968, value: c.entityCommunity, size: 17, kind: "mono", weight: 500, fill: C.sageDark, anchor: "middle" })}
    ${text({ locale, x: 462, y: 994, value: c.communityMethod, size: 14, kind: "mono", weight: 500, fill: C.tertiary, anchor: "middle" })}
    ${networkEdges(prefix, [
      "M220 424 C248 470 280 522 296 572",
      "M504 432 C470 486 438 530 414 568",
      "M560 594 C560 626 534 642 498 656",
      "M218 560 C242 576 256 592 266 600",
      "M578 576 C682 722 672 956 642 1018 C620 1068 516 1116 440 1108",
      "M272 874 C230 898 210 926 198 936",
      "M92 384 C44 520 50 788 108 908",
      "M424 1154 C450 1174 474 1190 500 1216",
      "M372 1188 C380 1250 390 1310 398 1344",
      "M244 1224 C266 1206 294 1182 310 1172",
      "M526 1326 C510 1350 494 1368 474 1384",
      "M344 1384 C310 1366 282 1348 268 1326",
    ])}
    ${networkSourceNode({ locale, c, prefix, cx: 170, cy: 365, radius: 80, title: c.sourceOne })}
    ${networkSourceNode({ locale, c, prefix, cx: 552, cy: 365, radius: 80, title: c.sourceTwo })}
    ${networkHero({ locale, c, prefix, cx: 360, cy: 730, mobile: true })}
    ${networkMemoryNode({ locale, c, prefix, cx: 560, cy: 576, id: "mem_42", labelX: 540, labelAnchor: "end", mobile: true })}
    ${networkMemoryNode({ locale, c, prefix, cx: 200, cy: 555, id: "mem_77", labelX: 178, labelAnchor: "end", mobile: true })}
    ${networkKnowledgeNode({ locale, c, prefix, cx: 140, cy: 980, radius: 82, title: c.linkedPage })}
    ${networkEntityNode({ locale, c, prefix, cx: 360, cy: 1110, radius: 80, title: c.entityOne, subtype: c.project })}
    ${networkEntityNode({ locale, c, prefix, cx: 560, cy: 1260, radius: 76, title: c.entityTwo, subtype: c.tool })}
    ${networkEntityNode({ locale, c, prefix, cx: 410, cy: 1420, radius: 76, title: c.entityThree, subtype: c.method })}
    ${networkEntityNode({ locale, c, prefix, cx: 190, cy: 1290, radius: 86, title: c.entityFour, subtype: c.concept })}
    ${networkEdgeLabel({ locale, x: 264, y: 494, value: labels[0], size: 17 })}
    ${networkEdgeLabel({ locale, x: 554, y: 642, value: labels[1], size: 17 })}
    ${networkEdgeLabel({ locale, x: 612, y: 866, value: labels[2], size: 17 })}
    ${networkEdgeLabel({ locale, x: 234, y: 928, value: labels[3], size: 17 })}
    ${networkEdgeLabel({ locale, x: 488, y: 1202, value: labels[4], size: 17 })}
    ${networkEdgeLabel({ locale, x: 282, y: 1214, value: labels[5], size: 17 })}
    </g>
    ${region({
      id: `${prefix}-footer`,
      x: 40,
      y: 1590,
      width: 640,
      height: 58,
      checkOverlap: false,
      content: `
        ${text({ locale, x: 360, y: 1632, value: c.footer, size: locale === "en" ? 18 : 19, kind: "mono", weight: 500, fill: C.secondary, anchor: "middle" })}
      `,
    })}
  `;
}

function makeKnowledgeNetwork(locale, viewport) {
  const c = NETWORK_COPY[locale];
  if (!c) throw new Error(`Unknown knowledge-network locale: ${locale}`);
  const { width, height } = VIEWPORTS.network[viewport];
  const prefix = `network-${locale}-${viewport}`;
  const body = viewport === "mobile"
    ? networkMobile(c, locale, prefix)
    : networkDesktop(c, locale, prefix);
  const suffix = locale === "en" ? "" : `-${locale}`;
  const name = `wenlan-knowledge-network${suffix}${viewport === "mobile" ? "-mobile" : ""}`;
  const svg = `<svg width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" xmlns="http://www.w3.org/2000/svg" role="img" aria-labelledby="title desc">
    <title id="title">${esc(c.title)}</title>
    <desc id="desc">${esc(c.description)}</desc>
    <style>text { font-kerning: normal; }</style>
    <rect width="${width}" height="${height}" fill="${C.paper}"/>
    ${body}
    <defs>
      ${logoDefs(prefix)}
      <marker id="${prefix}-relation-arrow" markerWidth="12" markerHeight="12" refX="9" refY="6" orient="auto" markerUnits="userSpaceOnUse">
        <path d="M2 2 L9 6 L2 10" fill="none" stroke="${C.sage}" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>
      </marker>
      <filter id="${prefix}-page-shadow" x="-20%" y="-20%" width="140%" height="150%">
        <feDropShadow dx="0" dy="12" stdDeviation="16" flood-color="#1A1A2E" flood-opacity="0.09"/>
        <feDropShadow dx="0" dy="2" stdDeviation="3" flood-color="#1A1A2E" flood-opacity="0.05"/>
      </filter>
    </defs>
  </svg>
`.replace(/[ \t]+$/gmu, "");
  return {
    group: "network",
    name,
    width,
    height,
    background: C.paper,
    requiredCopy: [
      c.eyebrow,
      c.title,
      c.knowledgePage,
      c.entity,
      c.sourcePage,
      c.memory,
      c.heroTitle,
      c.entityCommunity,
      c.communityMethod,
      ...c.edgeLabels,
      c.footer,
    ],
    svg,
  };
}

function memoryObjectDesktop(c, locale, prefix) {
  const learned = chip({
    locale,
    x: 396,
    y: 382,
    label: c.earlierState,
    width: locale === "en" ? 104 : 94,
    height: 28,
    mono: true,
    size: 12,
  }).markup;
  const confirmed = chip({
    locale,
    x: 538,
    y: 630,
    label: c.correctedState,
    width: locale === "en" ? 112 : 94,
    height: 28,
    fill: C.sageSoft,
    stroke: "#C7DACB",
    color: C.sageDark,
    mono: true,
    size: 12,
  }).markup;
  const oldMemory = chip({
    locale,
    x: 436,
    y: 770,
    label: "mem_42",
    width: 88,
    height: 30,
    fill: C.indigoSoft,
    stroke: "#D2CFF0",
    color: C.indigo,
    mono: true,
  }).markup;
  return region({
    id: `${prefix}-memory-object`,
    x: 70,
    y: 236,
    width: 640,
    height: 670,
    checkOverlap: false,
    content: `
      ${text({ locale, x: 88, y: 258, value: c.memoryLabel, size: 13, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 88, y: 302, value: c.memoryTitle, size: locale === "en" ? 30 : 32, kind: "heading", weight: 600 })}
      <g opacity="0.92">
        <rect x="112" y="362" width="420" height="212" rx="8" fill="${C.raised}" stroke="${C.border}"/>
        ${text({ locale, x: 142, y: 400, value: c.earlierMemory, size: 12, kind: "mono", weight: 500, fill: C.tertiary })}
        ${learned}
        ${text({ locale, x: 142, y: 448, value: "mem_42", size: 28, kind: "heading", weight: 600 })}
        <rect x="142" y="478" width="250" height="9" rx="4.5" fill="#D9DEE7"/>
        <rect x="142" y="500" width="320" height="9" rx="4.5" fill="#E3E7EE"/>
        ${chip({ locale, x: 142, y: 528, label: "source_07", width: 108, height: 30, fill: C.surface, stroke: C.border, color: C.secondary, mono: true }).markup}
      </g>
      <path d="M438 552 C494 570 512 592 512 622" fill="none" stroke="${C.indigo}" stroke-width="2" marker-end="url(#${prefix}-arrow)"/>
      ${text({ locale, x: 454, y: 584, value: c.correct, size: 11, kind: "mono", weight: 500, fill: C.indigo })}
      <g filter="url(#${prefix}-card-shadow)">
        <rect x="220" y="610" width="460" height="238" rx="8" fill="${C.surface}" stroke="${C.border}"/>
      </g>
      ${text({ locale, x: 250, y: 648, value: c.correctedMemory, size: 12, kind: "mono", weight: 500, fill: C.tertiary })}
      ${confirmed}
      ${text({ locale, x: 250, y: 696, value: "mem_77", size: 30, kind: "heading", weight: 600 })}
      <rect x="250" y="724" width="286" height="9" rx="4.5" fill="#D9DEE7"/>
      <rect x="250" y="746" width="352" height="9" rx="4.5" fill="#E3E7EE"/>
      ${text({ locale, x: 250, y: 790, value: c.supersedes, size: 11, kind: "mono", weight: 500, fill: C.indigo })}
      <path d="M346 786 H420" stroke="${C.indigo}" stroke-width="1.6" marker-end="url(#${prefix}-arrow)"/>
      ${oldMemory}
      ${text({ locale, x: 250, y: 826, value: c.oldLinked, size: locale === "en" ? 14 : 15, weight: 600, fill: C.sageDark })}
      <circle cx="102" cy="642" r="5" fill="${C.indigo}"/>
      ${text({ locale, x: 88, y: 674, value: c.enrich, size: 11, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 88, y: 696, value: c.enrichDetail, size: locale === "en" ? 14 : 15, weight: 600, fill: C.secondary })}
      <circle cx="102" cy="728" r="5" fill="${C.indigo}"/>
      ${text({ locale, x: 88, y: 760, value: c.connect, size: 11, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 88, y: 782, value: c.connectDetail, size: locale === "en" ? 14 : 15, weight: 600, fill: C.secondary })}
    `,
  });
}

function refineryHubDesktop(c, locale, prefix) {
  return region({
    id: `${prefix}-refinery`,
    x: 698,
    y: 260,
    width: 360,
    height: 610,
    checkOverlap: false,
    content: `
      <rect x="760" y="318" width="236" height="88" rx="8" fill="${C.raised}" stroke="${C.border}"/>
      ${documentGlyph({ x: 784, y: 344, color: C.secondary })}
      ${text({ locale, x: 826, y: 348, value: c.sourceChanged, size: 11, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 826, y: 378, value: "source_11", size: 19, kind: "heading", weight: 600 })}
      <path d="M878 406 V466" fill="none" stroke="${C.indigo}" stroke-width="2" marker-end="url(#${prefix}-arrow)"/>
      <circle cx="878" cy="578" r="114" fill="${C.surface}" stroke="${C.border}"/>
      <circle cx="878" cy="578" r="90" fill="none" stroke="${C.indigo}" stroke-width="3" stroke-dasharray="112 26"/>
      <circle cx="878" cy="578" r="58" fill="${C.indigoSoft}" stroke="#D2CFF0"/>
      ${text({ locale, x: 878, y: 568, value: c.refinery, size: 12, kind: "mono", weight: 500, fill: C.indigo, anchor: "middle" })}
      ${text({ locale, x: 878, y: 596, value: c.maintain[0], size: locale === "en" ? 22 : 23, kind: "heading", weight: 600, anchor: "middle" })}
      ${text({ locale, x: 878, y: 620, value: c.maintain[1], size: locale === "en" ? 13 : 14, weight: 600, fill: C.secondary, anchor: "middle" })}
      <circle cx="878" cy="474" r="7" fill="${C.indigo}"/>
      <circle cx="974" cy="578" r="7" fill="${C.indigo}"/>
      <circle cx="878" cy="682" r="7" fill="${C.indigo}"/>
      <circle cx="782" cy="578" r="7" fill="${C.indigo}"/>
      <path d="M680 728 C728 728 744 660 780 622" fill="none" stroke="${C.indigo}" stroke-width="2" marker-end="url(#${prefix}-arrow)"/>
      ${text({ locale, x: 700, y: 758, value: c.memoryCorrected, size: 15, kind: "mono", weight: 500, fill: C.indigo })}
      <path d="M878 692 V782" fill="none" stroke="${C.warm}" stroke-width="1.8" marker-end="url(#${prefix}-warm-arrow)"/>
      <rect x="730" y="798" width="296" height="64" rx="8" fill="${C.warmSoft}" stroke="#E6C9B8"/>
      ${text({ locale, x: 878, y: 824, value: c.contradiction, size: 11, kind: "mono", weight: 500, fill: C.warmDark, anchor: "middle" })}
      ${text({ locale, x: 878, y: 848, value: c.wait, size: locale === "en" ? 15 : 16, kind: "heading", weight: 600, anchor: "middle" })}
    `,
  });
}

function pageObjectDesktop(c, locale, prefix) {
  return region({
    id: `${prefix}-page-object`,
    x: 1044,
    y: 236,
    width: 686,
    height: 680,
    checkOverlap: false,
    content: `
      ${text({ locale, x: 1080, y: 258, value: c.pageLabel, size: 13, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 1080, y: 302, value: c.pageTitle, size: locale === "en" ? 30 : 32, kind: "heading", weight: 600 })}
      <rect x="1130" y="388" width="500" height="406" rx="8" fill="${C.raised}" stroke="${C.border}"/>
      <rect x="1160" y="356" width="500" height="424" rx="8" fill="#FAFAFA" stroke="${C.border}"/>
      ${text({ locale, x: 1190, y: 394, value: locale === "en" ? "PAGE v11" : (locale === "zh-Hans" ? "页面 v11" : "頁面 v11"), size: 11, kind: "mono", weight: 500, fill: C.tertiary })}
      <rect x="1136" y="502" width="56" height="112" rx="8" fill="${C.amberSoft}" stroke="#E5D3AA"/>
      ${text({ locale, x: 1164, y: 542, value: "v11", size: 10, kind: "mono", weight: 500, fill: C.amberDark, anchor: "middle" })}
      ${text({ locale, x: 1164, y: 566, value: c.stale, size: 12, kind: "mono", weight: 500, fill: C.amberDark, anchor: "middle" })}
      <g filter="url(#${prefix}-page-shadow)">
        <rect x="1192" y="330" width="510" height="454" rx="8" fill="${C.surface}" stroke="${C.border}"/>
      </g>
      ${text({ locale, x: 1224, y: 370, value: c.pageVersion, size: 12, kind: "mono", weight: 500, fill: C.tertiary })}
      ${chip({ locale, x: 1568, y: 350, label: c.current, width: locale === "en" ? 102 : 82, height: 28, fill: C.sageSoft, stroke: "#C7DACB", color: C.sageDark, mono: true, size: 12 }).markup}
      ${text({ locale, x: 1224, y: 424, value: c.maintainedPage, size: locale === "en" ? 36 : 34, kind: "heading", weight: 600 })}
      ${text({ locale, x: 1224, y: 454, value: c.pageMeta, size: 13, kind: "mono", weight: 500, fill: C.tertiary })}
      <rect x="1224" y="514" width="280" height="9" rx="4.5" fill="#D9DEE7"/>
      <rect x="1224" y="536" width="346" height="9" rx="4.5" fill="#E3E7EE"/>
      ${chip({ locale, x: 1580, y: 505, label: "source_07", width: 108, height: 30, mono: true }).markup}
      <rect x="1212" y="574" width="470" height="88" rx="8" fill="${C.sageSoft}" stroke="#C7DACB"/>
      ${text({ locale, x: 1236, y: 602, value: c.verified, size: 11, kind: "mono", weight: 500, fill: C.sageDark })}
      <rect x="1236" y="620" width="250" height="8" rx="4" fill="#BFD0C2"/>
      <rect x="1236" y="640" width="322" height="8" rx="4" fill="#D6E2D8"/>
      ${chip({ locale, x: 1570, y: 606, label: "mem_77", width: 88, height: 30, fill: C.surface, stroke: "#C7DACB", color: C.sageDark, mono: true }).markup}
      ${text({ locale, x: 1224, y: 738, value: c.prior, size: locale === "en" ? 15 : 16, weight: 600, fill: C.sageDark })}
      ${text({ locale, x: 1638, y: 738, value: c.versions, size: 13, kind: "mono", weight: 500, fill: C.tertiary, anchor: "end" })}
      <path d="M1408 784 V822" fill="none" stroke="${C.warm}" stroke-width="1.8" marker-end="url(#${prefix}-warm-arrow)"/>
      <rect x="1140" y="838" width="562" height="64" rx="8" fill="${C.warmSoft}" stroke="#E6C9B8"/>
      ${text({ locale, x: 1164, y: 864, value: c.humanPage, size: 11, kind: "mono", weight: 500, fill: C.warmDark })}
      ${text({ locale, x: 1164, y: 888, value: c.humanLead, size: locale === "en" ? 15 : 16, weight: 600, fill: C.secondary })}
    `,
  });
}

function lifecycleFooterDesktop(c, locale, prefix) {
  return region({
    id: `${prefix}-footer`,
    x: 74,
    y: 966,
    width: 1652,
    height: 106,
    checkOverlap: false,
    content: `
      ${text({ locale, x: 88, y: 1007, value: c.background, size: 13, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 88, y: 1043, value: c.modelNote, size: locale === "en" ? 18 : 20, fill: C.secondary })}
      ${text({ locale, x: 1698, y: 1043, value: c.archive, size: locale === "en" ? 17 : 18, kind: "heading", weight: 600, fill: C.sageDark, anchor: "end" })}
    `,
  });
}

function lifecycleDesktop(c, locale, prefix) {
  return `
    ${logo({ x: 68, y: 60, size: 58, prefix })}
    ${region({
      id: `${prefix}-heading`,
      x: 148,
      y: 56,
      width: 1578,
      height: 130,
      checkOverlap: false,
      content: `
        ${text({ locale, x: 148, y: 84, value: c.eyebrow, size: 13, kind: "mono", weight: 500, fill: C.tertiary })}
        ${text({ locale, x: 148, y: 136, value: c.title, size: locale === "en" ? 48 : 46, kind: "heading", weight: 600 })}
        ${text({ locale, x: 148, y: 176, value: c.subtitle, size: locale === "en" ? 20 : 21, fill: C.secondary })}
      `,
    })}
    ${memoryObjectDesktop(c, locale, prefix)}
    ${refineryHubDesktop(c, locale, prefix)}
    ${pageObjectDesktop(c, locale, prefix)}
    <path d="M992 578 C1076 578 1128 606 1198 614" fill="none" stroke="${C.sage}" stroke-width="2.5" marker-end="url(#${prefix}-sage-arrow)"/>
    ${text({ locale, x: 1066, y: 562, value: c.affectedClaim, size: 14, kind: "mono", weight: 500, fill: C.sageDark, anchor: "middle" })}
    ${lifecycleFooterDesktop(c, locale, prefix)}
  `;
}

function memoryObjectMobile(c, locale, prefix) {
  return region({
    id: `${prefix}-memory-object`,
    x: 40,
    y: 286,
    width: 640,
    height: 546,
    checkOverlap: false,
    content: `
      ${text({ locale, x: 40, y: 318, value: c.memoryLabel, size: 20, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 40, y: 362, value: c.memoryTitle, size: locale === "en" ? 34 : 36, kind: "heading", weight: 600 })}
      <rect x="74" y="390" width="500" height="164" rx="8" fill="${C.raised}" stroke="${C.border}"/>
      ${text({ locale, x: 104, y: 428, value: c.earlierMemory, size: 20, kind: "mono", weight: 500, fill: C.tertiary })}
      ${chip({ locale, x: 426, y: 408, label: c.earlierState, width: locale === "en" ? 112 : 98, height: 38, mono: true, size: 18 }).markup}
      ${text({ locale, x: 104, y: 476, value: "mem_42", size: 32, kind: "heading", weight: 600 })}
      <rect x="104" y="496" width="270" height="9" rx="4.5" fill="#D9DEE7"/>
      ${chip({ locale, x: 104, y: 516, label: "source_07", width: 122, height: 30, fill: C.surface, mono: true, size: 18 }).markup}
      <path d="M554 554 V592" fill="none" stroke="${C.indigo}" stroke-width="2" marker-end="url(#${prefix}-arrow)"/>
      ${text({ locale, x: 532, y: 582, value: c.correct, size: 20, kind: "mono", weight: 500, fill: C.indigo, anchor: "end" })}
      <g filter="url(#${prefix}-card-shadow)">
        <rect x="126" y="602" width="540" height="224" rx="8" fill="${C.surface}" stroke="${C.border}"/>
      </g>
      ${text({ locale, x: 158, y: 640, value: c.correctedMemory, size: 20, kind: "mono", weight: 500, fill: C.tertiary })}
      ${chip({ locale, x: 510, y: 620, label: c.correctedState, width: locale === "en" ? 120 : 104, height: 38, fill: C.sageSoft, stroke: "#C7DACB", color: C.sageDark, mono: true, size: 18 }).markup}
      ${text({ locale, x: 158, y: 688, value: "mem_77", size: 34, kind: "heading", weight: 600 })}
      <rect x="158" y="708" width="294" height="9" rx="4.5" fill="#D9DEE7"/>
      ${text({ locale, x: 158, y: 754, value: c.supersedes, size: 20, kind: "mono", weight: 500, fill: C.indigo })}
      <path d="M286 748 H354" stroke="${C.indigo}" stroke-width="1.6" marker-end="url(#${prefix}-arrow)"/>
      ${chip({ locale, x: 372, y: 730, label: "mem_42", width: 96, height: 36, fill: C.indigoSoft, stroke: "#D2CFF0", color: C.indigo, mono: true, size: 18 }).markup}
      ${text({ locale, x: 158, y: 800, value: c.oldLinked, size: locale === "en" ? 24 : 25, weight: 600, fill: C.sageDark })}
    `,
  });
}

function refineryHubMobile(c, locale, prefix) {
  return region({
    id: `${prefix}-refinery`,
    x: 40,
    y: 850,
    width: 640,
    height: 450,
    checkOverlap: false,
    content: `
      <rect x="214" y="860" width="292" height="86" rx="8" fill="${C.raised}" stroke="${C.border}"/>
      ${documentGlyph({ x: 240, y: 888, color: C.secondary })}
      ${text({ locale, x: 282, y: 894, value: c.sourceChanged, size: 20, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 282, y: 928, value: "source_11", size: 28, kind: "heading", weight: 600 })}
      <path d="M360 946 V960" fill="none" stroke="${C.indigo}" stroke-width="2" marker-end="url(#${prefix}-arrow)"/>
      <path d="M126 780 C66 834 116 1002 250 1064" fill="none" stroke="${C.indigo}" stroke-width="1.8" marker-end="url(#${prefix}-arrow)"/>
      ${networkEdgeLabel({ locale, x: 144, y: 980, value: c.memoryCorrected, size: 19 })}
      <circle cx="360" cy="1080" r="110" fill="${C.surface}" stroke="${C.border}"/>
      <circle cx="360" cy="1080" r="90" fill="none" stroke="${C.indigo}" stroke-width="3" stroke-dasharray="112 29"/>
      ${text({ locale, x: 360, y: 1054, value: c.refinery, size: 20, kind: "mono", weight: 500, fill: C.indigo, anchor: "middle" })}
      ${text({ locale, x: 360, y: 1088, value: c.maintain[0], size: locale === "en" ? 28 : 30, kind: "heading", weight: 600, anchor: "middle" })}
      ${lines({ locale, x: 360, y: 1118, values: locale === "en" ? ["from current", "support"] : [c.maintain[1]], size: locale === "en" ? 18 : 23, lineHeight: 22, weight: 600, fill: C.secondary, anchor: "middle" })}
      <path d="M360 1190 V1208" fill="none" stroke="${C.warm}" stroke-width="1.8" marker-end="url(#${prefix}-warm-arrow)"/>
      <rect x="170" y="1220" width="380" height="72" rx="8" fill="${C.warmSoft}" stroke="#E6C9B8"/>
      ${text({ locale, x: 360, y: 1248, value: c.contradiction, size: 20, kind: "mono", weight: 500, fill: C.warmDark, anchor: "middle" })}
      ${text({ locale, x: 360, y: 1280, value: c.wait, size: locale === "en" ? 26 : 27, kind: "heading", weight: 600, anchor: "middle" })}
    `,
  });
}

function pageObjectMobile(c, locale, prefix) {
  return region({
    id: `${prefix}-page-object`,
    x: 40,
    y: 1308,
    width: 640,
    height: 620,
    checkOverlap: false,
    content: `
      ${text({ locale, x: 40, y: 1338, value: c.pageLabel, size: 20, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 40, y: 1382, value: c.pageTitle, size: locale === "en" ? 34 : 36, kind: "heading", weight: 600 })}
      <rect x="92" y="1470" width="520" height="344" rx="8" fill="${C.raised}" stroke="${C.border}"/>
      <rect x="116" y="1446" width="520" height="344" rx="8" fill="#FAFAFA" stroke="${C.border}"/>
      <rect x="70" y="1540" width="58" height="106" rx="8" fill="${C.amberSoft}" stroke="#E5D3AA"/>
      ${text({ locale, x: 99, y: 1582, value: "v11", size: 18, kind: "mono", weight: 500, fill: C.amberDark, anchor: "middle" })}
      ${text({ locale, x: 99, y: 1610, value: c.stale, size: 17, kind: "mono", weight: 500, fill: C.amberDark, anchor: "middle" })}
      <g filter="url(#${prefix}-page-shadow)">
        <rect x="138" y="1420" width="520" height="354" rx="8" fill="${C.surface}" stroke="${C.border}"/>
      </g>
      ${text({ locale, x: 170, y: 1462, value: c.pageVersion, size: 20, kind: "mono", weight: 500, fill: C.tertiary })}
      ${chip({ locale, x: 518, y: 1438, label: c.current, width: locale === "en" ? 108 : 88, height: 38, fill: C.sageSoft, stroke: "#C7DACB", color: C.sageDark, mono: true, size: 18 }).markup}
      ${text({ locale, x: 170, y: 1518, value: c.maintainedPage, size: locale === "en" ? 36 : 34, kind: "heading", weight: 600 })}
      ${text({ locale, x: 170, y: 1552, value: c.pageMeta, size: 20, kind: "mono", weight: 500, fill: C.tertiary })}
      <rect x="170" y="1584" width="276" height="9" rx="4.5" fill="#D9DEE7"/>
      <rect x="170" y="1608" width="304" height="9" rx="4.5" fill="#E3E7EE"/>
      ${chip({ locale, x: 504, y: 1572, label: "source_07", width: 122, height: 38, mono: true, size: 18 }).markup}
      <rect x="158" y="1640" width="480" height="70" rx="8" fill="${C.sageSoft}" stroke="#C7DACB"/>
      ${text({ locale, x: 182, y: 1668, value: c.verified, size: 20, kind: "mono", weight: 500, fill: C.sageDark })}
      <rect x="182" y="1686" width="280" height="8" rx="4" fill="#BFD0C2"/>
      ${chip({ locale, x: 508, y: 1670, label: "mem_77", width: 96, height: 30, fill: C.surface, stroke: "#C7DACB", color: C.sageDark, mono: true, size: 18 }).markup}
      ${text({ locale, x: 170, y: 1746, value: c.prior, size: locale === "en" ? 24 : 25, weight: 600, fill: C.sageDark })}
      <path d="M398 1814 V1830" fill="none" stroke="${C.warm}" stroke-width="1.8" marker-end="url(#${prefix}-warm-arrow)"/>
      <rect x="90" y="1844" width="568" height="86" rx="8" fill="${C.warmSoft}" stroke="#E6C9B8"/>
      ${text({ locale, x: 116, y: 1872, value: c.humanPage, size: 20, kind: "mono", weight: 500, fill: C.warmDark })}
      ${lines({
        locale,
        x: 116,
        y: 1902,
        values: c.mobileHumanLead,
        size: locale === "en" ? 24 : 25,
        lineHeight: 28,
        weight: 600,
        fill: C.secondary,
      })}
    `,
  });
}

function lifecycleFooterMobile(c, locale, prefix) {
  return region({
    id: `${prefix}-footer`,
    x: 40,
    y: 1970,
    width: 640,
    height: 154,
    checkOverlap: false,
    content: `
      ${text({ locale, x: 40, y: 1998, value: c.background, size: 20, kind: "mono", weight: 500, fill: C.tertiary })}
      ${lines({ locale, x: 40, y: 2034, values: c.mobileModelNote, size: locale === "en" ? 24 : 25, lineHeight: 30, fill: C.secondary })}
      ${text({ locale, x: 680, y: 2110, value: c.archive, size: locale === "en" ? 24 : 25, kind: "heading", weight: 600, fill: C.sageDark, anchor: "end" })}
    `,
  });
}

function lifecycleMobile(c, locale, prefix) {
  return `
    ${logo({ x: 40, y: 48, size: 58, prefix })}
    ${region({
      id: `${prefix}-heading`,
      x: 122,
      y: 44,
      width: 558,
      height: 210,
      checkOverlap: false,
      content: `
        ${text({ locale, x: 122, y: 70, value: c.eyebrow, size: 20, kind: "mono", weight: 500, fill: C.tertiary })}
        ${lines({ locale, x: 122, y: 116, values: c.mobileTitle, size: locale === "en" ? 34 : 36, lineHeight: 42, kind: "heading", weight: 600 })}
        ${lines({ locale, x: 122, y: 206, values: c.mobileSubtitle, size: locale === "en" ? 24 : 25, lineHeight: 30, fill: C.secondary })}
      `,
    })}
    ${memoryObjectMobile(c, locale, prefix)}
    ${refineryHubMobile(c, locale, prefix)}
    ${pageObjectMobile(c, locale, prefix)}
    <path d="M470 1080 C630 1080 650 1280 628 1410" fill="none" stroke="${C.sage}" stroke-width="2.4" marker-end="url(#${prefix}-sage-arrow)"/>
    ${networkEdgeLabel({ locale, x: 580, y: 1178, value: c.affectedClaim, size: 20 })}
    ${lifecycleFooterMobile(c, locale, prefix)}
  `;
}

function makeLifecycle(locale, viewport) {
  const c = LIFECYCLE_COPY[locale];
  if (!c) throw new Error(`Unknown lifecycle locale: ${locale}`);
  const { width, height } = VIEWPORTS.lifecycle[viewport];
  const prefix = `lifecycle-${locale}-${viewport}`;
  const body = viewport === "mobile"
    ? lifecycleMobile(c, locale, prefix)
    : lifecycleDesktop(c, locale, prefix);
  const suffix = locale === "en" ? "" : `-${locale}`;
  const name = `wenlan-lifecycle${suffix}${viewport === "mobile" ? "-mobile" : ""}`;
  const svg = `<svg width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" xmlns="http://www.w3.org/2000/svg" role="img" aria-labelledby="title desc">
    <title id="title">${esc(c.title)}</title>
    <desc id="desc">${esc(c.subtitle)}</desc>
    <style>text { font-kerning: normal; }</style>
    <rect width="${width}" height="${height}" fill="${C.paper}"/>
    ${body}
    <defs>
      ${logoDefs(prefix)}
      ${arrowMarker(`${prefix}-arrow`)}
      ${arrowMarker(`${prefix}-warm-arrow`, C.warm)}
      ${arrowMarker(`${prefix}-sage-arrow`, C.sage)}
      <filter id="${prefix}-card-shadow" x="-20%" y="-20%" width="140%" height="150%">
        <feDropShadow dx="0" dy="10" stdDeviation="14" flood-color="#1A1A2E" flood-opacity="0.08"/>
      </filter>
      <filter id="${prefix}-page-shadow" x="-20%" y="-20%" width="140%" height="150%">
        <feDropShadow dx="0" dy="14" stdDeviation="18" flood-color="#1A1A2E" flood-opacity="0.10"/>
        <feDropShadow dx="0" dy="2" stdDeviation="3" flood-color="#1A1A2E" flood-opacity="0.06"/>
      </filter>
    </defs>
  </svg>
`.replace(/[ \t]+$/gmu, "");
  return {
    group: "lifecycle",
    name,
    width,
    height,
    background: C.paper,
    requiredCopy: [
      c.eyebrow,
      c.title,
      c.memoryLabel,
      c.correctedMemory,
      c.supersedes,
      c.sourceChanged,
      c.refinery,
      c.contradiction,
      c.pageLabel,
      c.verified,
      c.humanPage,
      c.background,
      c.archive,
    ],
    svg,
  };
}

module.exports = {
  family,
  makeOverview,
  makeKnowledgeNetwork,
  makeLifecycle,
};
