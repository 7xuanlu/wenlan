<!-- README_SYNC: source=README.md sha256=6de841b549c9e884e6ed47cf35a13226c270bcb81d348d58fcd7eed7897057cc -->

<p align="center">
  <picture>
    <source media="(max-width: 600px)" srcset="./docs/assets/readme-banner-zh-Hant-mobile.png">
    <img src="./docs/assets/readme-banner-zh-Hant.png" alt="Wenlan：持續更新的個人維基。AI 幫你整理，你保有主導權。" width="100%">
  </picture>
</p>

Wenlan 將筆記、文件與 AI 對話整理成可編輯且連回來源的頁面，讓你和已連接的 AI 工具能延續已有知識。

<p align="center">
  <a href="./README.md">English</a> | <a href="./README.zh-Hans.md">简体中文</a> | 繁體中文 | <a href="./README.es-ES.md">Español</a>
</p>

<p align="center">
  <a href="https://github.com/wenlan-app/wenlan/actions/workflows/ci.yml?query=branch%3Amain"><img alt="CI" src="https://github.com/wenlan-app/wenlan/actions/workflows/ci.yml/badge.svg?branch=main&event=push"></a>
  <a href="https://github.com/wenlan-app/wenlan/releases/latest"><img alt="最新版本" src="https://img.shields.io/github/v/release/wenlan-app/wenlan?sort=semver&label=release"></a>
  <a href="#license"><img alt="授權：Apache-2.0 與 AGPL-3.0" src="https://img.shields.io/badge/license-Apache--2.0%20%2B%20AGPL--3.0-blue.svg"></a>
</p>

<p align="center">
  <a href="#start-in-30-seconds">開&#8288;始&#8288;使&#8288;用</a> ·
  <a href="#what-can-i-bring-in">瀏&#8288;覽&#8288;與&#8288;整&#8288;理</a> ·
  <a href="#retrieval">搭&#8288;配&#8288;AI</a> ·
  <a href="#two-lifecycles">持&#8288;續&#8288;更&#8288;新</a> ·
  <a href="#learn-more">文&#8288;件&#8288;與&#8288;社&#8288;群</a> ·
  <a href="https://discord.gg/nmzVTTU3HG">Discord</a>
</p>

<!-- Replace the linked walkthrough when the separately produced release video is ready. -->
<p align="center">
  <a href="https://github.com/user-attachments/assets/e4c934f9-c284-480f-a20f-708d9c91748d">
    <img src="./docs/assets/wenlan-personal-knowledge-zh-Hant.png" alt="Wenlan 中彼此連結的頁面、記憶與來源" width="100%">
  </a>
  <br><a href="https://github.com/user-attachments/assets/e4c934f9-c284-480f-a20f-708d9c91748d">▶ 觀看影片</a>
</p>
<p align="center">封面由多個實機畫面與示範資料組合；圖譜為示意，並非 App 原生的三欄畫面。</p>

<a id="quickstart"></a>
<a id="start-in-30-seconds"></a>
<a id="start-with-the-app"></a>
<a id="open-the-wiki"></a>
<a id="desktop-app"></a>
<a id="claude-code-in-30-seconds"></a>
<a id="codex-plugin"></a>
<a id="mcp-setup"></a>
<a id="mcp-clients"></a>

## 開始使用

### 1. 下載並開啟 Wenlan

