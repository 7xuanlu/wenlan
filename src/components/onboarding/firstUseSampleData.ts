// SPDX-License-Identifier: AGPL-3.0-only
//
// Deliberate built-in tutorial data for the FirstUseGuide sample walkthrough.
// This is a product sample feature, not fixture leakage: it is never imported
// from the preview harness, never persisted, never sent to the daemon, and the
// sample-only path issues no read/write daemon calls (clipboard is the only
// exception, and it stays on-device). The tutorial prose is localized per app
// locale via getSampleData; UI chrome around it uses translated copy.
//
// Invariant (enforced by firstUseSampleData.test.ts): every citation quote is
// an exact substring of its source's body, in every locale.

export type SampleSourceType = "memory" | "file" | "page";

export interface SampleSource {
  id: string;
  type: SampleSourceType;
  /** Short kind tag shown beside the title (part of the tutorial data). */
  kind: string;
  title: string;
  location: string;
  excerpt: string;
  body: string[];
  /** File sources carry the on-disk origin of the quoted line. */
  path?: string;
  /** 1-based line number of the quoted line inside `body`. */
  quoteLine?: number;
}

export interface SampleCitation {
  id: string;
  sourceId: string;
  quote: string;
}

export interface SamplePageParagraph {
  text: string;
  citationIds: string[];
}

export interface SamplePage {
  title: string;
  subtitle: string;
  paragraphs: SamplePageParagraph[];
  related: {
    title: string;
    description: string;
    body: string;
  };
}

export type SampleClient = "chatgpt" | "codex" | "claude";

export interface SampleCommands {
  /** Recall works on the user's OWN imported data — never on this sample,
   *  which lives only in the walkthrough and no AI can retrieve. */
  recall: string;
  handoff: string;
  brief: string;
}

export interface SampleDataset {
  sources: SampleSource[];
  citations: SampleCitation[];
  page: SamplePage;
  commands: Record<SampleClient, SampleCommands>;
}

export type SampleLocale = "en" | "zh-Hans" | "zh-Hant";

/** Map an i18next language tag onto the closest tutorial dataset. */
export function resolveSampleLocale(language: string | undefined): SampleLocale {
  const tag = (language ?? "").toLowerCase().replace("_", "-");
  if (tag === "zh-hant" || tag.startsWith("zh-hant-") || tag === "zh-tw" || tag === "zh-hk" || tag === "zh-mo") {
    return "zh-Hant";
  }
  if (tag === "zh" || tag.startsWith("zh-")) {
    return "zh-Hans";
  }
  return "en";
}

const enData: SampleDataset = {
  sources: [
    {
      id: "sample-memory",
      type: "memory",
      kind: "Memory",
      title: "Weekend trip decision",
      location: "Conversation excerpts · weekend trip planning",
      excerpt: "Take the train to Tainan, leaving Saturday morning.",
      body: [
        "We want a slow weekend trip with no packed schedule.",
        "We decided to take the train to Tainan, leaving Saturday morning.",
        "Keep the afternoon open for wandering — no hour-by-hour plan.",
      ],
    },
    {
      id: "sample-file",
      type: "file",
      kind: "File",
      title: "packing-list.md",
      location: "trip folder / packing-list.md",
      excerpt: "Book a stay near the Tainan station and pack light.",
      body: [
        "No driving this time — keep getting around simple.",
        "Book a stay near the Tainan station and pack light.",
        "Rooms and tickets aren't booked yet; confirm before we leave.",
      ],
      path: "trip folder / packing-list.md",
      quoteLine: 2,
    },
    {
      id: "sample-page",
      type: "page",
      kind: "Page",
      title: "Slow travel",
      location: "Wenlan / travel ideas / slow travel",
      excerpt: "On short trips, anchor on one neighborhood and leave some hours unplanned.",
      body: [
        "An existing sample note about a favorite way to travel.",
        "On short trips, anchor on one neighborhood and leave some hours unplanned.",
        "Wandering into small shops along the way can become the memory of the trip.",
      ],
    },
  ],
  citations: [
    {
      id: "sample-cite-1",
      sourceId: "sample-memory",
      quote: "We decided to take the train to Tainan, leaving Saturday morning.",
    },
    {
      id: "sample-cite-2",
      sourceId: "sample-file",
      quote: "Book a stay near the Tainan station and pack light.",
    },
    {
      id: "sample-cite-3",
      sourceId: "sample-page",
      quote: "On short trips, anchor on one neighborhood and leave some hours unplanned.",
    },
    {
      id: "sample-cite-4",
      sourceId: "sample-memory",
      quote: "Keep the afternoon open for wandering — no hour-by-hour plan.",
    },
    {
      id: "sample-cite-5",
      sourceId: "sample-file",
      quote: "Rooms and tickets aren't booked yet; confirm before we leave.",
    },
  ],
  page: {
    title: "An unhurried Tainan weekend",
    subtitle: "From a conversation decision to a departure-ready plan.",
    paragraphs: [
      {
        text: "Transit is decided: take the train to Tainan, leaving Saturday morning.",
        citationIds: ["sample-cite-1"],
      },
      {
        text: "Stay near the station and pack light to keep moving simple.",
        citationIds: ["sample-cite-2"],
      },
      {
        text: "Wander one neighborhood and keep unscheduled hours — echoing the decision not to pack the afternoon.",
        citationIds: ["sample-cite-4", "sample-cite-3"],
      },
    ],
    related: {
      title: "Two things left before departure",
      description: "Connecting decided plans to unfinished prep.",
      body: "Rooms and tickets still aren't booked. Compare stays near the station first, then confirm Saturday morning trains.",
    },
  },
  commands: {
    chatgpt: {
      recall: "@wenlan recall the decisions and next steps from my imported notes, and list the sources.",
      handoff: "@wenlan note today's decisions and progress so I can pick this up later.",
      brief: "@wenlan this is a new conversation — catch me up on earlier work and open todos.",
    },
    codex: {
      recall: "/recall decisions and next steps from my imports",
      handoff: "/handoff",
      brief: "/brief",
    },
    claude: {
      recall: "/recall decisions and next steps from my imports",
      handoff: "/handoff",
      brief: "/brief",
    },
  },
};

