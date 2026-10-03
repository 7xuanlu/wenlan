<!-- README_SYNC: source=README.md sha256=d37b170604ca1a51807bb44de4d33297cf3dc83091c38c15baa8217961ce4ef3 -->

<p align="center">
  <picture>
    <source media="(max-width: 600px)" srcset="./docs/assets/readme-banner-zh-Hans-mobile.png">
    <img src="./docs/assets/readme-banner-zh-Hans.png" alt="Wenlan：持续更新的个人维基。AI 帮你整理，你保有主导权。" width="100%">
  </picture>
</p>

Wenlan 把你的文档、笔记和 AI 对话整理成可编辑、附有来源链接的页面，让你和 AI 工具在已有成果上继续工作。

来源有变，AI 跟着更新页面。如果你编辑过页面，Wenlan 会提出修订，让你确认是否采用，而不是自动覆盖你的内容。

<p align="center">
  <a href="./README.md">English</a> | 简体中文 | <a href="./README.zh-Hant.md">繁體中文</a> | <a href="./README.es-ES.md">Español</a>
</p>

<p align="center">
  <a href="https://github.com/7xuanlu/wenlan/actions/workflows/ci.yml?query=branch%3Amain"><img alt="CI" src="https://github.com/7xuanlu/wenlan/actions/workflows/ci.yml/badge.svg?branch=main&event=push"></a>
  <a href="https://github.com/7xuanlu/wenlan/releases/latest"><img alt="最新版本" src="https://img.shields.io/github/v/release/7xuanlu/wenlan?sort=semver&label=release"></a>
  <a href="#license"><img alt="许可证：Apache-2.0 与 AGPL-3.0" src="https://img.shields.io/badge/license-Apache--2.0%20%2B%20AGPL--3.0-blue.svg"></a>
</p>

<p align="center">
  <a href="#start-in-30-seconds">开&#8288;始&#8288;使&#8288;用</a> ·
  <a href="#what-does-wenlan-build">这&#8288;是&#8288;什&#8288;么？</a> ·
  <a href="#what-can-it-do">能&#8288;力</a> ·
  <a href="#how-does-it-work">日&#8288;常&#8288;流&#8288;程</a> ·
  <a href="#evaluation">评&#8288;估</a> ·
  <a href="#learn-more">进&#8288;一&#8288;步&#8288;了&#8288;解</a>
</p>

https://github.com/user-attachments/assets/2c91437c-59f5-44af-a5e1-da7627b921ad

<p align="center">
  <sub>桌面 app 中持续维护的页面：打开任意引用，就能检查这条结论背后的来源或记忆。</sub>
</p>

<a id="quickstart"></a>
<a id="start-in-30-seconds"></a>

## 开始使用

<a id="start-with-the-app"></a>
<a id="open-the-wiki"></a>
<a id="desktop-app"></a>

### 1. 下载并打开 Wenlan

