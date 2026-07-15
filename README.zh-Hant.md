<!-- README_SYNC: source=README.md sha256=2fc14ca626575e9b4d41fc4fe06e56fafe958ce60a9d1d32e82b27426b1d61cd -->

<p align="center">
  <picture>
    <source media="(max-width: 600px)" srcset="./docs/assets/readme-banner-mobile.png">
    <img src="./docs/assets/readme-banner.png" alt="Wenlan：有來源支撐、持續更新的 llm-wiki。" width="100%">
  </picture>
</p>

和 AI 聊出的成果，不該在對話結束後消失。

Wenlan 會把散落的文件、筆記和 AI 對話整理成同一套持續更新的 wiki。重要知識能在不同 AI 工具裡再次派上用場，來源也隨時可查。平常的整理和更新由 Wenlan 處理；只有需要你判斷時才會請你介入，例如來源彼此矛盾，或需要改動你親自寫的內容。

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
  <a href="#how-does-it-work">如&#8288;何&#8288;運&#8288;作</a> ·
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

桌面 app 是最快看到完整工作流程的方式：閱讀頁面、檢查來源，並整理知識體系。從 [wenlan-app releases](https://github.com/7xuanlu/wenlan-app/releases/latest) 下載最新 macOS Apple Silicon DMG，開啟 Wenlan，再跟隨內建設定精靈。App 自帶本地執行環境，也能設定偵測到的 AI 工具，不需要先使用終端。

App 原始碼：[wenlan-app](https://github.com/7xuanlu/wenlan-app)。產品與文件：[wenlan.app](https://wenlan.app)。

<a id="claude-code-in-30-seconds"></a>

### Claude Code

```text
/plugin marketplace add 7xuanlu/wenlan
/plugin install wenlan@7xuanlu-wenlan
/setup
```

如果 Claude Code 安裝後要求重新啟動，請重啟一次，再執行 `/setup`。Plugin 會完成本地 runtime、MCP 連線、知識庫與第一次 round-trip 檢查。

Plugin 指令與工作流程：[plugin/](plugin/.claude-plugin/README.md)。

<a id="codex-plugin"></a>

### Codex plugin

```bash
npx -y wenlan setup
codex plugin marketplace add .
codex plugin add wenlan@7xuanlu-wenlan
```

安裝後開啟新的 Codex thread，讓 plugin 與 MCP server 載入。詳細說明：[plugin-codex/](plugin-codex/README.md)。

<a id="mcp-setup"></a>
<a id="mcp-clients"></a>

### 無 GUI 與其他 MCP 用戶端

偏好只使用終端，或所用用戶端不需要 Wenlan 的圖形介面？

```bash
npx -y wenlan setup
wenlan connect claude-code  # or: codex, cursor, claude-desktop, vscode, gemini
```

核心工具是 `context`、`capture`、`recall`、`pages` 和 `doctor`。所有連線的 client 都使用同一個本地 daemon 與 store。

<a id="cli"></a>

偏好直接用 CLI？

```bash
wenlan status
wenlan recall <query>
wenlan capture <text>
```

CLI 詳細說明：[crates/wenlan-cli](crates/wenlan-cli/README.md)。

---

<a id="what-does-wenlan-build"></a>
<a id="why-it-compounds"></a>

## 這是什麼？

<p align="center">
  <img src="./docs/assets/feature-reel.gif" alt="Wenlan feature reel，展示有來源支撐的頁面、來源檢查、graph context、agent capture 與 curation。" width="100%">
</p>

Wenlan 是一套 local-first llm-wiki，由兩個相連的知識層構成：可檢查的證據，以及會隨時間持續累積的 wiki。

### 隨時可查的證據

來源文件、匯入的對話和工作中捕獲的決策，會保留成可追溯的記憶與來源紀錄。你能知道它們來自哪裡、目前有多穩定，以及後來有哪些資訊修正或取代了它們。

### 會持續複利的 wiki

Wenlan 把相關證據彙整成附有來源引用的 Markdown 頁面。即使換了 AI 工具，`/brief` 和 `/recall` 也能把目前的頁面及其證據帶回後續工作。新材料會繼續改善同一個頁面，而不是又產生一個互不相連的答案。

頁面和 session notes 都以純 Markdown 保存在 `~/.wenlan/`。Distill 與 handoff workflows 會把檔案的邏輯批次提交到本地 git repository，留下可檢查、可攜帶的歷史。

**已經在用 Obsidian？繼續用。** Wenlan 可以把現有 vault 當成來源讀取。若要在 Obsidian 中使用 Wenlan 自己的頁面，可以把 `~/.wenlan/pages/` symlink 到 vault，或從桌面 app 匯出頁面。Wenlan 會把你對這些頁面的編輯視為使用者擁有的內容；之後的機器更新會成為可審核的修訂建議，不會覆蓋你的文字。

<a id="what-makes-wenlan-distinct"></a>
<a id="why-is-wenlan-different"></a>
<a id="two-lifecycles"></a>

### llm-wiki v2：兩套相連的生命週期

生成出來的 wiki 只是一份快照。Wenlan 把它當成持續維護的知識體系，讓證據與由它整理成的文章各自演進，又彼此相連。

- **記憶生命週期：** 證據可以被確認、連結、修正或取代，而不失去原有歷史。近似重複可以安靜合併；矛盾則等待審核。
- **文章生命週期：** 每篇文章都保留引用，也知道來源何時改變。由 Wenlan 維護的文章可以刷新；你親自編輯過的文章只會收到修訂提案，不會被靜默改寫。
- **兩者的連結：** 記憶改變時，Wenlan 知道哪些文章可能已經過時，並能在保留證據脈絡的前提下更新它們。

例如，匯入一份設計文件，再讓 Codex 捕獲一次除錯決策。Wenlan 可以把兩者整理成一篇同時引用兩份證據的文章。任一來源改變時，文章可以刷新；如果你已經編輯過文章，更新提案會等待審核。

這就是 Wenlan 所說的 llm-wiki v2：證據與文章會一起持續改善，而不是生成一次就停止。

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

## 如何運作

四個公開 workflow 驅動整個系統：

1. **帶著 context 開始。** `/brief [topic]` 載入專案狀態、偏好與相關知識。只使用 MCP 的 clients 可呼叫 `context` 完成同一件事。
2. **在工作中捕獲與查找。** `/capture <thing>` 以 typed provenance 儲存決策、經驗、gotcha 或事實。`/recall <query>` 只取回相關部分，不把全部歷史塞進 context。
3. **閉合循環。** `/handoff` 記錄改了什麼、還有哪些未解問題，以及下一次 session 該從哪裡繼續。
4. **維護 wiki。** `/distill` 主動建立或刷新頁面。背景流程可以在 sessions 之間補充 captures、連結 entities、合併重複內容，並刷新符合條件的頁面。`/lint` 與 `/curate` 則公開完整性與審核工作。

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

- [開始使用](https://wenlan.app/docs/get-started)：安裝並驗證第一個本地循環。
- [日常 workflow](https://wenlan.app/docs/daily-workflow)：brief、capture、recall、handoff、distill、lint 與 curate。
- [MCP clients](https://wenlan.app/docs/mcp-clients)：連接 Claude Code、Codex、Cursor、Claude Desktop 與其他 clients。
- [為什麼需要 living wiki，而不只是 AI memory](https://wenlan.app/learn/ai-work-memory)：深入說明問題與產品模型。
- [Markdown 與本地索引](https://wenlan.app/learn/markdown-local-index-ai-memory)：儲存、retrieval 與 ownership。

<a id="what-wenlan-is-not"></a>

Wenlan 專注於長期 AI 工作產生的知識。它不是 life OS、通用 workflow suite、memory infrastructure SDK，也不會取代你已經在用的筆記編輯器。

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
