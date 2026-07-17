<!-- README_SYNC: source=README.md sha256=c6ffffa1258f3fd3377ba83735fcad9a0c9ab37cf87bd5bbaff4aee221016a6b -->

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
  <img src="./docs/assets/desktop-wiki-preview.png" alt="Wenlan 桌面 app，展示有來源支撐的 wiki 頁面與可檢查的引用。" width="100%">
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

只需要在 macOS Apple Silicon 上執行的無 GUI 本地服務？

```bash
npx -y wenlan setup
```

這個命令會下載預先編譯的 CLI、背景服務（daemon）與 MCP 連接器，啟動並驗證本地服務；不需要安裝 Rust 或 Cargo。Linux x64/ARM64 可以使用自動化的 [shell 設定流程](docs/setup-with-ai.md#install-the-runtime)；Windows x64 請從 [Releases](https://github.com/7xuanlu/wenlan/releases/latest) 下載對應的 archive。macOS Intel 目前[沒有受支援的完整 runtime 安裝方式](crates/wenlan-cli/README.md#macos-intel)。

手動與各 client 設定說明：[AI 輔助設定](docs/setup-with-ai.md) · [Claude Code plugin](plugin/.claude-plugin/README.md) · [Codex plugin](plugin-codex/README.md) · [CLI 與 MCP](crates/wenlan-cli/README.md)。

---

<a id="what-does-wenlan-build"></a>
<a id="why-it-compounds"></a>

## 這是什麼？

Wenlan 把文件、筆記和過去的 AI 對話整理成會隨工作持續更新、每個結論都能追溯來源的知識庫。原始材料保留為來源；工作中的決策、經驗與修正成為長期記憶；兩者都能支撐同一批持續維護的頁面。

<p align="center">
  <picture>
    <source media="(max-width: 600px)" srcset="./docs/assets/wenlan-system-zh-Hant-mobile.png">
    <img src="./docs/assets/wenlan-system-zh-Hant.png" alt="來源與記憶分別支撐同一個持續維護的頁面。頁面過時後，Wenlan 可以依目前依據重建；可選的衝突審核可以讓受保護內容的衝突浮現，對人工文字的改動則等待使用者判斷。" width="100%">
  </picture>
</p>

<a id="what-wenlan-is-not"></a>

**適合需要長期延續的工作。** Wenlan 面向研究者、寫作者、顧問、產品團隊與軟體團隊：當知識散落在文件、筆記和 AI 對話裡，它會把這些材料變成可檢查、能隨專案持續改進的頁面，而不是另一份聊天紀錄或孤立的記憶庫。它不以生活管理系統或嵌入其他產品的 memory SDK 為目標。

**記憶保留依據，頁面彙整知識。** 來源保留原始材料；工作中捕獲的決策保留出處，也記錄修正與取代關係。Wenlan 再把兩者整理成附有引用的 Markdown 頁面，讓你反覆使用、更新與審核。

[Karpathy 原始 LLM-wiki](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) 提出三層基礎：不可變的**來源（Sources）**、由 AI 維護的 Markdown **Wiki**，以及會隨你和 AI 一起演進、規定組織與維護方式的 **Schema（規則層）**。Wenlan 實作其中從來源到持續維護頁面的基礎，並以明確的記憶類型與內建規則落實頁面結構、出處、引用、更新、歸屬和審核。

[Rohitg00 的 llm-wiki v2](https://gist.github.com/rohitg00/2067ab416f7bbe447c1977edaaa681e2) 加入了記憶生命週期。Wenlan 把這個方向做成可以直接使用的產品：可追溯的來源、由 AI agent 依 Zettelkasten（卡片盒筆記法）捕獲的原子記憶（每條只表達一個完整想法），以及同時由兩者建立並持續維護的頁面。

**Wenlan 最獨特的做法：** 來源與原子記憶都會成為持續維護頁面的依據。記憶歷史保留知識如何改變；頁面歷史說明目前結論建立在什麼之上。機器維護的頁面可以依目前依據重建；對人工文字的改動則成為可審核的修訂，不會直接覆蓋。

<p align="center">
  <img src="./docs/assets/feature-reel.gif" alt="Wenlan feature reel，展示有來源支撐的頁面、來源檢查、graph context、agent capture 與 curation。" width="100%">
</p>

### 知識如何持續複利

來源文件與匯入的對話會保留為來源紀錄。工作中捕獲的決策、經驗與修正則成為原子記憶，並保留出處、可信度、穩定度、更正與取代關係。

隨著內容增長，人物、概念、決策與來源之間的連結會縮小後續工作需要讀取的範圍，也讓相關依據更容易彼此比較與校正。

記憶檢索以倒數排名融合（RRF）整合 FTS5 關鍵字搜尋與基於 embedding 的向量搜尋，並可沿知識圖譜的實體與關係補充相連依據。啟用頁面檢索通道後，持續維護的頁面會獨立檢索，並與相關記憶一同回傳，範圍受所選空間（Space）限定。可選的進階通道還支援有邊界的多跳圖遍歷與情節記憶檢索。

Wenlan 把相關來源與記憶彙整成附有引用的 Markdown 頁面。即使換了 AI 工具，也能透過頁面、搜尋與 `/recall` 把最新知識帶回工作；`/brief` 只是可選的會話開始彙整入口。新材料會繼續改善同一個頁面，而不是又產生一個互不相連的答案。

頁面和 session notes 都以純 Markdown 保存在 `~/.wenlan/`。Distill 與 handoff workflows 可以把檔案的邏輯批次提交到本地 git repository，留下可檢查、可攜帶的歷史。

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

### 兩套生命週期，一個持續維護的知識系統

一次生成的 wiki 會過時；只存記憶又容易碎成互不相連的事實。Wenlan 連結兩套生命週期，但不把它們混成同一層。

<p align="center">
  <picture>
    <source media="(max-width: 600px)" srcset="./docs/assets/wenlan-lifecycle-zh-Hant-mobile.png">
    <img src="./docs/assets/wenlan-lifecycle-zh-Hant.png" alt="明確取代舊說法的新記憶仍會保留前後關聯。頁面過時後，Wenlan 會依目前來源與記憶重建、記錄修訂，並把對人工文字的改動變成審核提案。" width="100%">
  </picture>
</p>

#### 原子記憶

`CAPTURE -> CLASSIFY -> ENRICH -> LINK -> RECONCILE`

Capture 與明確的 supersession 屬於核心流程。模型支援的階段只會在設定相應模型後執行，Reconcile 預設關閉。

| 操作 | Wenlan 做什麼 |
|---|---|
| **Capture** | AI agent 每次寫入一條完整、自足的想法，遵循 Zettelkasten 的原子筆記原則，而不是保存整段對話。 |
| **Classify** | 設定本地模型後，Wenlan 將記憶分為 `identity`、`preference`、`decision`、`lesson`、`gotcha` 或 `fact`；呼叫端明確提供的準確類型優先。 |
| **Enrich** | 設定本地模型後，在可用時補充結構化欄位、檢索提示、事件日期、品質、重要性與標籤。 |
| **Link** | 保留出處；啟用 enrichment 後，把記憶連結到知識圖譜中的實體與關係。 |
| **Reconcile** | 明確取代舊說法時保留 `supersedes` 鏈。可選的本地模型流程可以把受保護內容的衝突放入審核，而不是覆蓋歷史；它預設關閉，必須明確啟用。 |

進階設定：使用 `WENLAN_ENABLE_DUAL_POOL_RESOLVE=1` 啟用這個 Reconcile 流程。

#### 持續維護的頁面

`DISTILL -> CITE -> TRACK -> REFRESH -> REVIEW`

| 操作 | Wenlan 做什麼 |
|---|---|
| **Distill** | 把相關來源與記憶彙整成一個 Markdown 頁面。 |
| **Cite** | 保留引用紀錄與驗證狀態；自動 refresh 若未通過引用支撐檢查，就會捨棄草稿。 |
| **Track** | 記錄哪些證據支撐頁面、頁面為何過時，以及有上限的變更紀錄。 |
| **Refresh** | 頁面被標記為過時後，依目前證據重建符合條件、由機器維護的頁面。 |
| **Review** | 對你編輯過的頁面提出修訂，而不是靜默改寫。 |

例如，匯入一份設計文件，再讓 Codex 捕獲一次除錯決策。Wenlan 可以把兩者整理成一個同時引用兩份依據的頁面。這個頁面 refresh 時，會依目前依據重建；如果你已經編輯過它，改動提案會等待審核。

---

<a id="what-you-get"></a>
<a id="what-can-it-do"></a>

## 能力

- **持續維護的知識：** Wenlan 把相關來源與記憶彙整成附有引用的頁面，追蹤支撐內容何時改變，並依目前依據更新符合條件、由機器維護的頁面。
- **有證據支撐的知識：** 捕獲的記憶保留 source agent、confidence、stability 與 supersession；經過 distill 或 refresh 的 Pages 保留對來源紀錄與記憶的連結、citation records 與 verification status、stale reasons、ownership 與 revision state。
- **持續維護、可審核的頁面：** 自動 re-distill refresh 無法通過 citation verification gate 時會 fail closed。對使用者擁有頁面的更新會成為 pending revision，不會靜默改寫。
- **可選的衝突審核：** 明確啟用本地 Reconcile 流程後，受保護 memory 的衝突可以進入 review，而不會把每條 capture 都變成審批任務。
- **混合關聯檢索：** 記憶搜尋以倒數排名融合整合 FTS5 與基於 embedding 的向量搜尋，可補充知識圖譜關聯的依據，也可選擇本地 cross-encoder reranker。頁面通道啟用後，會獨立檢索相關頁面，再與記憶一起回傳。
- **一個本地 Rust 服務：** 桌面 app、CLI 與 MCP 用戶端使用同一個 daemon，因此在一個工具捕獲的知識可以在另一個工具回來。安裝為受管理的背景服務是明確選擇；啟用後，不必保持任何用戶端視窗開啟，它也能持續執行已設定的擷取、補充、圖譜連結、引用與符合條件的頁面維護。退出 Wenlan 仍會將它完整關閉。
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

1. **找到最新知識。** 開啟相關 Page、搜尋，或使用 `/recall <query>`；`/brief [topic]` 可選擇性彙整更完整的 session-start context。其他 AI 工具可使用等價的 page、search、recall 與 context 工具。
2. **工作時隨手保存與查找。** `/capture <thing>` 保存決策、經驗、踩坑或事實，並記錄來源。`/recall <query>` 只取回相關內容，不載入全部歷史。
3. **閉合循環。** `/handoff` 記錄改動與待辦，也指出下次工作的起點。
4. **讓 wiki 保持最新。** `/distill` 主動建立或刷新頁面。可選的模型流程會在兩次工作之間補充已保存內容、連結相關知識，並刷新符合條件的頁面。`/lint` 檢查知識庫健康狀態；`/curate` 讓你審核頁面更新提案，以及可選 Reconcile 流程產生的衝突項目。

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

歡迎 bug fixes、eval cases、文件與功能。安裝 Wenlan 不需要從原始碼建置；本地開發主要使用下面兩組命令：

```bash
# Runtime、CLI 與 MCP（本 repository）
cargo build --workspace
cargo test --workspace

# 桌面 app（7xuanlu/wenlan-app）
pnpm install
pnpm tauri dev
pnpm build:all
```

需要從全新的 daemon 開始執行 app 時，請在 app repository 使用 `pnpm dev:all`。完整開發流程見本 repository 的 [AGENTS.md](AGENTS.md) 與 [CONTRIBUTING.md](CONTRIBUTING.md)，以及 [wenlan-app 的 AGENTS.md](https://github.com/7xuanlu/wenlan-app/blob/main/AGENTS.md)。安全性問題請見 [SECURITY.md](SECURITY.md)，也請閱讀 [Code of Conduct](CODE_OF_CONDUCT.md)。

---

<a id="license"></a>

## 授權

Wenlan 採用 **Apache-2.0** 授權，包括本 repository 內的 local runtime、CLI、MCP server、shared types，以及 Claude Code/Codex plugin files。

---

<a id="acknowledgments"></a>

## 源流與同類專案

Wenlan（文瀾）的名字來自文瀾閣。這座皇家藏書樓收藏《四庫全書》，曾是中國最大的藏書之一。

Wenlan 的 llm-wiki v2 模型是自己的產品方向，並受到 LLM-wiki 與 agent-memory 兩條脈絡啟發：

- [Karpathy 的 LLM-wiki note](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) 建立了從 raw sources 到持續維護 wiki 的模式。
- [Rohitg00 的 LLM Wiki v2 proposal](https://gist.github.com/rohitg00/2067ab416f7bbe447c1977edaaa681e2) 加入 memory lifecycle、confidence、graph 與 retrieval mechanisms。[agentmemory](https://github.com/rohitg00/agentmemory) 是其具體的 agent-memory implementation。
- [nashsu/llm_wiki](https://github.com/nashsu/llm_wiki) 是以文件為核心的 LLM-wiki 完整桌面實作。
- [basic-memory](https://github.com/basicmachines-co/basic-memory)、[obsidian-mind](https://github.com/breferrari/obsidian-mind)、[mcp-memory-service](https://pypi.org/project/mcp-memory-service/)、[Memoria](https://github.com/matrixorigin/Memoria) 和 [OpenMemory](https://github.com/CaviraOSS/OpenMemory) 探索相鄰的本地知識與 agent-memory 方向。