const hansData: SampleDataset = {
  sources: [
    {
      id: "sample-memory",
      type: "memory",
      kind: "记忆",
      title: "周末旅行的决定",
      location: "对话摘录 · 周末旅行的讨论",
      excerpt: "搭火车去台南，周六早上出发。",
      body: [
        "这周末想安排一趟不用赶行程的小旅行。",
        "我们决定搭火车去台南，周六早上出发。",
        "下午留给散步，不要把每个时间都排满。",
      ],
    },
    {
      id: "sample-file",
      type: "file",
      kind: "文件",
      title: "出发清单.md",
      location: "旅行文件夹 / 出发清单.md",
      excerpt: "先找车站附近的住宿，带轻便行李。",
      body: [
        "这次不开车，希望移动简单一点。",
        "先找台南车站附近的住宿，带轻便行李。",
        "还没订房，也还没订车票；出发前要确认。",
      ],
      path: "旅行文件夹 / 出发清单.md",
      quoteLine: 2,
    },
    {
      id: "sample-page",
      type: "page",
      kind: "页面",
      title: "慢旅行",
      location: "文澜 / 旅行灵感 / 慢旅行",
      excerpt: "短途旅行可以以一个街区为中心，留一些没有安排的时间。",
      body: [
        "这是一页已存在的范例知识，记下喜欢的旅行方式。",
        "短途旅行可以以一个街区为中心，留一些没有安排的时间。",
        "比起赶完景点清单，沿途发现的小店也能成为旅行的记忆。",
      ],
    },
  ],
  citations: [
    {
      id: "sample-cite-1",
      sourceId: "sample-memory",
      quote: "我们决定搭火车去台南，周六早上出发。",
    },
    {
      id: "sample-cite-2",
      sourceId: "sample-file",
      quote: "先找台南车站附近的住宿，带轻便行李。",
    },
    {
      id: "sample-cite-3",
      sourceId: "sample-page",
      quote: "短途旅行可以以一个街区为中心，留一些没有安排的时间。",
    },
    {
      id: "sample-cite-4",
      sourceId: "sample-memory",
      quote: "下午留给散步，不要把每个时间都排满。",
    },
    {
      id: "sample-cite-5",
      sourceId: "sample-file",
      quote: "还没订房，也还没订车票；出发前要确认。",
    },
  ],
  page: {
    title: "一个不赶路的台南周末",
    subtitle: "从对话里的决定，到出发前用得上的安排。",
    paragraphs: [
      {
        text: "交通已决定：搭火车去台南，周六早上出发。",
        citationIds: ["sample-cite-1"],
      },
      {
        text: "住宿先找车站附近，行李保持轻便，让移动更简单。",
        citationIds: ["sample-cite-2"],
      },
      {
        text: "行程以街区散步为主，保留没有安排的时间。这也呼应了对话里不想把下午排满的想法。",
        citationIds: ["sample-cite-4", "sample-cite-3"],
      },
    ],
    related: {
      title: "出发前，还有两件事",
      description: "把已决定的事，接到还没完成的准备。",
      body: "订房与订车票都还没有完成。先比较车站附近的住宿，再确认周六早上的车次。",
    },
  },
  commands: {
    chatgpt: {
      recall: "@wenlan 找出我导入的资料中已做出的决定与待办，并列出来源。",
      handoff: "@wenlan 记下这次工作的决定与进度，方便之后接续。",
      brief: "@wenlan 这是新的对话，请带我接上之前的工作与待办。",
    },
    codex: {
      recall: "/recall 我导入资料中的决定与待办",
      handoff: "/handoff",
      brief: "/brief",
    },
    claude: {
      recall: "/recall 我导入资料中的决定与待办",
      handoff: "/handoff",
      brief: "/brief",
    },
  },
};

