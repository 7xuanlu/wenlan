<!-- README_SYNC: source=README.md sha256=a6204734668fa784a7fac85c76d08ab696d22dfa5bf681160942e963a65b6c93 -->

<p align="center">
  <picture>
    <source media="(max-width: 600px)" srcset="./docs/assets/readme-banner-mobile.png">
    <img src="./docs/assets/readme-banner.png" alt="Wenlan：有來源支撐、會持續累積的知識庫。" width="100%">
  </picture>
</p>

和 AI 聊出的成果，不該在對話結束後消失。Wenlan 會建立真正需要的頁面，並在來源變動時讓它們保持最新；只有需要判斷時才找你。

<p align="center">
  <a href="./README.md">English</a> | <a href="./README.zh-Hans.md">简体中文</a> | 繁體中文
</p>

<p align="center">
  <a href="https://github.com/7xuanlu/wenlan/actions/workflows/ci.yml?query=branch%3Amain"><img alt="CI" src="https://github.com/7xuanlu/wenlan/actions/workflows/ci.yml/badge.svg?branch=main&event=push"></a>
  <a href="https://github.com/7xuanlu/wenlan/releases/latest"><img alt="最新版本" src="https://img.shields.io/github/v/release/7xuanlu/wenlan?sort=semver&label=release"></a>
  <a href="#license"><img alt="授權：Apache-2.0" src="https://img.shields.io/badge/license-Apache--2.0-blue.svg"></a>
</p>

<p align="center">
  <a href="#start-in-30-seconds">開&#8288;始&#8288;使&#8288;用</a> ·
  <a href="#what-does-wenlan-build">這&#8288;是&#8288;什&#8288;麼？</a> ·
  <a href="#what-can-it-do">能&#8288;力</a> ·
  <a href="#how-does-it-work">日&#8288;常&#8288;流&#8288;程</a> ·
  <a href="#evaluation">評&#8288;估</a> ·
  <a href="#learn-more">進&#8288;一&#8288;步&#8288;了&#8288;解</a>
</p>

<p align="center">
  <img src="./docs/assets/desktop-wiki-preview.png" alt="Wenlan 桌面 app，展示有來源支撐的 wiki 頁面與可檢查的來源記憶。" width="100%">
</p>

---

<a id="quickstart"></a>
<a id="start-in-30-seconds"></a>

## 開始使用

<a id="start-with-the-app"></a>
<a id="open-the-wiki"></a>

### 桌面 app

桌面 app 是最快看到完整工作流程的方式：閱讀頁面、檢查來源並整理知識體系。目前僅提供 macOS Apple Silicon 預覽版，尚未經 Apple notarization。下面的安裝器會驗證 GitHub release，只為 Wenlan 清除 quarantine，安裝後直接開啟，不會變更 macOS 系統安全設定：

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/7xuanlu/wenlan/main/scripts/install-macos-app.sh)"
```

你可以直接[檢查安裝器原始碼](scripts/install-macos-app.sh)。安裝器會先用 GitHub 發布的 SHA-256 核對下載檔案，再替換現有 app。偏好 DMG 或想查看 app 原始碼？請前往 [wenlan-app releases](https://github.com/7xuanlu/wenlan-app/releases/latest) 和 [wenlan-app](https://github.com/7xuanlu/wenlan-app)。

<a id="claude-code-in-30-seconds"></a>

<a id="codex-plugin"></a>

<a id="mcp-setup"></a>
<a id="mcp-clients"></a>

### 讓你的 AI 完成設定

把下面這段貼給 Claude Code、Codex，或其他能夠讀取設定指南的工具：

```text
請為目前的 AI 用戶端設定 Wenlan，並嚴格遵循：
https://raw.githubusercontent.com/7xuanlu/wenlan/main/docs/setup-with-ai.md

