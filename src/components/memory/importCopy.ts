// SPDX-License-Identifier: AGPL-3.0-only
//
// Locale literals for the real ImportView onboarding import. Imported by
// src/i18n/resources.ts into the `importView` namespace of every locale.
// The three objects must keep identical key sets (parity is enforced by
// src/i18n/resources.test.ts).
//
// Each locale carries its own fully localized export prompt body: both the
// shown instructions and the copied text come from `exportPromptBody` via
// t(). Only the [TYPE] machine tags — and their protocol meaning — stay
// literal in every locale, never translated. Brand names ChatGPT/Claude
// stay literal; "Other" is localized via `sourceOther`.
//
// EXPORT_PROMPT is the English body, kept as a named export for tests.

export const EXPORT_PROMPT = `Export all of my stored memories and any context you've learned about me. Preserve my words verbatim where possible.

EVERY line MUST follow this exact format — no exceptions:
[TYPE] - content

TYPE must be exactly one of: identity, preference, decision, lesson, gotcha, fact

Where:
- identity = who I am (name, location, education, family, languages)
- preference = how I like things (opinions, tastes, working style, rules like "always do X" or "never do Y")
- decision = choices I made with rationale (tech, career, project directions)
- lesson = reusable learnings from experience
- gotcha = pitfalls, traps, or things to avoid
- fact = things about my work, projects, skills, situation

Example output:
[identity] - Lives in San Francisco, originally from Taiwan
[preference] - Prefers concise responses without trailing summaries
[decision] - Chose Rust + Tauri for the desktop app over Electron
[preference] - Never use emojis unless explicitly asked
[lesson] - TDD caught the config regression before launch
[gotcha] - Tauri rolling::daily suffixes file names with the date
[fact] - Building a local-first AI memory layer called Wenlan

Rules:
- NO section headers, category labels, or grouping text
- NO explanations before or after — ONLY the tagged lines
- One memory per line
- Wrap entire output in a single code block`;

/** Protocol tags that must appear literally in anything the user copies. */
export const IMPORT_TYPE_TAGS = [
  "[identity]",
  "[preference]",
  "[decision]",
  "[lesson]",
  "[gotcha]",
  "[fact]",
] as const;

/**
 * Extract the meaningful detail from an import failure without swallowing
 * it. Strings and Error messages pass through; anything else degrades to
 * "" so the caller can fall back to a bare localized heading instead of
 * printing "[object Object]".
 */
export function formatImportErrorDetail(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  if (
    err !== null &&
    typeof err === "object" &&
    "message" in err &&
    typeof (err as { message: unknown }).message === "string"
  ) {
    return (err as { message: string }).message;
  }
  if (typeof err === "number" || typeof err === "boolean") return String(err);
  return "";
}

export const enImportView = {
  title: "Import Memories",
  back: "Back",
  sourceOther: "Other",
  exportPrompt: "Export prompt",
  exportPromptBody: EXPORT_PROMPT,
  instructions: "Instructions",
  copyPrompt: "Copy prompt",
  copied: "Copied!",
  copyError: "Couldn't copy the prompt. Try again.",
  exportIntro:
    "Copy this prompt, paste into {{source}}, then paste the output below.",
  otherInstructions: `Paste any list of facts or memories, one per line.

Each line becomes a separate memory in Wenlan.
Empty lines and separators (---, ===) are skipped.

Optionally prefix lines with a type tag:
[identity] - Lives in San Francisco
[preference] - Prefers concise responses
[lesson] - TDD caught a config regression before launch
[gotcha] - Tauri rolling::daily suffixes file names with the date
[fact] - Building Wenlan, a local-first AI memory app

Lines without a tag are stored as facts.`,
  pasteOutput: "Paste output",
  lines_one: "{{count}} line",
  lines_other: "{{count}} lines",
  placeholder: "Paste your memories here, one per line...",
  uploadFile: "Upload file",
  fileReadError: "Couldn't read that file. Try again.",
  import: "Import",
  skip: "Skip",
  importFailedTitle: "Import failed",
  viewMemories: "View memories",
  importMore: "Import more",
  continue: "Continue",
  typeLabels: {
    identity: "Identity",
    preference: "Preference",
    decision: "Decision",
    lesson: "Lesson",
    gotcha: "Gotcha",
    fact: "Fact",
    goal: "Goal",
    unclassified: "Unclassified",
  },
};