const hantData: SampleDataset = {
  sources: [
    {
      id: "sample-memory",
      type: "memory",
      kind: "記憶",
      title: "週末旅行的決定",
      location: "對話擷取 · 週末旅行的討論",
      excerpt: "搭火車去台南，週六早上出發。",
      body: [
        "這週末想安排一趟不用趕行程的小旅行。",
        "我們決定搭火車去台南，週六早上出發。",
        "下午留給散步，不要把每個時間都排滿。",
      ],
    },
    {
      id: "sample-file",
      type: "file",
      kind: "檔案",
      title: "出發清單.md",
      location: "旅行資料夾 / 出發清單.md",
      excerpt: "先找車站附近的住宿，帶輕便行李。",
      body: [
        "這次不開車，希望移動簡單一點。",
        "先找台南車站附近的住宿，帶輕便行李。",
        "還沒訂房，也還沒訂車票；出發前要確認。",
      ],
      path: "旅行資料夾 / 出發清單.md",
      quoteLine: 2,
    },
    {
      id: "sample-page",
      type: "page",
      kind: "頁面",
      title: "慢旅行",
      location: "文瀾 / 旅行靈感 / 慢旅行",
      excerpt: "以一個街區為中心，留一些沒有安排的時間。",
      body: [
        "這是一頁已存在的範例知識，記下喜歡的旅行方式。",
        "短途旅行可以以一個街區為中心，留一些沒有安排的時間。",
        "比起趕完景點清單，沿途發現的小店也能成為旅行的記憶。",
      ],
    },
  ],
  citations: [
    {
      id: "sample-cite-1",
      sourceId: "sample-memory",
      quote: "我們決定搭火車去台南，週六早上出發。",
    },
    {
      id: "sample-cite-2",
      sourceId: "sample-file",
      quote: "先找台南車站附近的住宿，帶輕便行李。",
    },
    {
      id: "sample-cite-3",
      sourceId: "sample-page",
      quote: "短途旅行可以以一個街區為中心，留一些沒有安排的時間。",
    },
    {
      id: "sample-cite-4",
      sourceId: "sample-memory",
      quote: "下午留給散步，不要把每個時間都排滿。",
    },
    {
      id: "sample-cite-5",
      sourceId: "sample-file",
      quote: "還沒訂房，也還沒訂車票；出發前要確認。",
    },
  ],
  page: {
    title: "一個不趕路的台南週末",
    subtitle: "從對話裡的決定，到出發前用得上的安排。",
    paragraphs: [
      {
        text: "交通已決定：搭火車去台南，週六早上出發。",
        citationIds: ["sample-cite-1"],
      },
      {
        text: "住宿先找車站附近，行李保持輕便，讓移動更簡單。",
        citationIds: ["sample-cite-2"],
      },
      {
        text: "行程以街區散步為主，保留沒有安排的時間。這也呼應了對話裡不想把下午排滿的想法。",
        citationIds: ["sample-cite-4", "sample-cite-3"],
      },
    ],
    related: {
      title: "出發前，還有兩件事",
      description: "把已決定的事，接到還沒完成的準備。",
      body: "訂房與訂車票都還沒有完成。先比較車站附近的住宿，再確認週六早上的車次。",
    },
  },
  commands: {
    chatgpt: {
      recall: "@wenlan 找出我匯入的資料中已做出的決定與待辦，並列出來源。",
      handoff: "@wenlan 記下這次工作的決定與進度，方便之後接續。",
      brief: "@wenlan 這是新的對話，請帶我接上之前的工作與待辦。",
    },
    codex: {
      recall: "/recall 我匯入資料中的決定與待辦",
      handoff: "/handoff",
      brief: "/brief",
    },
    claude: {
      recall: "/recall 我匯入資料中的決定與待辦",
      handoff: "/handoff",
      brief: "/brief",
    },
  },
};

const DATASETS: Record<SampleLocale, SampleDataset> = {
  en: enData,
  "zh-Hans": hansData,
  "zh-Hant": hantData,
};

export function getSampleData(locale: SampleLocale): SampleDataset {
  return DATASETS[locale];
}