只安裝這個用戶端需要的內容。完成後驗證本地 runtime、
Wenlan connection，以及一次 capture/recall round trip。
```

指南會識別目前使用的 client，把各平台命令留在專門文件中。除非你明確要求，否則它不會設定所有 AI 工具。

只需要無 GUI 的本地 runtime？

```bash
npx -y wenlan setup
```

手動與各 client 設定說明：[AI 輔助設定](docs/setup-with-ai.md) · [Claude Code plugin](plugin/.claude-plugin/README.md) · [Codex plugin](plugin-codex/README.md) · [CLI 與 MCP](crates/wenlan-cli/README.md)。

---

<a id="what-does-wenlan-build"></a>
<a id="why-it-compounds"></a>

## 這是什麼？

Wenlan 把文件、筆記和 AI 對話整理成可追溯來源、會隨工作持續改善的知識庫。工作中的新資訊會修正既有知識，相關頁面也會一起更新。

<p align="center">
  <picture>
    <source media="(max-width: 600px)" srcset="./docs/assets/wenlan-system-zh-Hant-mobile.png">
    <img src="./docs/assets/wenlan-system-zh-Hant.png" alt="Wenlan 把文件、Obsidian 筆記和 AI 對話等來源，與工作中值得保留的決策、經驗和修正連起來，維護有引用的頁面，並讓最新知識回到下一次 AI 工作。" width="100%">
  </picture>
</p>

這是 Wenlan 對 llm-wiki v2 的實作：證據與頁面擁有各自獨立又彼此相連的生命週期。繁瑣的維護工作由 Wenlan 處理；只有來源衝突，或更新會改動你親自寫的內容時，才需要你判斷。

<p align="center">
  <img src="./docs/assets/feature-reel.gif" alt="Wenlan feature reel，展示有來源支撐的頁面、來源檢查、graph context、agent capture 與 curation。" width="100%">
</p>

### 隨時可查的證據

來源文件、匯入的對話和工作中捕獲的決策，會保留成可追溯的記憶與來源紀錄。你能知道它們來自哪裡、目前有多穩定，以及後來有哪些資訊修正或取代了它們。

### 會持續複利的 wiki

Wenlan 把相關證據彙整成附有來源引用的 Markdown 頁面。即使換了 AI 工具，`/brief` 和 `/recall` 也能把目前的頁面及其證據帶回後續工作。新材料會繼續改善同一個頁面，而不是又產生一個互不相連的答案。

頁面和 session notes 都以純 Markdown 保存在 `~/.wenlan/`。Distill 與 handoff workflows 會把檔案的邏輯批次提交到本地 git repository，留下可檢查、可攜帶的歷史。

本地歷史可以直接檢查：

```text
$ git -C ~/.wenlan log --oneline
a1b2c3d distill: 4 pages
9f8e7d6 session: embedding-work
```

**已經在用 Obsidian？繼續用。** Wenlan 可以把現有 vault 當成來源讀取。若要在 Obsidian 中使用 Wenlan 自己的頁面，可以把 `~/.wenlan/pages/` symlink 到 vault，或從桌面 app 匯出頁面。Wenlan 會把你對這些頁面的編輯視為使用者擁有的內容；之後的機器更新會成為可審核的修訂建議，不會覆蓋你的文字。

<a id="what-makes-wenlan-distinct"></a>
<a id="why-is-wenlan-different"></a>
<a id="two-lifecycles"></a>

### llm-wiki v2：兩套相連的生命週期

生成出來的 wiki 只是一份快照。Wenlan 把它當成持續維護的知識體系，讓證據與由它整理成的文章各自演進，又彼此相連。

<p align="center">
  <picture>
    <source media="(max-width: 600px)" srcset="./docs/assets/wenlan-lifecycle-zh-Hant-mobile.png">
    <img src="./docs/assets/wenlan-lifecycle-zh-Hant.png" alt="Wenlan 的雙生命週期：依據會被捕獲、豐富、關聯、更正與審核；有來源引用的頁面會辨識過時內容並更新，同時保護人工文字；後台精煉階段持續維護體系，並明確標示預設關閉的可選行為。" width="100%">
  </picture>
</p>

- **記憶生命週期：** 證據可以被確認、連結、修正或取代，而不失去原有歷史。近似重複可以安靜合併；矛盾則等待審核。
- **文章生命週期：** 每篇文章都保留引用，也知道來源何時改變。由 Wenlan 維護的文章可以刷新；你親自編輯過的文章只會收到修訂提案，不會被靜默改寫。
- **兩者的連結：** 記憶改變時，Wenlan 知道哪些文章可能已經過時，並能在保留證據脈絡的前提下更新它們。

例如，匯入一份設計文件，再讓 Codex 捕獲一次除錯決策。Wenlan 可以把兩者整理成一篇同時引用兩份證據的文章。任一來源改變時，文章可以刷新；如果你已經編輯過文章，更新提案會等待審核。

這就是 Wenlan 所說的 llm-wiki v2：證據與文章會一起持續改善，而不是生成一次就停止。

<a id="what-wenlan-is-not"></a>

### 適合需要長期延續的工作

Wenlan 適合橫跨多次會話、專案與數週的軟體開發、研究、寫作、諮詢、產品決策和客戶工作。它不是為一次性聊天、生活管理系統，或作為其他產品的記憶 SDK 而設計。

---

<a id="what-you-get"></a>
<a id="what-can-it-do"></a>

## 能力

- **整合多種來源：** 匯入 ChatGPT 和 Claude 對話、索引 Obsidian 或文件資料夾、接收工作中的直接 captures，並讓它們共同成為同一個頁面的證據。
- **有證據支撐的知識：** captures 保留 source agent、confidence、stability 與 supersession；pages 保留 source memories、citation records、stale reasons、ownership 與 revision state。
- **持續維護、可審核的頁面：** 自動 re-distill refresh 無法驗證其 claims 時會 fail closed。對使用者擁有頁面的更新會成為 pending revision，不會靜默改寫。
- **會話之間持續整理：** 設定本地模型或 API provider 後，背景流程可以 enrich captures、連結 entities、合併重複內容，並在下一次 session 前刷新符合條件的頁面。
- **只在需要時審核：** 矛盾與受保護 memory 的衝突可以進入明確 review，而不會把每條 capture 都變成審批任務。
- **混合關聯檢索：** libSQL 以 reciprocal-rank fusion 結合 FTS5、vector search、pages、memories 與 graph context，也可選擇本地 cross-encoder reranker。
- **跨工具連續性：** Claude Code、Codex、Cursor、桌面 app 與 MCP clients 查詢同一個本地 daemon，因此在一個工具捕獲的 context 可以在另一個工具回來。
- **明確空間：** 把 memories、pages 與 recall 限定在 work、personal 或 client contexts；預設可依 repo 判斷，也永遠能明確覆寫。
- **保留 Obsidian，也不被綁住：** 把現有 vault 當成唯讀來源索引，再用你熟悉的編輯器閱讀、編輯、symlink 或匯出 Wenlan 的 Markdown 頁面。
- **本機優先的資料所有權：** daemon 預設只綁定 localhost；memories 與 graph data 留在本地 libSQL，長期 pages 與 session notes 則以使用者擁有的 Markdown 保存在 `~/.wenlan/`，並留下本地 git 歷史。

<a id="what-can-i-bring-in"></a>

### 支援來源

Wenlan 從你已經擁有的材料開始，並讓每一項內容都能追溯到來源。

- **過去的 AI 對話：** 把 ChatGPT 或 Claude 匯出的 ZIP 放進桌面 app。Wenlan 會批次匯入對話，並自動略過已經匯入的內容。
- **筆記與文件：** 連接 Obsidian vault，或任何包含 `.md`、`.txt`、`.pdf` 的資料夾。Wenlan 只讀取來源資料夾，不會回寫；一般資料夾會在背景檢查變更，Obsidian vault 則可從 app 重新同步。CLI 也能用 `wenlan sources add <path>` 登錄單一支援檔案。
- **正在進行的 AI 工作：** Claude Code、Codex、Cursor、Claude Desktop、VS Code、Gemini CLI 與其他 MCP 用戶端，都能在工作過程中把決策、經驗和脈絡存進同一個本地知識庫。
- **自訂整合：** 需要接上其他收集流程時，本地 HTTP API 可以接收整理好的文字、網頁內容與記憶。

一份文件、一段舊對話和一項新的 agent 決策，可以共同支撐同一個頁面，而不再分散於不同孤島。

---

<a id="how-wenlan-works"></a>
<a id="how-does-it-work"></a>

## 日常流程

日常使用分成一個小循環：取回相關知識、保存工作重點、以 handoff 收尾，再由 Wenlan 整理下次需要的內容。每一輪都改善同一個知識庫，不再累積互不相連的歷史。

這個循環分成四步：

1. **帶著脈絡開始。** 帶回頁面、記憶、目前進度與你的偏好：`/brief [topic]`。其他 AI 工具可使用等價的 `context` 工具。
2. **工作時隨手保存與查找。** `/capture <thing>` 保存決策、經驗、踩坑或事實，並記錄來源。`/recall <query>` 只取回相關內容，不載入全部歷史。
3. **閉合循環。** `/handoff` 記錄改動與待辦，也指出下次工作的起點。
4. **讓 wiki 保持最新。** `/distill` 主動建立或刷新頁面。背景流程會在兩次工作之間補充已保存內容、連結相關知識、合併重複內容，並刷新符合條件的頁面。`/lint` 檢查知識庫健康狀態；`/curate` 讓你審核衝突與更新提案。

### 選擇 Wenlan 如何整理

Capture、recall、混合搜尋、graph context 與 Markdown store 不需要 API key 或 cloud account，就能在本地運作。需要彙整頁面或進行更深入的補充時，可以使用裝置上的模型、Ollama 或 LM Studio 等 OpenAI-compatible 本地端點，或已設定的雲端 provider。Wenlan 不傳送 telemetry。

完整 workflow 參考：[plugin/skills](plugin/skills/README.md)。

---

<a id="evaluation"></a>

## 評估

以下是 retrieval-only snapshot，不代表 end-to-end answer quality。方法、環境 receipts 與更新流程見 [docs/eval](docs/eval/README.md)。

<!-- EVAL_SNAPSHOT_START -->
| Benchmark | Recall@5 | MRR | NDCG@10 |
|---|---:|---:|---:|
| LME_Oracle (500 Q) | 93.6% | 0.857 | 0.883 |
| LME_S (deep, 90 Q) | 87.7% | 0.815 | 0.822 |
<!-- EVAL_SNAPSHOT_END -->

---

<a id="build-from-source"></a>

## 從原始碼建置

Wenlan 可從原始碼建置於 macOS（Apple Silicon 與 Intel）、Linux（x86_64 與 ARM64，glibc）和 Windows（x86_64）。目前預建 releases 支援 macOS Apple Silicon、Linux x86_64/ARM64 with glibc 與 Windows x86_64；macOS Intel 仍需從原始碼建置。多數使用者應透過 plugin 或 `npx` 安裝。

```bash
git clone https://github.com/7xuanlu/wenlan.git
cd wenlan
cargo build --workspace
cargo run -p wenlan-server
```

Runtime、crates 與平台細節見 [AGENTS.md](AGENTS.md#cross-platform) 和各 crate README。

---

<a id="learn-more"></a>

## 進一步了解

更完整的文件、概念說明與比較：

### 文件

- [開始使用](https://wenlan.app/docs/get-started)：安裝並驗證第一個本地循環。
- [日常工作流程](https://wenlan.app/docs/daily-workflow)：brief、capture、recall、handoff、distill、lint 與 curate。
- [MCP 用戶端](https://wenlan.app/docs/mcp-clients)：連接 Claude Code、Codex、Cursor、Claude Desktop 與其他工具。

### 概念

- [為什麼需要持續演進的 wiki，而不只是 AI 記憶](https://wenlan.app/learn/ai-work-memory)：深入理解問題與產品模型。
- [MCP 記憶伺服器](https://wenlan.app/learn/mcp-memory-server)：Wenlan 如何讓知識跨 AI 工具使用。
- [本機優先的 AI 記憶](https://wenlan.app/learn/local-first-ai-memory)：資料、隱私與控制權。
- [Markdown 與本地索引](https://wenlan.app/learn/markdown-local-index-ai-memory)：儲存、檢索與所有權。
- [AI agent 的交接循環](https://wenlan.app/learn/ai-agent-handoff-loop)：把工作完整帶到下一次會話。

### 比較

- [Wenlan 與 Basic Memory](https://wenlan.app/learn/wenlan-vs-basic-memory)
- [Wenlan 與 claude-mem](https://wenlan.app/learn/wenlan-vs-claude-mem)
- [Wenlan 與 Superlocal Memory](https://wenlan.app/learn/wenlan-vs-superlocal-memory)

---

## 貢獻

歡迎 bug fixes、eval cases、文件與功能。先閱讀 [CONTRIBUTING.md](CONTRIBUTING.md)。架構與開發規則在 [AGENTS.md](AGENTS.md)。安全性問題請見 [SECURITY.md](SECURITY.md)，也請閱讀 [Code of Conduct](CODE_OF_CONDUCT.md)。

---

<a id="license"></a>

## 授權

Wenlan 採用 **Apache-2.0** 授權，包括本 repository 內的 local runtime、CLI、MCP server、shared types，以及 Claude Code/Codex plugin files。

---

<a id="acknowledgments"></a>

## 源流與同類專案

Wenlan（文瀾）的名字來自文瀾閣。這座皇家藏書樓收藏《四庫全書》，曾是中國最大的藏書之一。

Wenlan 吸收 LLM-wiki 與 agent-memory 兩條脈絡，但不聲稱完整實作 LLM Wiki v2：

- [Karpathy 的 LLM-wiki note](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) 建立了從 raw sources 到持續維護 wiki 的模式。
- [Rohitg00 的 LLM Wiki v2 proposal](https://gist.github.com/rohitg00/2067ab416f7bbe447c1977edaaa681e2) 加入 memory lifecycle、confidence、graph 與 retrieval mechanisms。[agentmemory](https://github.com/rohitg00/agentmemory) 是其具體的 agent-memory implementation。
- [nashsu/llm_wiki](https://github.com/nashsu/llm_wiki) 是以文件為核心的 LLM-wiki 完整桌面實作。
- [basic-memory](https://github.com/basicmachines-co/basic-memory)、[obsidian-mind](https://github.com/breferrari/obsidian-mind)、[mcp-memory-service](https://pypi.org/project/mcp-memory-service/)、[Memoria](https://github.com/matrixorigin/Memoria) 和 [OpenMemory](https://github.com/CaviraOSS/OpenMemory) 探索相鄰的本地知識與 agent-memory 方向。