[下載桌面版](https://github.com/wenlan-app/wenlan/releases/latest)，安裝後開啟：

- **macOS（Apple Silicon）：** 打開 `.dmg`，把 Wenlan 拖進「應用程式」。App 已簽署並通過公證。
- **Windows x64：** 執行 `-setup.exe`，安裝包尚未簽署。若 SmartScreen 警告，先確認檔案來自官方 Releases 頁面，再選「其他資訊」→「仍要執行」。
- **Linux：** 暫無桌面版；請依[AI 設定指南](docs/setup-with-ai.md#install-the-runtime)安裝 runtime。

首次啟動會下載本地搜尋模型，設定完成前請保持連網。[下載與隱私說明](docs/PRIVACY.md#when-wenlan-reaches-the-network)。

### 2. 開啟範例或建立頁面

選擇「開啟筆記」進入 Wenlan，可先看歡迎畫面的範例，或建立自己的頁面。

### 3. 想使用時再連接 AI 工具

需要 AI 使用你保存的知識時，再連接 Claude Code、Codex 或其他支援的用戶端。請依[AI 設定指南](docs/setup-with-ai.md)操作。

> 參考我之前的筆記，幫我接著完成這個任務。

<a id="what-does-wenlan-build"></a>
<a id="why-it-compounds"></a>
<a id="這是什麼"></a>
<a id="what-is-this"></a>
<a id="what-you-get"></a>
<a id="what-can-it-do"></a>
<a id="what-can-i-bring-in"></a>
<a id="knowledge-graph"></a>

## 瀏覽與整理個人知識

來源是帶入 Wenlan 的原始筆記、文件與對話。記憶保存其中有用的事實與決定；頁面整理相關知識，並連回來源。

- **檔案與對話匯入**：匯入 Markdown、文字檔、文字型 PDF、資料夾，以及 ChatGPT 或 Claude 匯出的 ZIP，保留來源參照。
- **Obsidian 唯讀連接**：讀取既有儲存庫並按需重新整理，不修改原始檔案，也不將變更同步回去。
- **Quick Capture 與原子記憶**：在 App 或透過 AI 保存一則則決定、經驗與偏好，並保留其來源。
- **附引用的 Markdown 頁面**：將相關來源與記憶整理成可編輯的頁面，附上引用並以 `[[wikilinks]]` 連到其他頁面。
- **知識圖譜與社群分群**：設定整理模型後，從記憶擷取具類型的實體與關係、辨識重複提及，並將相連實體聚成社群。

掃描版 PDF 須先擷取文字再匯入。

### 探索你的個人知識圖譜

<p align="center">
  <img src="./docs/assets/wenlan-personal-graph-zh-Hant.png" alt="Wenlan 實機知識圖譜，搭配個人興趣、研究與開發的區域示意" width="100%">
</p>
<p align="center">實機節點與連線；區域名稱與範圍為示意。</p>

<a id="what-wenlan-is-not"></a>
<a id="retrieval"></a>
<a id="從關鍵字語意與關聯找回正確內容"></a>
<a id="retrieval-across-words-meaning-and-connections"></a>

## 用 AI 延續已有知識

- **MCP 與 AI 插件**：Claude Code、Codex 與其他已連接的用戶端，透過 Wenlan 工具讀寫同一份本地知識。
- **本地混合檢索（Hybrid Retrieval）**：以加權 RRF 合併 SQLite FTS5 全文搜尋與 BGE 向量檢索，基礎搜尋不需 API key。
- **圖譜輔助排序**：有符合條件的實體連結時，提高相關候選記憶的排名，也可選用 cross-encoder reranking 精排結果。
- **記憶擷取與工作交接**：用 `/capture` 保存決定與脈絡，用 `/handoff` 留下進度和待解問題，供下一段工作接續。
- **Spaces 範圍檢索**：選定 Space，將回傳的知識限制在相關專案或個人資料集合內。

<p align="center">
  <img src="./docs/assets/wenlan-knowledge-to-work-zh-Hant.png" alt="AI 接著做，不必從頭交代：關鍵詞、語義與圖譜檢索" width="100%">
</p>
<p align="center">虛構專案與對話的概念示意，並非產品測試。差異在於加入文瀾脈絡，而非 Claude 沒有原生記憶。</p>

[搜尋方式](docs/technical-foundations.md#retrieval-pipeline)。

實驗中的網頁存取功能可讓支援的用戶端搜尋整個資料庫或指定空間，但執行 Wenlan 的電腦必須保持連線。查詢與結果會經過中繼服務，詳見[隱私界限](docs/PRIVACY.md#pre-release-standalone-wenlan-relay-connector)。

<a id="what-makes-wenlan-distinct"></a>
<a id="why-is-wenlan-different"></a>
<a id="two-lifecycles"></a>
<a id="two-lifecycles-one-maintained-knowledge-system"></a>
<a id="兩套生命週期一個持續維護的知識系統"></a>
<a id="local-markdown"></a>
<a id="原子記憶"></a>
<a id="持續維護的頁面"></a>
<a id="與-obsidian-共存的本地-markdown"></a>

## 持續更新知識並保留歷史

- **依據追蹤與頁面更新**：追蹤支撐頁面的來源與記憶，讓過時頁面可根據最新依據重新整理。
- **Steep 背景整理**：設定[模型](docs/setup-and-data.zh-Hant.md#models)後，補上記憶類型、結構化資訊、搜尋線索與圖譜連結，並可選用頁面合成。
- **修訂審核與歷史**：自動更新你編輯過的頁面時先提出修訂，並保留可查看的頁面版本與明確的記憶取代鏈。
- **本地 Markdown 與維基匯出**：頁面保留為一般 Markdown 檔，可將符合條件的頁面匯出為 OKF 維基，記憶與圖譜則保存在本地 libSQL。
- **自由選擇模型執行方式**：以 llama.cpp 執行裝置端 Qwen，或連接 Ollama、LM Studio 等 OpenAI 相容本地服務及已設定的雲端供應商，處理整理與頁面合成。

<p align="center">
  <img src="./docs/assets/wenlan-updates-history-zh-Hant.png" alt="示意流程：文件來源中的發現將記憶由 V1 更新為 V2，再影響自動更新或待審修訂的知識頁。" width="100%">
</p>
<p align="center">實機畫面搭配示範資料；連線與標註為示意。</p>

自動更新若引用不足，就會拒絕草稿；直接改檔與強制重建遵循不同的審核規則。

[更新與審核](docs/knowledge-guide.zh-Hant.md#updates-and-history) · [Doctor 與問題回報](https://wenlan.app/docs/diagnostics-and-issue-reports)。

<a id="how-wenlan-works"></a>
<a id="how-does-it-work"></a>

## 簡單工作流程

找出已有知識 → 保存有用決定 → 整理成頁面 → 下次接著使用。安裝 Wenlan 插件即可使用 `/recall`、`/capture`、`/distill`、`/handoff`（[指令說明](plugin/skills/README.md)）。

<a id="models-and-privacy"></a>

### 資料與隱私

頁面與工作筆記是 Markdown 檔案；記憶與圖譜資料保存在本地資料庫。雲端 AI 用戶端可能會把找回的知識傳給其服務商，雲端模型也會收到它處理所需的內容。本地儲存不代表這些互動都留在本機。使用統計預設關閉。

[檔案位置、備份與移除](docs/setup-and-data.zh-Hant.md#backup-and-removal) · [連網與隱私詳情](docs/PRIVACY.md#when-wenlan-reaches-the-network)。

<a id="evaluation"></a>

## 評估

這些結果用來檢查搜尋能否找回相關知識，不代表 AI 回答的品質。

<details>
<summary>搜尋指標</summary>

<!-- EVAL_SNAPSHOT_START -->
| Benchmark | Recall@5 | MRR | NDCG@10 |
|---|---:|---:|---:|
| LME_Oracle (500 Q) | 93.6% | 0.857 | 0.883 |
| LME_S (deep, 90 Q) | 87.7% | 0.815 | 0.822 |
<!-- EVAL_SNAPSHOT_END -->

[評估方法與基準詳情](docs/eval/README.md)。

</details>

<a id="learn-more"></a>

<a id="離線佇列outbox"></a>

## 進一步了解

從安裝與連接開始，再查閱各項功能的詳細指南：

- [開始使用與安裝](docs/setup-and-data.zh-Hant.md#installation)
- [連接 AI 工具](docs/setup-with-ai.md)
- [來源與匯入](docs/knowledge-guide.zh-Hant.md#sources-and-pages) · [本地檔案、備份與離線使用](docs/setup-and-data.zh-Hant.md)
- [隱私](docs/PRIVACY.md)
- [完整文件索引](docs/README.md)

## 貢獻

歡迎加入 [Discord 社群](https://discord.gg/nmzVTTU3HG)。請在 [GitHub issues](https://github.com/wenlan-app/wenlan/issues) 回報問題或提出功能建議；Issue 是公開的，分享前請遮蔽密鑰與個人資料。程式碼與文件貢獻請見[貢獻指南](.github/CONTRIBUTING.md)、[安全性回報](.github/SECURITY.md)與[行為準則](.github/CODE_OF_CONDUCT.md)。

<a id="code-signing-policy"></a>

## 程式碼簽章政策

請見[各平台簽章與公證說明](docs/code-signing.md)。

<a id="license"></a>

## 授權

本地 runtime、CLI、MCP server、shared types 與 plugins 採用 [Apache-2.0](LICENSE)。桌面 app 與其前端採用 [AGPL-3.0-only](app/LICENSE)；若將修改版作為網路服務運行，須向使用者提供其原始碼。

<a id="acknowledgments"></a>

## 源流與同類專案

Wenlan 名稱來自文瀾閣，產品方向承接 LLM-wiki 與 agent-memory 脈絡：[Karpathy 的 LLM-wiki note](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)、[Rohitg00 的提案](https://gist.github.com/rohitg00/2067ab416f7bbe447c1977edaaa681e2) 與 [nashsu/llm_wiki](https://github.com/nashsu/llm_wiki)。