export const hansImportView = {
  title: "导入记忆",
  back: "返回",
  sourceOther: "其他",
  exportPrompt: "导出提示词",
  exportPromptBody: `导出我存储的所有记忆以及你学到的关于我的上下文。尽量保留我的原话。

每一行都必须严格遵循以下格式——没有例外：
[TYPE] - content

TYPE 必须是以下之一：identity, preference, decision, lesson, gotcha, fact

其中：
- identity = 我是谁（姓名、地点、教育、家庭、语言）
- preference = 我的喜好（观点、品味、工作风格，像“总是做 X”或“从不做 Y”这样的规则）
- decision = 我做出的选择及理由（技术、职业、项目方向）
- lesson = 从经验中获得的可复用收获
- gotcha = 陷阱、坑，或需要避免的事情
- fact = 关于我的工作、项目、技能、现状的事实

示例输出：
[identity] - 住在旧金山，原籍台湾
[preference] - 喜欢简洁的回复，不要结尾总结
[decision] - 桌面应用选择了 Rust + Tauri，而不是 Electron
[preference] - 除非明确要求，否则从不使用表情符号
[lesson] - TDD 在发布前发现了配置回归问题
[gotcha] - Tauri rolling::daily 会在文件名后加上日期
[fact] - 正在开发本地优先的 AI 记忆层 Wenlan

规则：
- 不要章节标题、分类标签或分组文字
- 不要前后解释——只要带标签的行
- 每行一条记忆
- 整个输出包在一个代码块里`,
  instructions: "说明",
  copyPrompt: "复制提示词",
  copied: "已复制！",
  copyError: "无法复制提示词，请重试。",
  exportIntro: "复制这段提示词，粘贴到 {{source}}，再把它的输出粘贴到下方。",
  otherInstructions: `粘贴事实或记忆列表，每行一条。

每行都会成为文澜中的一条记忆。
空行和分隔符（---、===）会被跳过。

也可以在行首加上类型标签：
[identity] - 住在旧金山
[preference] - 喜欢简洁的回复
[lesson] - TDD 在发布前发现了配置回归问题
[gotcha] - Tauri rolling::daily 会在文件名后加上日期
[fact] - 正在开发本地优先的 AI 记忆应用 Wenlan

没有标签的行会存为事实。`,
  pasteOutput: "粘贴输出",
  lines_one: "{{count}} 行",
  lines_other: "{{count}} 行",
  placeholder: "把记忆粘贴到这里，每行一条…",
  uploadFile: "上传文件",
  fileReadError: "文件读取失败，请重试。",
  import: "导入",
  skip: "跳过",
  importFailedTitle: "导入失败",
  viewMemories: "查看记忆",
  importMore: "再导入",
  continue: "继续",
  typeLabels: {
    identity: "身份",
    preference: "偏好",
    decision: "决定",
    lesson: "经验",
    gotcha: "注意事项",
    fact: "事实",
    goal: "目标",
    unclassified: "待分类",
  },
};

export const hantImportView = {
  title: "匯入記憶",
  back: "返回",
  sourceOther: "其他",
  exportPrompt: "匯出提示詞",
  exportPromptBody: `匯出我儲存的所有記憶以及你學到的關於我的上下文。盡量保留我的原話。

每一行都必須嚴格遵循以下格式——沒有例外：
[TYPE] - content

TYPE 必須是以下之一：identity, preference, decision, lesson, gotcha, fact

其中：
- identity = 我是誰（姓名、地點、教育、家庭、語言）
- preference = 我的喜好（觀點、品味、工作風格，像「總是做 X」或「從不做 Y」這樣的規則）
- decision = 我做出的選擇及理由（技術、職業、專案方向）
- lesson = 從經驗中獲得的可重用收穫
- gotcha = 陷阱、坑，或需要避免的事情
- fact = 關於我的工作、專案、技能、現狀的事實

範例輸出：
[identity] - 住在舊金山，原籍台灣
[preference] - 喜歡簡潔的回覆，不要結尾總結
[decision] - 桌面應用選擇了 Rust + Tauri，而不是 Electron
[preference] - 除非明確要求，否則從不使用表情符號
[lesson] - TDD 在發佈前發現了設定回歸問題
[gotcha] - Tauri rolling::daily 會在檔名後加上日期
[fact] - 正在開發本地優先的 AI 記憶層 Wenlan

規則：
- 不要章節標題、分類標籤或分組文字
- 不要前後解釋——只要帶標籤的行
- 每行一則記憶
- 整個輸出包在一個程式碼區塊裡`,
  instructions: "說明",
  copyPrompt: "複製提示詞",
  copied: "已複製！",
  copyError: "無法複製提示詞，請重試。",
  exportIntro: "複製這段提示詞，貼到 {{source}}，再把它的輸出貼到下方。",
  otherInstructions: `貼上事實或記憶清單，每行一則。

每行都會成為文瀾中的一則記憶。
空行和分隔符（---、===）會被略過。

也可以在行首加上類型標籤：
[identity] - 住在舊金山
[preference] - 喜歡簡潔的回覆
[lesson] - TDD 在發佈前發現了設定回歸問題
[gotcha] - Tauri rolling::daily 會在檔名後加上日期
[fact] - 正在開發本地優先的 AI 記憶應用 Wenlan

沒有標籤的行會存為事實。`,
  pasteOutput: "貼上輸出",
  lines_one: "{{count}} 行",
  lines_other: "{{count}} 行",
  placeholder: "把記憶貼到這裡，每行一則…",
  uploadFile: "上傳檔案",
  fileReadError: "檔案讀取失敗，請重試。",
  import: "匯入",
  skip: "略過",
  importFailedTitle: "匯入失敗",
  viewMemories: "查看記憶",
  importMore: "再匯入",
  continue: "繼續",
  typeLabels: {
    identity: "身份",
    preference: "偏好",
    decision: "決定",
    lesson: "經驗",
    gotcha: "注意事項",
    fact: "事實",
    goal: "目標",
    unclassified: "待分類",
  },
};
