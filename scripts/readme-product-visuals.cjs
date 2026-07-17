const VIEWPORTS = {
  overview: {
    desktop: { width: 1600, height: 980 },
    mobile: { width: 720, height: 2210 },
  },
  lifecycle: {
    desktop: { width: 1800, height: 1120 },
    mobile: { width: 720, height: 2820 },
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

const FONTS = {
  heading: '"Fraunces", Georgia, serif',
  body: '"Instrument Sans", -apple-system, BlinkMacSystemFont, sans-serif',
  mono: '"JetBrains Mono", ui-monospace, monospace',
  cjkHeading: '"Songti SC", "Songti TC", "STSong", "PingFang SC", "PingFang TC", serif',
  cjkBody: '"PingFang SC", "PingFang TC", "Hiragino Sans GB", sans-serif',
  cjkMono: '"JetBrains Mono", "PingFang SC", "PingFang TC", ui-monospace, monospace',
};

const OVERVIEW_COPY = {
  en: {
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
    sourcesDescription: "Documents / notes / AI conversations",
    sourceTags: ["PDF", "MARKDOWN", "AI CHAT"],
    memories: "Memories",
    memoriesLead: "What ongoing work teaches you",
    memoriesDescription: "Decisions / corrections / context",
    memoryTags: ["DECISION", "LESSON", "CONTEXT"],
    pageLabel: "MAINTAINED PAGE",
    current: "CURRENT",
    pageTitle: "What your work knows now",
    revised: "Last revised 13h ago",
    records: "6 supporting records",
    synthesis: "CURRENT SYNTHESIS",
    linked: "LINKED KNOWLEDGE",
    linkedTags: ["project overview", "decision log", "related research"],
    pageTraits: ["Plain Markdown", "Inspectable citations", "Versioned history"],
    backLabel: "BACK TO YOUR WORK",
    backWords: ["Read it.", "Ask it.", "Reuse it."],
    backSteps: [
      "Open the current Page",
      "Ask from your AI tool",
      "Continue with full context",
    ],
    changed: "Changed support",
    changedLead: "Only the affected claim refreshes.",
    upkeep: "Wenlan handles routine upkeep",
    upkeepLead: "Organize, connect, cite, and refresh in the background.",
    authority: "You keep authority",
    authorityLead: "Conflicts and changes to your writing wait for your judgment.",
  },
  "zh-Hans": {
    eyebrow: "WENLAN 知识系统",
    title: ["你的资料与工作经验，成为", "持续更新的知识库。"],
    mobileTitle: ["你的资料与工作经验，", "成为持续更新的知识库。"],
    sources: "来源",
    sourcesLead: "你已经拥有的材料",
    sourcesDescription: "文档 / 笔记 / AI 对话",
    sourceTags: ["PDF", "MARKDOWN", "AI 对话"],
    memories: "记忆",
    memoriesLead: "工作中值得留下的知识",
    memoriesDescription: "决策 / 修正 / 脉络",
    memoryTags: ["决策", "经验", "脉络"],
    pageLabel: "持续维护的页面",
    current: "当前",
    pageTitle: "你现在掌握的知识",
    revised: "13 小时前更新",
    records: "6 条支撑记录",
    synthesis: "当前结论",
    linked: "相关知识",
    linkedTags: ["项目概览", "决策记录", "相关研究"],
    pageTraits: ["纯 Markdown", "引用可检查", "版本历史"],
    backLabel: "回到你的工作",
    backWords: ["阅读。", "提问。", "继续使用。"],
    backSteps: ["打开当前页面", "从 AI 工具中提问", "带着完整脉络继续"],
    changed: "依据发生变化",
    changedLead: "只更新受影响的结论。",
    upkeep: "日常维护交给 Wenlan",
    upkeepLead: "在后台整理、关联、引用与更新。",
    authority: "你保留决定权",
    authorityLead: "冲突与对你文字的改动，等待你的判断。",
  },
  "zh-Hant": {
    eyebrow: "WENLAN 知識系統",
    title: ["你的資料與工作經驗，成為", "持續更新的知識庫。"],
    mobileTitle: ["你的資料與工作經驗，", "成為持續更新的知識庫。"],
    sources: "來源",
    sourcesLead: "你已經擁有的材料",
    sourcesDescription: "文件 / 筆記 / AI 對話",
    sourceTags: ["PDF", "MARKDOWN", "AI 對話"],
    memories: "記憶",
    memoriesLead: "工作中值得留下的知識",
    memoriesDescription: "決策 / 修正 / 脈絡",
    memoryTags: ["決策", "經驗", "脈絡"],
    pageLabel: "持續維護的頁面",
    current: "目前",
    pageTitle: "你現在掌握的知識",
    revised: "13 小時前更新",
    records: "6 條支撐紀錄",
    synthesis: "目前結論",
    linked: "相關知識",
    linkedTags: ["專案概覽", "決策紀錄", "相關研究"],
    pageTraits: ["純 Markdown", "引用可檢查", "版本歷史"],
    backLabel: "回到你的工作",
    backWords: ["閱讀。", "提問。", "繼續使用。"],
    backSteps: ["開啟目前頁面", "從 AI 工具中提問", "帶著完整脈絡繼續"],
    changed: "依據發生變化",
    changedLead: "只更新受影響的結論。",
    upkeep: "日常維護交給 Wenlan",
    upkeepLead: "在背景整理、關聯、引用與更新。",
    authority: "你保留決定權",
    authorityLead: "衝突與對你文字的改動，等待你的判斷。",
  },
};

const LIFECYCLE_COPY = {
  en: {
    eyebrow: "TWO LINKED LIFECYCLES",
    title: "Knowledge changes. History stays.",
    mobileTitle: ["Knowledge changes.", "History stays."],
    subtitle: "Sources and memories evolve; Wenlan refreshes only the supported parts of each Page.",
    mobileSubtitle: [
      "Sources and memories evolve.",
      "Only supported parts of each Page refresh.",
    ],
    memoryLabel: "MEMORY LIFECYCLE",
    memoryTitle: "Corrected, never erased.",
    earlierMemory: "EARLIER MEMORY",
    earlierState: "LEARNED",
    correctedMemory: "CORRECTED MEMORY",
    correctedState: "CONFIRMED",
    correct: "CORRECT",
    supersedes: "SUPERSEDES",
    oldLinked: "Old claim remains linked",
    enrich: "ENRICH",
    enrichDetail: "facts / confidence",
    connect: "CONNECT",
    connectDetail: "entities / relations",
    sourceChanged: "SOURCE CHANGED",
    memoryCorrected: "MEMORY CORRECTED",
    refinery: "REFINERY",
    maintain: ["Maintain", "only what changed"],
    ring: {
      understand: "UNDERSTAND",
      connect: "CONNECT",
      reconcile: "RECONCILE",
      verify: "VERIFY",
    },
    contradiction: "CONTRADICTION",
    wait: "Wait for your judgment",
    affectedClaim: "AFFECTED CLAIM",
    pageLabel: "PAGE LIFECYCLE",
    pageTitle: "Updated, history preserved.",
    pageVersion: "PAGE v12",
    current: "CURRENT",
    maintainedPage: "Maintained Page",
    pageMeta: "12 versions / 6 supporting records",
    verified: "VERIFIED REVISION",
    prior: "Prior versions remain inspectable",
    versions: "v10 / v11 / v12",
    humanPage: "HUMAN-OWNED PAGE",
    humanLead: "Changes to your prose become a proposed revision.",
    background: "BACKGROUND REFINERY",
    phases: ["Enrich", "Link", "Reconcile", "Verify"],
    runs: "RUNS QUIETLY",
    schedule: "After work / when idle / daily / backstop",
    archive: "Archive, never delete.",
  },
  "zh-Hans": {
    eyebrow: "两套相连的生命周期",
    title: "知识会改变，历史仍会保留。",
    mobileTitle: ["知识会改变，", "历史仍会保留。"],
    subtitle: "来源与记忆持续演进；Wenlan 只更新每个页面中有依据支撑的部分。",
    mobileSubtitle: ["来源与记忆持续演进；", "只更新页面中有依据支撑的部分。"],
    memoryLabel: "记忆生命周期",
    memoryTitle: "被修正，但不会被抹除。",
    earlierMemory: "较早的记忆",
    earlierState: "已学习",
    correctedMemory: "修正后的记忆",
    correctedState: "已确认",
    correct: "更正",
    supersedes: "取代",
    oldLinked: "旧说法仍保留关联",
    enrich: "丰富",
    enrichDetail: "事实 / 可信度",
    connect: "连接",
    connectDetail: "实体 / 关系",
    sourceChanged: "来源已变化",
    memoryCorrected: "记忆已更正",
    refinery: "精炼",
    maintain: ["只维护", "发生变化的部分"],
    ring: {
      understand: "理解",
      connect: "连接",
      reconcile: "校正",
      verify: "验证",
    },
    contradiction: "发现矛盾",
    wait: "等待你的判断",
    affectedClaim: "受影响的结论",
    pageLabel: "页面生命周期",
    pageTitle: "持续更新，历史完整保留。",
    pageVersion: "页面 v12",
    current: "当前",
    maintainedPage: "持续维护的页面",
    pageMeta: "12 个版本 / 6 条支撑记录",
    verified: "已验证的修订",
    prior: "较早版本仍可检查",
    versions: "v10 / v11 / v12",
    humanPage: "人工拥有的页面",
    humanLead: "对你文字的改动会成为修订提案。",
    background: "后台精炼",
    phases: ["丰富", "连接", "校正", "验证"],
    runs: "安静运行",
    schedule: "工作结束后 / 空闲时 / 每日 / 后备检查",
    archive: "只封存，不删除。",
  },
  "zh-Hant": {
    eyebrow: "兩套相連的生命週期",
    title: "知識會改變，歷史仍會保留。",
    mobileTitle: ["知識會改變，", "歷史仍會保留。"],
    subtitle: "來源與記憶持續演進；Wenlan 只更新每個頁面中有依據支撐的部分。",
    mobileSubtitle: ["來源與記憶持續演進；", "只更新頁面中有依據支撐的部分。"],
    memoryLabel: "記憶生命週期",
    memoryTitle: "被修正，但不會被抹除。",
    earlierMemory: "較早的記憶",
    earlierState: "已學習",
    correctedMemory: "修正後的記憶",
    correctedState: "已確認",
    correct: "更正",
    supersedes: "取代",
    oldLinked: "舊說法仍保留關聯",
    enrich: "豐富",
    enrichDetail: "事實 / 可信度",
    connect: "連接",
    connectDetail: "實體 / 關係",
    sourceChanged: "來源已變化",
    memoryCorrected: "記憶已更正",
    refinery: "精煉",
    maintain: ["只維護", "發生變化的部分"],
    ring: {
      understand: "理解",
      connect: "連接",
      reconcile: "校正",
      verify: "驗證",
    },
    contradiction: "發現矛盾",
    wait: "等待你的判斷",
    affectedClaim: "受影響的結論",
    pageLabel: "頁面生命週期",
    pageTitle: "持續更新，歷史完整保留。",
    pageVersion: "頁面 v12",
    current: "目前",
    maintainedPage: "持續維護的頁面",
    pageMeta: "12 個版本 / 6 條支撐紀錄",
    verified: "已驗證的修訂",
    prior: "較早版本仍可檢查",
    versions: "v10 / v11 / v12",
    humanPage: "人工擁有的頁面",
    humanLead: "對你文字的改動會成為修訂提案。",
    background: "背景精煉",
    phases: ["豐富", "連接", "校正", "驗證"],
    runs: "安靜執行",
    schedule: "工作結束後 / 閒置時 / 每日 / 後備檢查",
    archive: "只封存，不刪除。",
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
  if (locale === "en") return FONTS[kind];
  if (kind === "heading") return FONTS.cjkHeading;
  if (kind === "mono") return FONTS.cjkMono;
  return FONTS.cjkBody;
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
  return `<g fill="none" stroke="${color}" stroke-width="1.7" stroke-linecap="round">
    <path d="M${x + 3} ${y + 6}c0-3 2-5 5-5 2 0 4 1 5 3 1-2 3-3 5-3 3 0 5 2 5 5v14c0 3-2 5-5 5H${x + 8}c-3 0-5-2-5-5z"/>
    <path d="M${x + 8} ${y + 8}h10M${x + 8} ${y + 13}h10M${x + 8} ${y + 18}h7"/>
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
  const labelSize = mobile ? 12 : 13;
  const chipHeight = mobile ? 30 : 30;
  const currentChip = chip({
    locale,
    x: x + width - (mobile ? 126 : 142),
    y: y + 24,
    label: c.current,
    width: mobile ? 94 : 102,
    height: chipHeight,
    fill: C.sageSoft,
    stroke: "#C7DACB",
    color: C.sageDark,
    mono: true,
    size: mobile ? 12 : 13,
  }).markup;
  const linkedRow = chipRow({
    locale,
    x: x + pad,
    y: y + (mobile ? 505 : 408),
    labels: c.linkedTags,
    gap: mobile ? 8 : 12,
    height: mobile ? 32 : 34,
    size: mobile ? 13 : 14,
  }).markup;
  const citationOne = chip({
    locale,
    x: x + width - (mobile ? 142 : 206),
    y: y + (mobile ? 300 : 228),
    label: "source_07",
    width: 106,
    height: 30,
    mono: true,
  }).markup;
  const citationMemory = chip({
    locale,
    x: x + width - (mobile ? 142 : 92),
    y: y + (mobile ? 342 : 228),
    label: "mem_42",
    width: 84,
    height: 30,
    fill: C.indigoSoft,
    stroke: "#D2CFF0",
    color: C.indigo,
    mono: true,
  }).markup;
  const citationTwo = chip({
    locale,
    x: x + width - (mobile ? 142 : 186),
    y: y + (mobile ? 407 : 296),
    label: "source_11",
    width: 106,
    height: 30,
    mono: true,
  }).markup;
  const traitsY = y + height - (mobile ? 42 : 36);
  const traits = mobile
    ? dotSeparated({
      locale,
      x: x + pad,
      y: traitsY,
      labels: c.pageTraits,
      size: locale === "en" ? 13 : 14,
      gap: 10,
    })
    : dotSeparated({
      locale,
      x: x + pad,
      y: traitsY,
      labels: c.pageTraits,
      size: 15,
      gap: 12,
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
      ${text({ locale, x: x + pad, y: y + (mobile ? 154 : 140), value: c.revised, size: mobile ? 13 : 14, kind: "mono", weight: 500, fill: C.tertiary })}
      <circle cx="${x + pad + (mobile ? 156 : 178)}" cy="${y + (mobile ? 149 : 135)}" r="2.5" fill="${C.border}"/>
      ${text({ locale, x: x + pad + (mobile ? 170 : 190), y: y + (mobile ? 154 : 140), value: c.records, size: mobile ? 13 : 14, kind: "mono", weight: 500, fill: C.tertiary })}
      <line x1="${x + pad}" y1="${y + (mobile ? 188 : 174)}" x2="${x + width - pad}" y2="${y + (mobile ? 188 : 174)}" stroke="${C.border}"/>
      ${text({ locale, x: x + pad, y: y + (mobile ? 228 : 210), value: c.synthesis, size: mobile ? 11 : 12, kind: "mono", weight: 500, fill: C.tertiary })}
      <rect x="${x + pad}" y="${y + (mobile ? 258 : 237)}" width="${mobile ? 280 : 320}" height="9" rx="4.5" fill="#DDE2EA"/>
      <rect x="${x + pad}" y="${y + (mobile ? 280 : 258)}" width="${mobile ? 338 : 382}" height="9" rx="4.5" fill="#E7EAF0"/>
      ${citationOne}
      ${citationMemory}
      <rect x="${x + pad}" y="${y + (mobile ? 365 : 305)}" width="${mobile ? 348 : 402}" height="9" rx="4.5" fill="#DDE2EA"/>
      <rect x="${x + pad}" y="${y + (mobile ? 387 : 326)}" width="${mobile ? 252 : 278}" height="9" rx="4.5" fill="#E7EAF0"/>
      ${citationTwo}
      <circle cx="${x + width - (mobile ? 88 : 56)}" cy="${y + (mobile ? 422 : 311)}" r="6" fill="${C.amber}"/>
      <line x1="${x + width - (mobile ? 88 : 56)}" y1="${y + (mobile ? 430 : 319)}" x2="${x + width - (mobile ? 88 : 56)}" y2="${y + (mobile ? 454 : 344)}" stroke="${C.amber}" stroke-width="1.5"/>
      ${text({ locale, x: x + pad, y: y + (mobile ? 488 : 388), value: c.linked, size: mobile ? 11 : 12, kind: "mono", weight: 500, fill: C.tertiary })}
      ${linkedRow}
      <line x1="${x + pad}" y1="${y + height - (mobile ? 78 : 70)}" x2="${x + width - pad}" y2="${y + height - (mobile ? 78 : 70)}" stroke="${C.border}"/>
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
    y: 246,
    width: 650,
    height: 544,
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
      <line x1="1208" y1="522" x2="1534" y2="522" stroke="${C.border}"/>
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
      <line x1="64" y1="842" x2="1536" y2="842" stroke="${C.border}"/>
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
        <path d="M1064 590 C1064 678 1110 710 1142 716" fill="none" stroke="${C.amber}" stroke-width="1.5"/>
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
    y: y + 180,
    labels: tagLines[0],
    gap: 8,
    height: 30,
    mono: true,
    size: 12,
  }).markup;
  const secondRow = tagLines[1]
    ? chipRow({
      locale,
      x: x + 24,
      y: y + 220,
      labels: tagLines[1],
      gap: 8,
      height: 30,
      mono: true,
      size: 12,
    }).markup
    : "";
  return region({
    id,
    x,
    y,
    width: 304,
    height: 270,
    content: `
      <rect x="${x}" y="${y}" width="304" height="270" rx="8" fill="${C.surface}" stroke="${C.border}"/>
      <line x1="${x}" y1="${y + 18}" x2="${x}" y2="${y + 252}" stroke="${C.indigo}" stroke-width="4"/>
      ${glyph({ x: x + 24, y: y + 26 })}
      ${text({ locale, x: x + 64, y: y + 48, value: title, size: locale === "en" ? 27 : 29, kind: "heading", weight: 600 })}
      ${text({ locale, x: x + 24, y: y + 98, value: lead, size: locale === "en" ? 16 : 17, weight: 600, fill: C.secondary })}
      ${text({ locale, x: x + 24, y: y + 134, value: description, size: locale === "en" ? 14 : 16, fill: C.secondary })}
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
    y: 630,
    width: 640,
    height: 720,
    prefix,
    mobile: true,
  });
  const sources = mobileInput({
    locale,
    id: `${prefix}-sources`,
    x: 40,
    y: 280,
    title: c.sources,
    lead: c.sourcesLead,
    description: c.sourcesDescription,
    tags: c.sourceTags,
    glyph: documentGlyph,
  });
  const memories = mobileInput({
    locale,
    id: `${prefix}-memories`,
    x: 376,
    y: 280,
    title: c.memories,
    lead: c.memoriesLead,
    description: c.memoriesDescription,
    tags: c.memoryTags,
    glyph: memoryGlyph,
  });
  const outputWords = c.backWords.map((word, index) => {
    const x = 142 + index * 220;
    return `${text({
      locale,
      x,
      y: 1608,
      value: word,
      size: locale === "en" ? 28 : 30,
      kind: "heading",
      weight: 600,
      anchor: "middle",
    })}
    <circle cx="${x}" cy="1652" r="13" fill="${C.indigoSoft}" stroke="#D2CFF0"/>
    ${text({ locale, x, y: 1657, value: String(index + 1), size: 13, kind: "mono", weight: 500, fill: C.indigo, anchor: "middle" })}
    ${lines({
      locale,
      x,
      y: 1690,
      values: locale === "en"
        ? (index === 2 ? ["Continue with", "full context"] : c.backSteps[index].split(index === 0 ? " current " : " your "))
        : [c.backSteps[index]],
      size: locale === "en" ? 14 : 16,
      lineHeight: 20,
      weight: 600,
      fill: C.secondary,
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
        ${text({ locale, x: 122, y: 70, value: c.eyebrow, size: 12, kind: "mono", weight: 500, fill: C.tertiary })}
        ${lines({ locale, x: 122, y: 112, values: c.mobileTitle, size: locale === "en" ? 30 : 34, lineHeight: locale === "en" ? 36 : 43, kind: "heading", weight: 600 })}
      `,
    })}
    ${sources}
    ${memories}
    <path d="M192 550 C192 586 284 590 330 616" fill="none" stroke="${C.indigo}" stroke-width="2.2" marker-end="url(#${marker})"/>
    <path d="M528 550 C528 586 436 590 390 616" fill="none" stroke="${C.indigo}" stroke-width="2.2" marker-end="url(#${marker})"/>
    ${page}
    ${region({
      id: `${prefix}-changed-support`,
      x: 254,
      y: 1375,
      width: 426,
      height: 72,
      content: `
        <path d="M592 1052 C650 1150 652 1320 632 1377" fill="none" stroke="${C.amber}" stroke-width="1.5"/>
        ${text({ locale, x: 280, y: 1400, value: c.changed, size: 12, kind: "mono", weight: 500, fill: C.amberDark })}
        ${text({ locale, x: 280, y: 1432, value: c.changedLead, size: locale === "en" ? 16 : 17, weight: 600, fill: C.secondary })}
      `,
    })}
    ${region({
      id: `${prefix}-back-to-work`,
      x: 40,
      y: 1500,
      width: 640,
      height: 270,
      content: `
        <line x1="40" y1="1500" x2="680" y2="1500" stroke="${C.border}"/>
        ${text({ locale, x: 40, y: 1542, value: c.backLabel, size: 12, kind: "mono", weight: 500, fill: C.tertiary })}
        ${outputWords}
      `,
    })}
    ${region({
      id: `${prefix}-authority`,
      x: 40,
      y: 1840,
      width: 640,
      height: 300,
      content: `
        <line x1="40" y1="1840" x2="680" y2="1840" stroke="${C.border}"/>
        <circle cx="54" cy="1894" r="7" fill="${C.sage}"/>
        ${text({ locale, x: 78, y: 1892, value: c.upkeep, size: locale === "en" ? 23 : 25, kind: "heading", weight: 600 })}
        ${lines({
          locale,
          x: 78,
          y: 1930,
          values: locale === "en" ? ["Organize, connect, cite, and refresh", "in the background."] : [c.upkeepLead],
          size: locale === "en" ? 16 : 17,
          lineHeight: 25,
          fill: C.secondary,
        })}
        <line x1="78" y1="1995" x2="642" y2="1995" stroke="${C.border}"/>
        <circle cx="54" cy="2050" r="7" fill="${C.warm}"/>
        ${text({ locale, x: 78, y: 2048, value: c.authority, size: locale === "en" ? 23 : 25, kind: "heading", weight: 600 })}
        ${lines({
          locale,
          x: 78,
          y: 2086,
          values: locale === "en"
            ? ["Conflicts and changes to your writing", "wait for your judgment."]
            : [c.authorityLead],
          size: locale === "en" ? 16 : 17,
          lineHeight: 25,
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
    <desc id="desc">${esc(`${c.sources} and ${c.memories} independently support a maintained Page.`)}</desc>
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
      <line x1="110" y1="642" x2="188" y2="642" stroke="${C.border}"/>
      ${text({ locale, x: 88, y: 674, value: c.enrich, size: 11, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 88, y: 696, value: c.enrichDetail, size: locale === "en" ? 14 : 15, weight: 600, fill: C.secondary })}
      <circle cx="102" cy="728" r="5" fill="${C.indigo}"/>
      <line x1="110" y1="728" x2="188" y2="728" stroke="${C.border}"/>
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
      ${text({ locale, x: 878, y: 440, value: c.ring.understand, size: 10, kind: "mono", weight: 500, fill: C.tertiary, anchor: "middle" })}
      ${text({ locale, x: 1000, y: 584, value: c.ring.connect, size: 10, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 878, y: 726, value: c.ring.reconcile, size: 10, kind: "mono", weight: 500, fill: C.tertiary, anchor: "middle" })}
      ${text({ locale, x: 738, y: 584, value: c.ring.verify, size: 10, kind: "mono", weight: 500, fill: C.tertiary, anchor: "end" })}
      <path d="M680 728 C728 728 744 660 780 622" fill="none" stroke="${C.indigo}" stroke-width="2" marker-end="url(#${prefix}-arrow)"/>
      ${text({ locale, x: 704, y: 710, value: c.memoryCorrected, size: 10, kind: "mono", weight: 500, fill: C.indigo })}
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
      ${text({ locale, x: 1164, y: 566, value: "STALE", size: 9, kind: "mono", weight: 500, fill: C.amberDark, anchor: "middle" })}
      <g filter="url(#${prefix}-page-shadow)">
        <rect x="1192" y="330" width="510" height="454" rx="8" fill="${C.surface}" stroke="${C.border}"/>
      </g>
      ${text({ locale, x: 1224, y: 370, value: c.pageVersion, size: 12, kind: "mono", weight: 500, fill: C.tertiary })}
      ${chip({ locale, x: 1568, y: 350, label: c.current, width: locale === "en" ? 102 : 82, height: 28, fill: C.sageSoft, stroke: "#C7DACB", color: C.sageDark, mono: true, size: 12 }).markup}
      ${text({ locale, x: 1224, y: 424, value: c.maintainedPage, size: locale === "en" ? 36 : 34, kind: "heading", weight: 600 })}
      ${text({ locale, x: 1224, y: 454, value: c.pageMeta, size: 13, kind: "mono", weight: 500, fill: C.tertiary })}
      <line x1="1224" y1="482" x2="1670" y2="482" stroke="${C.border}"/>
      <rect x="1224" y="514" width="280" height="9" rx="4.5" fill="#D9DEE7"/>
      <rect x="1224" y="536" width="346" height="9" rx="4.5" fill="#E3E7EE"/>
      ${chip({ locale, x: 1580, y: 505, label: "source_07", width: 108, height: 30, mono: true }).markup}
      <rect x="1212" y="574" width="470" height="88" rx="8" fill="${C.sageSoft}" stroke="#C7DACB"/>
      ${text({ locale, x: 1236, y: 602, value: c.verified, size: 11, kind: "mono", weight: 500, fill: C.sageDark })}
      <rect x="1236" y="620" width="250" height="8" rx="4" fill="#BFD0C2"/>
      <rect x="1236" y="640" width="322" height="8" rx="4" fill="#D6E2D8"/>
      ${chip({ locale, x: 1570, y: 606, label: "mem_77", width: 88, height: 30, fill: C.surface, stroke: "#C7DACB", color: C.sageDark, mono: true }).markup}
      <line x1="1224" y1="702" x2="1670" y2="702" stroke="${C.border}"/>
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
  const phaseMarkup = dotSeparated({
    locale,
    x: 88,
    y: 1043,
    labels: c.phases,
    size: 18,
    kind: "heading",
    weight: 600,
    fill: C.ink,
    gap: 14,
  });
  return region({
    id: `${prefix}-footer`,
    x: 74,
    y: 966,
    width: 1652,
    height: 106,
    checkOverlap: false,
    content: `
      <line x1="74" y1="966" x2="1726" y2="966" stroke="${C.border}"/>
      ${text({ locale, x: 88, y: 1007, value: c.background, size: 13, kind: "mono", weight: 500, fill: C.tertiary })}
      ${phaseMarkup}
      ${text({ locale, x: 724, y: 1007, value: c.runs, size: 13, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 724, y: 1043, value: c.schedule, size: locale === "en" ? 17 : 18, weight: 600, fill: C.secondary })}
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
    ${text({ locale, x: 1094, y: 532, value: c.affectedClaim, size: 10, kind: "mono", weight: 500, fill: C.sageDark, anchor: "middle" })}
    ${lifecycleFooterDesktop(c, locale, prefix)}
  `;
}

function memoryObjectMobile(c, locale, prefix) {
  return region({
    id: `${prefix}-memory-object`,
    x: 40,
    y: 286,
    width: 640,
    height: 800,
    checkOverlap: false,
    content: `
      ${text({ locale, x: 40, y: 318, value: c.memoryLabel, size: 12, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 40, y: 362, value: c.memoryTitle, size: locale === "en" ? 29 : 31, kind: "heading", weight: 600 })}
      <rect x="74" y="408" width="500" height="222" rx="8" fill="${C.raised}" stroke="${C.border}"/>
      ${text({ locale, x: 104, y: 448, value: c.earlierMemory, size: 11, kind: "mono", weight: 500, fill: C.tertiary })}
      ${chip({ locale, x: 434, y: 428, label: c.earlierState, width: locale === "en" ? 104 : 92, height: 28, mono: true, size: 11 }).markup}
      ${text({ locale, x: 104, y: 500, value: "mem_42", size: 28, kind: "heading", weight: 600 })}
      <rect x="104" y="532" width="270" height="9" rx="4.5" fill="#D9DEE7"/>
      <rect x="104" y="555" width="372" height="9" rx="4.5" fill="#E3E7EE"/>
      ${chip({ locale, x: 104, y: 582, label: "source_07", width: 108, height: 30, fill: C.surface, mono: true }).markup}
      <path d="M494 612 C548 636 554 662 554 690" fill="none" stroke="${C.indigo}" stroke-width="2" marker-end="url(#${prefix}-arrow)"/>
      ${text({ locale, x: 506, y: 650, value: c.correct, size: 11, kind: "mono", weight: 500, fill: C.indigo })}
      <g filter="url(#${prefix}-card-shadow)">
        <rect x="126" y="684" width="540" height="270" rx="8" fill="${C.surface}" stroke="${C.border}"/>
      </g>
      ${text({ locale, x: 158, y: 728, value: c.correctedMemory, size: 11, kind: "mono", weight: 500, fill: C.tertiary })}
      ${chip({ locale, x: 518, y: 708, label: c.correctedState, width: locale === "en" ? 112 : 98, height: 28, fill: C.sageSoft, stroke: "#C7DACB", color: C.sageDark, mono: true, size: 11 }).markup}
      ${text({ locale, x: 158, y: 782, value: "mem_77", size: 30, kind: "heading", weight: 600 })}
      <rect x="158" y="815" width="294" height="9" rx="4.5" fill="#D9DEE7"/>
      <rect x="158" y="838" width="392" height="9" rx="4.5" fill="#E3E7EE"/>
      ${text({ locale, x: 158, y: 884, value: c.supersedes, size: 11, kind: "mono", weight: 500, fill: C.indigo })}
      <path d="M266 880 H354" stroke="${C.indigo}" stroke-width="1.6" marker-end="url(#${prefix}-arrow)"/>
      ${chip({ locale, x: 372, y: 864, label: "mem_42", width: 88, height: 30, fill: C.indigoSoft, stroke: "#D2CFF0", color: C.indigo, mono: true }).markup}
      ${text({ locale, x: 158, y: 928, value: c.oldLinked, size: locale === "en" ? 15 : 16, weight: 600, fill: C.sageDark })}
      <line x1="58" y1="714" x2="104" y2="714" stroke="${C.border}"/>
      <circle cx="52" cy="714" r="5" fill="${C.indigo}"/>
      ${text({ locale, x: 40, y: 752, value: c.enrich, size: 10, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 40, y: 776, value: c.enrichDetail, size: locale === "en" ? 13 : 14, weight: 600, fill: C.secondary })}
      <line x1="58" y1="820" x2="104" y2="820" stroke="${C.border}"/>
      <circle cx="52" cy="820" r="5" fill="${C.indigo}"/>
      ${text({ locale, x: 40, y: 858, value: c.connect, size: 10, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 40, y: 882, value: c.connectDetail, size: locale === "en" ? 13 : 14, weight: 600, fill: C.secondary })}
    `,
  });
}

function refineryHubMobile(c, locale, prefix) {
  return region({
    id: `${prefix}-refinery`,
    x: 40,
    y: 1090,
    width: 640,
    height: 630,
    checkOverlap: false,
    content: `
      <line x1="40" y1="1090" x2="680" y2="1090" stroke="${C.border}"/>
      <rect x="214" y="1142" width="292" height="94" rx="8" fill="${C.raised}" stroke="${C.border}"/>
      ${documentGlyph({ x: 240, y: 1174, color: C.secondary })}
      ${text({ locale, x: 282, y: 1178, value: c.sourceChanged, size: 11, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 282, y: 1210, value: "source_11", size: 20, kind: "heading", weight: 600 })}
      <path d="M360 1236 V1300" fill="none" stroke="${C.indigo}" stroke-width="2" marker-end="url(#${prefix}-arrow)"/>
      <path d="M310 1092 C310 1150 248 1250 250 1352" fill="none" stroke="${C.indigo}" stroke-width="1.8" marker-end="url(#${prefix}-arrow)"/>
      ${text({ locale, x: 86, y: 1274, value: c.memoryCorrected, size: 10, kind: "mono", weight: 500, fill: C.indigo })}
      <circle cx="360" cy="1420" r="126" fill="${C.surface}" stroke="${C.border}"/>
      <circle cx="360" cy="1420" r="98" fill="none" stroke="${C.indigo}" stroke-width="3" stroke-dasharray="122 30"/>
      <circle cx="360" cy="1420" r="64" fill="${C.indigoSoft}" stroke="#D2CFF0"/>
      ${text({ locale, x: 360, y: 1408, value: c.refinery, size: 12, kind: "mono", weight: 500, fill: C.indigo, anchor: "middle" })}
      ${text({ locale, x: 360, y: 1438, value: c.maintain[0], size: locale === "en" ? 22 : 24, kind: "heading", weight: 600, anchor: "middle" })}
      ${text({ locale, x: 360, y: 1466, value: c.maintain[1], size: locale === "en" ? 14 : 15, weight: 600, fill: C.secondary, anchor: "middle" })}
      <circle cx="360" cy="1308" r="7" fill="${C.indigo}"/>
      <circle cx="464" cy="1420" r="7" fill="${C.indigo}"/>
      <circle cx="360" cy="1532" r="7" fill="${C.indigo}"/>
      <circle cx="256" cy="1420" r="7" fill="${C.indigo}"/>
      ${text({ locale, x: 360, y: 1274, value: c.ring.understand, size: 10, kind: "mono", weight: 500, fill: C.tertiary, anchor: "middle" })}
      ${text({ locale, x: 496, y: 1426, value: c.ring.connect, size: 10, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 360, y: 1574, value: c.ring.reconcile, size: 10, kind: "mono", weight: 500, fill: C.tertiary, anchor: "middle" })}
      ${text({ locale, x: 222, y: 1426, value: c.ring.verify, size: 10, kind: "mono", weight: 500, fill: C.tertiary, anchor: "end" })}
      <path d="M360 1546 V1606" fill="none" stroke="${C.warm}" stroke-width="1.8" marker-end="url(#${prefix}-warm-arrow)"/>
      <rect x="170" y="1620" width="380" height="72" rx="8" fill="${C.warmSoft}" stroke="#E6C9B8"/>
      ${text({ locale, x: 360, y: 1648, value: c.contradiction, size: 11, kind: "mono", weight: 500, fill: C.warmDark, anchor: "middle" })}
      ${text({ locale, x: 360, y: 1678, value: c.wait, size: locale === "en" ? 17 : 18, kind: "heading", weight: 600, anchor: "middle" })}
    `,
  });
}

function pageObjectMobile(c, locale, prefix) {
  return region({
    id: `${prefix}-page-object`,
    x: 40,
    y: 1750,
    width: 640,
    height: 760,
    checkOverlap: false,
    content: `
      <line x1="40" y1="1750" x2="680" y2="1750" stroke="${C.border}"/>
      ${text({ locale, x: 40, y: 1794, value: c.pageLabel, size: 12, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 40, y: 1842, value: c.pageTitle, size: locale === "en" ? 29 : 31, kind: "heading", weight: 600 })}
      <rect x="92" y="1928" width="520" height="454" rx="8" fill="${C.raised}" stroke="${C.border}"/>
      <rect x="116" y="1898" width="520" height="464" rx="8" fill="#FAFAFA" stroke="${C.border}"/>
      <rect x="70" y="2048" width="58" height="120" rx="8" fill="${C.amberSoft}" stroke="#E5D3AA"/>
      ${text({ locale, x: 99, y: 2092, value: "v11", size: 10, kind: "mono", weight: 500, fill: C.amberDark, anchor: "middle" })}
      ${text({ locale, x: 99, y: 2118, value: "STALE", size: 9, kind: "mono", weight: 500, fill: C.amberDark, anchor: "middle" })}
      <g filter="url(#${prefix}-page-shadow)">
        <rect x="138" y="1870" width="520" height="470" rx="8" fill="${C.surface}" stroke="${C.border}"/>
      </g>
      ${text({ locale, x: 170, y: 1912, value: c.pageVersion, size: 11, kind: "mono", weight: 500, fill: C.tertiary })}
      ${chip({ locale, x: 526, y: 1890, label: c.current, width: locale === "en" ? 100 : 82, height: 28, fill: C.sageSoft, stroke: "#C7DACB", color: C.sageDark, mono: true, size: 11 }).markup}
      ${text({ locale, x: 170, y: 1970, value: c.maintainedPage, size: locale === "en" ? 34 : 32, kind: "heading", weight: 600 })}
      ${text({ locale, x: 170, y: 2004, value: c.pageMeta, size: 12, kind: "mono", weight: 500, fill: C.tertiary })}
      <line x1="170" y1="2032" x2="626" y2="2032" stroke="${C.border}"/>
      <rect x="170" y="2064" width="276" height="9" rx="4.5" fill="#D9DEE7"/>
      <rect x="170" y="2088" width="338" height="9" rx="4.5" fill="#E3E7EE"/>
      ${chip({ locale, x: 516, y: 2054, label: "source_07", width: 108, height: 30, mono: true }).markup}
      <rect x="158" y="2130" width="480" height="94" rx="8" fill="${C.sageSoft}" stroke="#C7DACB"/>
      ${text({ locale, x: 182, y: 2160, value: c.verified, size: 11, kind: "mono", weight: 500, fill: C.sageDark })}
      <rect x="182" y="2180" width="250" height="8" rx="4" fill="#BFD0C2"/>
      <rect x="182" y="2202" width="316" height="8" rx="4" fill="#D6E2D8"/>
      ${chip({ locale, x: 516, y: 2167, label: "mem_77", width: 88, height: 30, fill: C.surface, stroke: "#C7DACB", color: C.sageDark, mono: true }).markup}
      <line x1="170" y1="2264" x2="626" y2="2264" stroke="${C.border}"/>
      ${text({ locale, x: 170, y: 2300, value: c.prior, size: locale === "en" ? 15 : 16, weight: 600, fill: C.sageDark })}
      ${text({ locale, x: 612, y: 2324, value: c.versions, size: 12, kind: "mono", weight: 500, fill: C.tertiary, anchor: "end" })}
      <path d="M398 2340 V2400" fill="none" stroke="${C.warm}" stroke-width="1.8" marker-end="url(#${prefix}-warm-arrow)"/>
      <rect x="90" y="2414" width="568" height="76" rx="8" fill="${C.warmSoft}" stroke="#E6C9B8"/>
      ${text({ locale, x: 116, y: 2444, value: c.humanPage, size: 11, kind: "mono", weight: 500, fill: C.warmDark })}
      ${text({ locale, x: 116, y: 2476, value: c.humanLead, size: locale === "en" ? 15 : 16, weight: 600, fill: C.secondary })}
    `,
  });
}

function lifecycleFooterMobile(c, locale, prefix) {
  const phases = dotSeparated({
    locale,
    x: 40,
    y: 2622,
    labels: c.phases,
    size: locale === "en" ? 18 : 19,
    kind: "heading",
    weight: 600,
    fill: C.ink,
    gap: 10,
  });
  return region({
    id: `${prefix}-footer`,
    x: 40,
    y: 2550,
    width: 640,
    height: 220,
    checkOverlap: false,
    content: `
      <line x1="40" y1="2550" x2="680" y2="2550" stroke="${C.border}"/>
      ${text({ locale, x: 40, y: 2590, value: c.background, size: 12, kind: "mono", weight: 500, fill: C.tertiary })}
      ${phases}
      ${text({ locale, x: 40, y: 2682, value: c.runs, size: 12, kind: "mono", weight: 500, fill: C.tertiary })}
      ${text({ locale, x: 40, y: 2716, value: c.schedule, size: locale === "en" ? 16 : 17, weight: 600, fill: C.secondary })}
      ${text({ locale, x: 680, y: 2760, value: c.archive, size: locale === "en" ? 17 : 18, kind: "heading", weight: 600, fill: C.sageDark, anchor: "end" })}
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
        ${text({ locale, x: 122, y: 70, value: c.eyebrow, size: 12, kind: "mono", weight: 500, fill: C.tertiary })}
        ${lines({ locale, x: 122, y: 116, values: c.mobileTitle, size: locale === "en" ? 34 : 36, lineHeight: 42, kind: "heading", weight: 600 })}
        ${lines({ locale, x: 122, y: 206, values: c.mobileSubtitle, size: locale === "en" ? 15 : 16, lineHeight: 24, fill: C.secondary })}
      `,
    })}
    ${memoryObjectMobile(c, locale, prefix)}
    ${refineryHubMobile(c, locale, prefix)}
    ${pageObjectMobile(c, locale, prefix)}
    <path d="M486 1420 C620 1420 642 1700 628 1866" fill="none" stroke="${C.sage}" stroke-width="2.4" marker-end="url(#${prefix}-sage-arrow)"/>
    ${text({ locale, x: 596, y: 1584, value: c.affectedClaim, size: 10, kind: "mono", weight: 500, fill: C.sageDark, anchor: "middle" })}
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
  makeOverview,
  makeLifecycle,
};