[下载桌面版](https://github.com/7xuanlu/wenlan/releases/latest)，安装后打开：

- **macOS（Apple Silicon）：** 打开 `.dmg`，把 Wenlan 拖进「应用程序」。App 已签名并通过公证。
- **Windows x64：** 运行 `-setup.exe`。安装包尚未签名。如果 SmartScreen 显示警告，请先确认文件来自官方 Releases 页面，再点「更多信息」→「仍要运行」。
- **Linux：** 暂时没有桌面版；可按照[设置指南](docs/setup-with-ai.md#install-the-runtime)，不通过桌面 app，直接搭配 AI 工具使用。

<a id="claude-code-in-30-seconds"></a>

<a id="codex-plugin"></a>

<a id="mcp-setup"></a>
<a id="mcp-clients"></a>

### 2. 让常用 AI 共用你的维基

在 Wenlan 的设置引导中，连接 Claude Code、Codex 等工具，让它们使用同一份知识。

<details>
<summary>连接时需要帮助？让 AI 协助设置</summary>

如有提示，请重新启动 AI 工具。需要协助设置时，把下面这段贴给 Claude Code、Codex，或其他能够读取设置指南的工具：

```text
请按照这份指南，将当前的 AI 工具连接到 Wenlan：
https://raw.githubusercontent.com/7xuanlu/wenlan/main/docs/setup-with-ai.md

如果已安装 Wenlan，请沿用现有安装。
只设置当前这个 AI 工具，并确认它能保存及找回一条测试记忆。
```

</details>

### 3. 留下成果，下次接着用

> 把这次讨论的重点整理成 Wenlan 页面。

<details>
<summary>模型、其他安装方式与更新</summary>

**模型**

你可以请已连接的 AI 整理页面。要让 Wenlan 自己在后台整理，则需要[配置模型](#models-and-privacy)。

**从终端安装 macOS app**

安装程序会下载 app、核对 SHA-256，并放进「应用程序」：

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/7xuanlu/wenlan/main/scripts/install-macos-app.sh)"
```

**不使用桌面 app**

在 macOS Apple Silicon 上运行：

```bash
npx -y wenlan setup
```

`npx` 需要 Node.js；若未安装，可先运行 `curl -fsSL https://raw.githubusercontent.com/7xuanlu/wenlan/main/install.sh | bash`，再运行 `wenlan setup --basic`。

这个命令会下载预编译的 CLI、后台服务（daemon）与 MCP 连接器，启动并验证本地服务；不需要安装 Rust 或 Cargo。使用 glibc 的 Linux x64/ARM64 可以采用自动化的 [shell 设置流程](docs/setup-with-ai.md#install-the-runtime)；Windows x64 请从 [Releases](https://github.com/7xuanlu/wenlan/releases/latest) 下载对应的 archive。macOS Intel 目前[没有受支持的完整 runtime 安装方式](crates/wenlan-cli/README.md#macos-intel)。

**安装内容与更新方式**

桌面 app 内置 daemon、CLI 与 MCP 连接器，打开时会启动 daemon，并提供检测到的 AI 工具接入选项：Claude Code、Codex 使用 plugin，其他支持工具使用 MCP 设置。不使用桌面 app 时，运行的也是同一个 daemon；两种方式都让 AI 工具访问同一个本地知识库。

更新 macOS app 时，把新 app 拖到旧 app 上覆盖并打开。Wenlan 0.17.0 及更早的版本需要先手动退出。

手动与各工具设置说明：[AI 辅助设置](docs/setup-with-ai.md) · [Claude Code plugin](plugin/README.md) · [Codex plugin](plugin-codex/README.md) · [CLI 与 MCP](crates/wenlan-cli/README.md)。

</details>



<a id="what-does-wenlan-build"></a>
<a id="why-it-compounds"></a>

<a id="这是什么"></a>

## 你与 AI 共用的个人维基

- **下次接着做。** 请 Claude Code 或 Codex 参考已有的 Wenlan 页面，继续下一项工作。
- **看得到依据。** 点击引用，查看原始文档、对话或决策记录。
- **自己的修改，自己决定。** 自动更新你编辑过的页面时，Wenlan 会提出修订，交给你确认。

页面是本地 Markdown 文件，你可以阅读、编辑，也能带走。

<p align="center">
  <picture>
    <source media="(max-width: 600px)" srcset="./docs/assets/wenlan-system-zh-Hans-mobile.png">
    <img src="./docs/assets/wenlan-system-zh-Hans.png" alt="来源与记忆分别支撑同一个持续维护的页面。页面过时后，Wenlan 可以依当前依据重建；可选的冲突审核可以让受保护内容的冲突浮现，对人工文字的改动则等待用户判断。" width="100%">
  </picture>
</p>

<details>
<summary>来源、记忆与页面如何协作</summary>

Wenlan 让持续进行的工作不只留在聊天窗口里。你可以保存选定的文档和对话，记下过程中的决定，再把它们整理成可阅读、编辑和重用的页面。页面生成与后台维护需要先配置[AI 路径](#models-and-privacy)。

<a id="what-wenlan-is-not"></a>

**适合需要长期延续的工作。** 如果你连续几天或几周用 AI 处理同一个主题，却常常要翻找之前的资料、重新解释已经做出的决定，Wenlan 就是为这种工作流程设计的。它不是生活管理系统，也不是嵌入其他产品的 memory SDK。你仍可继续使用 Obsidian；Wenlan 不承诺替代它的插件或迁移 vault 的所有功能。

**一个知识系统，三种角色：**

- **来源让 Wenlan 读到的材料始终可追溯。** 导入的对话保留为捕获时的记录；已注册文件会随内容变化同步当前版本。
- **记忆保留工作真正教会你的内容。** AI agent 捕获原子的决策、经验、修正与取代关系，并保留出处。
- **页面汇总当前知识。** Wenlan 把相关来源与记忆整理成带引用的 Markdown，让你反复使用、刷新与审核。

**更新方式：** 来源与捕获的记忆都可以作为同一页面的依据。记忆历史记录各项记忆的变化；页面历史则记录支撑页面的依据与修订。自动更新时，符合条件、由系统维护的页面可以直接更新；你编辑过的页面则会收到修订提案。审核让你决定是否应用更新，但不代表 AI 的结论一定正确。

技术读者可参考：Wenlan 遵循 LLM wiki 模式。数据模型、检索与维护规则请见 [LLM-wiki 实现指南](https://wenlan.app/zh-CN/learn/distilled-wiki-pages-ai-memory) 和[技术基础](docs/technical-foundations.md)。

</details>

<a id="knowledge-graph"></a>

### 越用越有价值的知识图谱

<p align="center">
  <picture>
    <source media="(max-width: 600px)" srcset="./docs/assets/wenlan-knowledge-network-zh-Hans-mobile.png">
    <img src="./docs/assets/wenlan-knowledge-network-zh-Hans.png" alt="Wenlan 连接式知识系统的概念图：知识页面、来源页面、原子记忆与实体通过页面链接、依据、记忆到实体的连接和实体关系互相连接。" width="100%">
  </picture>
</p>

<details>
<summary>图谱与搜索的技术细节</summary>

实体关系图谱只是 Wenlan 更大连接式 wiki 的一部分。**知识页面**保留持续维护的结论，**实体**固定可复用的人物、项目与概念，**来源页面**让导入或同步的材料可检查，原子**记忆**则保留决策与变化。它们通过彼此分开的明确连接协作：页面间的 wikilink、页面依据、记忆到实体的连接，以及实体间的有向关系。

在实体图谱这一层，配置 enrichment 模型后，Wenlan 会从记忆中提取带类型的实体、观察与有方向的关系。实体链接与解析会复用已有节点，而不是把每次提及都当成新事物；每条记忆仍保留来源，并可连接多个实体。[查看连接模型如何存储 ->](docs/technical-foundations.md#connected-knowledge-model)

- **含义与方向：** 关系使用 `uses`、`part_of`、`contradicts`、`replaced_by` 等预置词汇；未知类型会回退为 `related_to`，并成为可审核的词汇提案。
- **强度与出处：** 关系可以保存置信度、解释与对应的来源记忆，让强弱不同的主张仍可区分、可检查。
- **形成可复用群组：** 标签传播会依关系密度为实体分组，并按每对实体之间的关系数量加权。这些群组可组织可选的全局摘要，实体链接也会为检索补充脉络。
- **修正但不抹除：** 相关说法、修正与明确的取代关系可以放在一起检查，原始来源与记忆历史仍会保留。

检索时，Wenlan 会用实体向量匹配找到与问题相关的实体。存在符合条件的图谱链接时，默认开启的图谱记忆信号（graph-memory stream）会把相连记忆作为第三路 [RRF](https://cormack.uwaterloo.ca/cormacksigir09-rrf.pdf) 排名信号加以提升。这个路径取决于现有图谱数据与读取范围，Space 边界仍然有效。[查看图谱检索如何工作 ->](docs/technical-foundations.md#graph-assisted-retrieval)

<a id="retrieval"></a>

### 从关键词、语义与关联找回正确内容

Wenlan 的核心搜索是本地混合检索流程，不是单一的向量查询。每个阶段负责不同工作：

- **原词匹配，[SQLite FTS5](https://www.sqlite.org/fts5.html)：** 全文索引查找字面关键词、标识符与短语。
- **相近含义，FastEmbed + [`Qdrant/bge-base-en-v1.5-onnx-Q`](https://huggingface.co/Qdrant/bge-base-en-v1.5-onnx-Q)：** 量化的英文模型会产生 768 维语义向量；[libSQL cosine DiskANN](https://turso.tech/blog/approximate-nearest-neighbor-search-with-diskann-in-libsql) 再以近似最近邻搜索（ANN）快速取得候选。
- **合并排名，加权 [RRF](https://cormack.uwaterloo.ca/cormacksigir09-rrf.pdf)（`k = 60`）：** 融合原词与语义排名，不假设两者的原始分数采用同一尺度；向量信号还会由余弦相似度加权。
- **关联脉络，图谱记忆信号（graph-memory stream）：** 符合条件的实体链接会加入第三路 RRF 信号，返回的记忆仍受当前读取范围限制。
- **可选精排，交叉编码器（cross-encoder）：** 与分别编码查询和记忆的 embedding 不同，[`jinaai/jina-reranker-v1-turbo-en`](https://huggingface.co/jinaai/jina-reranker-v1-turbo-en) 或 [`BAAI/bge-reranker-base`](https://huggingface.co/BAAI/bge-reranker-base) 会同时读取查询与单个候选，再对较小的候选池重新排名；默认关闭。

页面、情节记忆与事实（fact）通道都需要主动启用；不可用时会退回其余搜索信号。Space 仍负责限制读取范围。[查看方法、默认值与限制 ->](docs/technical-foundations.md)

</details>

<a id="what-makes-wenlan-distinct"></a>
<a id="why-is-wenlan-different"></a>
<a id="two-lifecycles"></a>
<a id="two-lifecycles-one-maintained-knowledge-system"></a>
<a id="两套生命周期一个持续维护的知识系统"></a>

### 知识会改变，历史仍会保留。

一次生成的 wiki 会过时；只存记忆又容易碎成互不相连的事实。Wenlan 连接两套生命周期，但不把它们混成同一层。

<p align="center">
  <picture>
    <source media="(max-width: 600px)" srcset="./docs/assets/wenlan-lifecycle-zh-Hans-mobile.png">
    <img src="./docs/assets/wenlan-lifecycle-zh-Hans.png" alt="明确取代旧说法的新记忆仍会保留前后关联。页面过时后，Wenlan 会依当前来源与记忆重建、记录修订，并把对人工文字的改动变成审核提案。" width="100%">
  </picture>
</p>

<details>
<summary>更新、审核与本地文件的细节</summary>

#### 原子记忆

`CAPTURE -> CLASSIFY -> ENRICH -> LINK -> RECONCILE`

Capture 与明确的 supersession 属于核心流程。模型支持的阶段只会在配置相应模型后运行，Reconcile 默认关闭。

| 操作 | Wenlan 做什么 |
|---|---|
| **Capture** | AI agent 每次写入一条完整、自足的想法，遵循 Zettelkasten 的原子笔记原则，而不是保存整段对话。 |
| **Classify** | 配置语言模型后，Wenlan 将记忆分为 `identity`、`preference`、`decision`、`lesson`、`gotcha` 或 `fact`；调用方明确提供的准确类型优先。 |
| **Enrich** | 配置语言模型后，在可用时补充结构化字段、检索提示、事件日期、质量、重要性与标签。 |
| **Link** | 保留出处；启用 enrichment 后，把记忆连接到知识图谱中的实体与关系。 |
| **Reconcile** | 明确取代旧说法时保留 `supersedes` 链。若发起替换的 agent 信任级别低于 full，该替换会自动进入人工审核队列，无需任何开关。可选的模型流程还可以把受保护内容的冲突放入审核，而不是覆盖历史；这个流程默认关闭，必须明确启用。 |

高级设置：使用 `WENLAN_ENABLE_DUAL_POOL_RESOLVE=1` 启用这个 Reconcile 流程。

#### 持续维护的页面

`DISTILL -> CITE -> TRACK -> REFRESH -> REVIEW`

| 操作 | Wenlan 做什么 |
|---|---|
| **Distill** | 把相关来源与记忆汇总成一个 Markdown 页面。 |
| **Cite** | 保留引用记录与验证状态；自动 refresh 若未通过引用支撑检查，就会丢弃草稿。 |
| **Track** | 记录哪些证据支撑页面、页面为何过时，以及有上限的变更记录。 |
| **Refresh** | 页面被标记为过时后，依当前证据重建符合条件、由机器维护的页面。 |
| **Review** | 自动更新时，对你编辑过的页面提出修订，而非静默改写。 |

例如，导入一份设计文档，再让 Codex 记下一项调试决策。Wenlan 可以把两者整理成同一个页面，并引用两者。自动更新时，页面会依据当前材料重建；如果你编辑过它，更新提案会等你审核。

**审核范围：** 这是一项更新政策，不是保护你文件的安全机制。直接编辑文件或通过本地手动编辑 API 所做的修改，不会进入这个审核队列。明确强制重新生成也可能替换已编辑的页面；桌面 app 会在执行前要求确认。

<a id="local-markdown"></a>

### 与 Obsidian 共存的本地 Markdown

长期知识保留为普通文件，不被锁在专有编辑器格式里：

- **纯文本文件：** 页面与 session notes 都以 Markdown 保存在 `~/.wenlan/`。
- **可检查的历史：** Distill 与 handoff 可以把逻辑上属于同一批的文件提交到本地 git repository。
- **与 Obsidian 共存：** Wenlan 把现有 vault 当作来源读取。你可以把 `~/.wenlan/pages/` symlink 到 vault，或从桌面 app 导出页面；你的编辑仍由你拥有，之后的机器更新会成为可审核的修订建议。

本地历史可以直接检查：

```text
$ git -C ~/.wenlan log --oneline
a1b2c3d distill: 4 pages
9f8e7d6 session: embedding-work
```

</details>


<a id="what-you-get"></a>
<a id="what-can-it-do"></a>
<a id="what-can-i-bring-in"></a>

## 能力

### 导入你的资料

- **留下有用的 AI 对话：** 导入 ChatGPT 或 Claude 导出的 ZIP，已导入的对话不会重复加入。
- **导入现有笔记：** 导入 Markdown、文本文件或可提取文本的 PDF，也能批量读取文件夹，或把 Obsidian 仓库接为来源。扫描版 PDF 需先提取文本。
- **快速记录：** 直接在桌面 app 记下想法或决策，不必先打开 AI 对话。
- **[请 AI 记住工作重点](https://wenlan.app/learn/ai-memory-provenance)：** 请 AI 工具记下决策、经验、更正、偏好与事实，保留来源及替代的旧记录。
- **导入其他 wiki：** 通过 CLI 或 API 导入外部 OKF wiki，保留来源引用与链接。目前不支持重新导入 Wenlan 自己导出的 OKF。

### 浏览你的个人 wiki

- **[可编辑、有来源的页面](https://wenlan.app/docs/source-backed-pages)：** 把相关文档与记忆整理成 Markdown 页面，附引用与页面链接。
- **卡片或列表：** 用适合自己的方式浏览页面、实体与空间。
- **[知识图谱](docs/technical-foundations.md#graph-data-and-entity-resolution)：** 探索人物、项目、主张与支撑记忆之间的关系。

### 配合 AI 继续工作

- **[换个 AI，也能接着用](https://wenlan.app/docs/architecture)：** 连接后，Claude Code、Codex 与其他 MCP 客户端，可使用和桌面 app、CLI 相同的本地页面与记忆。
- **[原词与语义搜索](docs/technical-foundations.md#retrieval-pipeline)：** 结合精确匹配与本地语义搜索，图谱关系可补充相关背景。
- **[需要时，搜索得更细](docs/technical-foundations.md#optional-channels-and-defaults)：** 可选择搜索页面与更细致的记忆，并重新排序结果。
- **[不同项目，分开管理](https://wenlan.app/docs/spaces)：** 用空间选择 AI 这次要搜索的工作、个人、客户或代码库知识。
- **网页 AI 访问（实验性）：** 电脑保持联网时，让支持的网页 AI 客户端连接到你授权的一个空间。查询与结果会经过中继服务，详见[连接方式与隐私边界](docs/PRIVACY.md#pre-release-standalone-wenlan-relay-connector)。
- **接入自己的工具：** 通过本地 HTTP API 传入准备好的文本、网页内容或记忆；它接收内容，不会代为抓取网址。

### 持续更新与维护

后台整理与页面更新是可选功能，需要先[配置模型](#models-and-privacy)。

- **增量同步：** 文件与文件夹会在后台追踪变化；Obsidian 仓库保持只读，需要时再同步。
- **[整理已存的知识](docs/technical-foundations.md#typed-memory-schema)：** 配置模型后，可协助补上类型、字段、相关日期、标签、搜索提示与图谱链接。
- **有引用才更新：** 自动更新会拒绝引用不足的草稿。AI 生成的页面可以更新；你编辑过的页面则先提出修订。
- **[需要判断才审核](https://wenlan.app/docs/review-and-trust)：** 审核受保护的冲突、页面修订、实体合并与新词汇。
- **进度看得见：** 在 Activity 查看进度与受阻原因。本地服务运行时，关闭 app 窗口后仍可继续执行已配置的同步、补全与符合条件的页面更新。

### 保有数据与控制权

- **[数据在本地，也能检查](https://wenlan.app/learn/markdown-local-index-ai-memory)：** 保留 Markdown 页面、引用、修订、git 历史与 Obsidian 导出；记忆与图谱存于本地 libSQL。
- **把 wiki 带走：** 从设置或 CLI，把各空间符合条件的页面导出成 OKF v0.2 wiki。这是 wiki 导出，不是完整数据库备份。
- **[模型自己选](docs/technical-foundations.md#model-roles)：** 基础检索留在本机。可选的补全与页面合成能用设备端 Qwen、本地接口或云端模型；远程服务会收到该任务所需的内容。
- **检查问题，确认后修复：** [Doctor](https://wenlan.app/docs/diagnostics-and-issue-reports) 与 [lint](plugin/skills/lint/SKILL.md) 只报告问题，不改写知识。支持的修复可在 app 预览，确认后应用并验证。

**从一段值得留下的对话开始。** [开始使用](#start-in-30-seconds)。想之后再试，也可以先给这个 repo 一颗 Star，方便回来找到它。


<a id="how-wenlan-works"></a>
<a id="how-does-it-work"></a>

## 日常流程

AI 工具连接好后，可以直接这样说：

### 开始工作前

> 找出我存进 Wenlan 的［主题］相关资料，包括之前的决定与来源。

### 得到有用的结论时

> 把这个决定、背后的原因和来源记到 Wenlan。

### 值得整理成页面时

> 把［主题］的已存资料整理成 Wenlan 页面，已有的就更新，并保留引用。

在 Wenlan 里阅读、编辑页面、查看来源。下次做相关工作时，请 AI 先读这一页，再接着做。

<details>
<summary>插件指令与维护</summary>

- **找回背景：** `/recall <query>` 搜索已存的知识。`/brief [topic]` 读取当前空间的项目摘要；加上主题时，会补充同一空间的相关内容。
- **留下重点：** `/capture <thing>` 记下决策、经验、更正、偏好或事实，并保留来源。
- **工作收尾：** `/handoff` 记录这次的进展，创建或更新空间的项目摘要，方便下次继续。
- **整理与审核：** `/distill` 创建或更新 wiki 页面。`/lint` 检查知识库状态；`/curate` 审核待处理的记录或修订。

这些快捷指令由 Wenlan 插件提供。其他已连接的客户端使用对应的 MCP 工具。可选的后台整理与页面更新需要先[配置模型](#models-and-privacy)。

[完整指令参考](plugin/skills/README.md)。

</details>

<details>
<summary>CLI 离线队列详情</summary>

### 离线队列（outbox）

如果本地守护进程无法访问，`wenlan capture` 和 `wenlan brief update` 会把请求写入本地持久化队列（outbox）并正常退出。守护进程恢复后，它会通过常规 HTTP 路由排空这些写入；用 `wenlan outbox status` 查看队列，或用 `wenlan outbox drain` 立即重放。被守护进程直接拒绝的写入（4xx，例如未通过内容质量检查）会带着回执移动到 `outbox/failed/`，而不是无限重试；传输失败或服务器错误（5xx）则留在队列中等待下一次排空，排空每 60 秒自动运行一次。

</details>

<a id="models-and-privacy"></a>

### 模型与隐私

- **本地基础检索：** [BGE 向量模型（embedding model）](https://huggingface.co/Qdrant/bge-base-en-v1.5-onnx-Q) 通过 FastEmbed 在你的设备上运行，用于混合搜索，不需要 API key。
- **可选的设备端整理：** 内容补充（enrichment）与页面汇总可以使用你选择的 [`Qwen3 4B`](https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF) 或 [`Qwen3.5 9B`](https://huggingface.co/unsloth/Qwen3.5-9B-GGUF)，并通过 [llama.cpp](https://github.com/ggml-org/llama.cpp) 运行。你没有选择前，Wenlan 不会下载或启用语言模型。
- **其他模型来源：** Ollama 或 LM Studio 等 OpenAI 兼容的本地端点，或已设置的云端 provider，也可以提供模型支持的内容补充与页面汇总。
- **云端说明：** 如果你选择的模型端点位于远端，Wenlan 会把该任务需要的 system prompt 与 user prompt 发给它。本地检索与设备端整理仍留在你的设备上。
- **可选使用统计：** 默认关闭。只有你同意后，Wenlan 才会发送有限的操作计数、版本与平台，不包含知识内容或安装标识。详见[隐私说明](docs/PRIVACY.md#telemetry)。

完整 workflow 参考：[plugin/skills](plugin/skills/README.md)。模型分工与限制见：[技术基础（英文）](docs/technical-foundations.md#model-roles)。

### 你的数据与卸载

没有任何锁定。页面和会话笔记是 `~/.wenlan/` 下的 Markdown；记忆保存在平台数据目录下的一个 libSQL 数据库中（macOS 为 `~/Library/Application Support/wenlan/`，Linux 为 `~/.local/share/wenlan/`，Windows 为 `%LOCALAPPDATA%\wenlan\`）。复制这两个文件夹即可备份或迁移你的 Wenlan。如果这次安装是从 Origin 升级而来，仍会在 `~/.origin/` 和同级的 `origin` 数据文件夹中（macOS 为 `~/Library/Application Support/origin/`，Linux 为 `~/.local/share/origin/`，Windows 为 `%LOCALAPPDATA%\origin\`）各保留一份完整数据；这两个文件夹也请一并删除或复制。

卸载：app 中「登录时在后台运行文澜」开关会移除开机注册——关闭它并退出，删除 `Wenlan.app` 或运行 Windows 卸载程序，然后删除上述文件夹。`wenlan background off` 只会停止守护进程并关闭开机自启，不会移除开机注册；仅使用 CLI 的安装请改为参照 [PRIVACY.md](docs/PRIVACY.md) 中守护进程的卸载条目。Wenlan 写入的路径列在其中。


<a id="evaluation"></a>

## 评估

以下是 retrieval-only snapshot，不代表 end-to-end answer quality。方法、环境 receipts 与更新流程见 [docs/eval](docs/eval/README.md)。

<!-- EVAL_SNAPSHOT_START -->
| Benchmark | Recall@5 | MRR | NDCG@10 |
|---|---:|---:|---:|
| LME_Oracle (500 Q) | 93.6% | 0.857 | 0.883 |
| LME_S (deep, 90 Q) | 87.7% | 0.815 | 0.822 |
<!-- EVAL_SNAPSHOT_END -->


<a id="learn-more"></a>

## 进一步了解

更完整的文档、概念说明与比较：

### 文档

- [开始使用](https://wenlan.app/docs/get-started)：安装并验证第一个本地循环。
- [日常工作流程](https://wenlan.app/docs/daily-workflow)：brief、capture、recall、handoff、distill、lint 与 curate。
- [MCP 客户端](https://wenlan.app/docs/mcp-clients)：连接 Claude Code、Codex、Cursor、Claude Desktop 与其他工具。

### 工作流指南

- [为咨询项目建立客户知识库](https://wenlan.app/zh-CN/learn/build-client-project-knowledge-base-for-consulting)
- [建立有来源支撑的投资研究知识库](https://wenlan.app/zh-CN/learn/build-investment-research-knowledge-base)
- [在撰写 PRD 前建立产品研究知识库](https://wenlan.app/zh-CN/learn/build-product-research-knowledge-base-for-prd)
- [用运行手册与故障复盘建立 SRE 故障知识库](https://wenlan.app/zh-CN/learn/build-sre-incident-knowledge-base)
- [建立业务指标定义知识库](https://wenlan.app/zh-CN/learn/build-business-metric-definition-knowledge-base)：把获准的 KPI 规范整理成有来源的数据字典，保留公式文本、粒度、排除条件、负责人、修订和审核状态。

### 概念

- [为什么需要持续演进的 wiki，而不只是 AI 记忆](https://wenlan.app/learn/ai-work-memory)：深入理解问题与产品模型。
- [MCP 记忆服务器](https://wenlan.app/learn/mcp-memory-server)：Wenlan 如何让知识跨 AI 工具使用。
- [本地优先的 AI 记忆](https://wenlan.app/learn/local-first-ai-memory)：数据、隐私与控制权。
- [Markdown 与本地索引](https://wenlan.app/learn/markdown-local-index-ai-memory)：存储、检索与所有权。
- [AI agent 的交接循环](https://wenlan.app/learn/ai-agent-handoff-loop)：把工作完整带到下一次会话。
- [用论文建立研究知识库](https://wenlan.app/zh-CN/learn/source-backed-research-knowledge-base)：把已选好的论文整理成可检查的文献矩阵与来源支撑综述。

### 比较

- [Wenlan 与 Basic Memory](https://wenlan.app/learn/wenlan-vs-basic-memory)
- [Wenlan 与 claude-mem](https://wenlan.app/learn/wenlan-vs-claude-mem)
- [Wenlan 与 Superlocal Memory](https://wenlan.app/learn/wenlan-vs-superlocal-memory)


## 贡献

欢迎 bug fixes、eval cases、文档与功能。安装 Wenlan 不需要从源码构建。本地开发时，请从本 repository 的根目录运行以下命令：

```bash
# daemon crates（default-members——不会编译桌面 app）
cargo build
cargo test

# 桌面 app（Cargo target 与根目录的前端工具链）
pnpm install
pnpm dev:all
pnpm build:all
```

`pnpm dev:all` 是桌面 app 受支持的开发入口。它让开发用的端口、数据、进程归属、app 标识、MCP socket 与 Remote Access 状态都与已安装的生产运行时隔离；未处于该隔离环境的 debug build 会拒绝启动。完整开发流程见本 repository 的 [AGENTS.md](AGENTS.md) 与 [CONTRIBUTING.md](.github/CONTRIBUTING.md)，以及仓库内的 [app/AGENTS.md](app/AGENTS.md)。安全性问题请见 [SECURITY.md](.github/SECURITY.md)，隐私政策请见 [PRIVACY.md](docs/PRIVACY.md)，也请阅读 [Code of Conduct](.github/CODE_OF_CONDUCT.md)。


<a id="code-signing-policy"></a>

## Code signing policy

Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).

- **Authors：**[@7xuanlu](https://github.com/7xuanlu)，可直接向本 repository 提交 commit，无需额外 review。
- **Reviewers：**[@7xuanlu](https://github.com/7xuanlu)。非 committer 的每一处改动都以 pull request 形式提交，合并前先经过 review。
- **Approvers：**[@7xuanlu](https://github.com/7xuanlu)，审批每一次签名请求，决定哪一个 release 被签名。

本项目要求每位 maintainer 在 GitHub 与 SignPath 上都启用多因素认证；未启用者不会被加入其中任何一方。Release 只由本 repository 的 tag release workflow 构建，运行在 GitHub 托管的 runner 上，来源是该 tag 指向的 commit。

**隐私政策：**[PRIVACY.md](docs/PRIVACY.md) —— Wenlan 保存什么、保存在哪里，以及我们已知它会访问网络的各种情况。各平台的签名方式见 [docs/code-signing.md](docs/code-signing.md)。

SignPath 的申请正在审核中，Windows 安装包尚未签名。


<a id="license"></a>

## 许可

Wenlan 采用两种许可，按 repository 的不同部分划分。

- **Apache-2.0**（[`LICENSE`](LICENSE)）覆盖 local runtime、CLI、MCP server、shared types，以及 Claude Code 与 Codex 的 plugin files。可以自由基于这些开发。
- **AGPL-3.0-only**（[`app/LICENSE`](app/LICENSE)）覆盖桌面 app：`app/` crate 及其附带的 React 前端。如果你把修改过的 app 作为网络服务运行，AGPL 要求你向使用它的人提供这份修改后的源码。

这个划分是有意为之。Apache-2.0 的代码可以用在 AGPL-3.0 程序里，所以桌面 app 建立在 runtime 之上，两种许可都不会被违反。


<a id="acknowledgments"></a>

## 源流与同类项目

Wenlan（文澜）的名字来自文澜阁。这座皇家藏书楼收藏《四库全书》，曾是中国最大的藏书之一。

Wenlan 的 llm-wiki v2 模型是自己的产品方向，并受到 LLM-wiki 与 agent-memory 两条脉络启发：

- [Karpathy 的 LLM-wiki note](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) 建立了从 raw sources 到持续维护 wiki 的模式。
- [Rohitg00 的 LLM Wiki v2 proposal](https://gist.github.com/rohitg00/2067ab416f7bbe447c1977edaaa681e2) 加入 memory lifecycle、confidence、graph 与 retrieval mechanisms。[agentmemory](https://github.com/rohitg00/agentmemory) 是其具体的 agent-memory implementation。
- [nashsu/llm_wiki](https://github.com/nashsu/llm_wiki) 是以文档为核心的 LLM-wiki 完整桌面实现。
- [basic-memory](https://github.com/basicmachines-co/basic-memory)、[obsidian-mind](https://github.com/breferrari/obsidian-mind)、[mcp-memory-service](https://pypi.org/project/mcp-memory-service/)、[Memoria](https://github.com/matrixorigin/Memoria) 和 [OpenMemory](https://github.com/CaviraOSS/OpenMemory) 探索相邻的本地知识与 agent-memory 方向。
