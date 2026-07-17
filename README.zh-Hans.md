<!-- README_SYNC: source=README.md sha256=e4c03751702a190aa7e9e617f18784678f72406616a05948c04573384b5403b1 -->

<p align="center">
  <picture>
    <source media="(max-width: 600px)" srcset="./docs/assets/readme-banner-mobile.png">
    <img src="./docs/assets/readme-banner.png" alt="Wenlan：有来源支撑、会持续积累的知识库。" width="100%">
  </picture>
</p>

和 AI 聊出的成果，不该在对话结束后消失。Wenlan 会建立真正需要的页面，并在来源变化时让它们保持最新；只有需要判断时才找你。

<p align="center">
  <a href="./README.md">English</a> | 简体中文 | <a href="./README.zh-Hant.md">繁體中文</a>
</p>

<p align="center">
  <a href="https://github.com/7xuanlu/wenlan/actions/workflows/ci.yml?query=branch%3Amain"><img alt="CI" src="https://github.com/7xuanlu/wenlan/actions/workflows/ci.yml/badge.svg?branch=main&event=push"></a>
  <a href="https://github.com/7xuanlu/wenlan/releases/latest"><img alt="最新版本" src="https://img.shields.io/github/v/release/7xuanlu/wenlan?sort=semver&label=release"></a>
  <a href="#license"><img alt="许可证：Apache-2.0" src="https://img.shields.io/badge/license-Apache--2.0-blue.svg"></a>
</p>

<p align="center">
  <a href="#start-in-30-seconds">开&#8288;始&#8288;使&#8288;用</a> ·
  <a href="#what-does-wenlan-build">这&#8288;是&#8288;什&#8288;么？</a> ·
  <a href="#what-can-it-do">能&#8288;力</a> ·
  <a href="#how-does-it-work">日&#8288;常&#8288;流&#8288;程</a> ·
  <a href="#evaluation">评&#8288;估</a> ·
  <a href="#learn-more">进&#8288;一&#8288;步&#8288;了&#8288;解</a>
</p>

<p align="center">
  <img src="./docs/assets/desktop-wiki-preview.png" alt="Wenlan 桌面 app，展示有来源支撑的 wiki 页面与可检查的引用。" width="100%">
</p>

---

<a id="quickstart"></a>
<a id="start-in-30-seconds"></a>

## 开始使用

<a id="start-with-the-app"></a>
<a id="open-the-wiki"></a>

### 桌面 app

桌面 app 是最快看到完整工作流程的方式：阅读页面、检查来源并整理知识体系。目前仅提供 macOS Apple Silicon 预览版，尚未经过 Apple notarization。下面的安装器会验证 GitHub release，只为 Wenlan 清除 quarantine，安装后直接打开，不会更改 macOS 系统安全设置：

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/7xuanlu/wenlan/main/scripts/install-macos-app.sh)"
```

你可以直接[检查安装器源码](scripts/install-macos-app.sh)。安装器会先用 GitHub 发布的 SHA-256 核对下载文件，再替换现有 app。偏好 DMG 或想查看 app 源码？请前往 [wenlan-app releases](https://github.com/7xuanlu/wenlan-app/releases/latest) 和 [wenlan-app](https://github.com/7xuanlu/wenlan-app)。

<a id="claude-code-in-30-seconds"></a>

<a id="codex-plugin"></a>

<a id="mcp-setup"></a>
<a id="mcp-clients"></a>

### 让你的 AI 完成设置

把下面这段贴给 Claude Code、Codex，或其他能够读取设置指南的工具：

```text
请为当前 AI 客户端设置 Wenlan，并严格遵循：
https://raw.githubusercontent.com/7xuanlu/wenlan/main/docs/setup-with-ai.md

只安装这个客户端需要的内容。完成后验证本地 runtime、
Wenlan connection，以及一次 capture/recall round trip。
```

指南会识别当前使用的 client，把各平台命令留在专门文档中。除非你明确要求，否则它不会设置所有 AI 工具。

只需要在 macOS Apple Silicon 上运行的无 GUI 本地服务？

```bash
npx -y wenlan setup
```

这个命令会下载预编译的 CLI、后台服务（daemon）与 MCP 连接器，启动并验证本地服务；不需要安装 Rust 或 Cargo。Linux x64/ARM64 可以使用自动化的 [shell 设置流程](docs/setup-with-ai.md#install-the-runtime)；Windows x64 请从 [Releases](https://github.com/7xuanlu/wenlan/releases/latest) 下载对应的 archive。macOS Intel 目前[没有受支持的完整 runtime 安装方式](crates/wenlan-cli/README.md#macos-intel)。

手动与各 client 设置说明：[AI 辅助设置](docs/setup-with-ai.md) · [Claude Code plugin](plugin/.claude-plugin/README.md) · [Codex plugin](plugin-codex/README.md) · [CLI 与 MCP](crates/wenlan-cli/README.md)。

---

<a id="what-does-wenlan-build"></a>
<a id="why-it-compounds"></a>

## 这是什么？

Wenlan 把文档、笔记和过去的 AI 对话整理成会随工作持续更新、每个结论都能追溯来源的知识库。原始材料保留为来源；工作中的决策、经验与修正成为长期记忆；两者都能支撑同一批持续维护的页面。

<p align="center">
  <picture>
    <source media="(max-width: 600px)" srcset="./docs/assets/wenlan-system-zh-Hans-mobile.png">
    <img src="./docs/assets/wenlan-system-zh-Hans.png" alt="来源与记忆分别支撑同一个持续维护的页面。页面过时后，Wenlan 可以依当前依据重建；可选的冲突审核可以让受保护内容的冲突浮现，对人工文字的改动则等待用户判断。" width="100%">
  </picture>
</p>

[llm-wiki v2](https://gist.github.com/rohitg00/2067ab416f7bbe447c1977edaaa681e2) 这个名称来自 Rohitg00 对 [Karpathy 原始 LLM-wiki](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) 的延伸提案。Wenlan 把这个模型变成可以直接使用的产品：可追溯的来源、由 AI agent 按 Zettelkasten（卡片盒笔记法）捕获的原子记忆（每条只表达一个完整想法），以及同时由两者建立并持续维护的页面。

**Wenlan 最独特的做法：** 来源与原子记忆不是终点。Wenlan 把两者整理成可以阅读和反复使用的页面，持续记录每个页面由什么支撑，也保留被取代的旧知识。机器维护的页面可以依当前证据重建；对你文字的改动会等待审核，而不是直接覆盖。

<p align="center">
  <img src="./docs/assets/feature-reel.gif" alt="Wenlan feature reel，展示有来源支撑的页面、来源检查、graph context、agent capture 与 curation。" width="100%">
</p>

### 随时可查的证据

来源文档与导入的对话会保留为来源记录。工作中捕获的决策、经验与修正则成为记忆。两者都保留出处；记忆还会记录可信度、稳定度、更正与取代关系。

### 会持续复利的 wiki

Wenlan 把相关来源与记忆汇总成带有引用的 Markdown 页面。即使换了 AI 工具，也能通过页面、搜索与 `/recall` 把最新知识带回工作；`/brief` 只是可选的会话开始汇总入口。新材料会继续改善同一个页面，而不是又产生一个互不相连的答案。

页面和 session notes 都以纯 Markdown 保存在 `~/.wenlan/`。Distill 与 handoff workflows 可以把文件的逻辑批次提交到本地 git repository，留下可检查、可携带的历史。

本地历史可以直接检查：

```text
$ git -C ~/.wenlan log --oneline
a1b2c3d distill: 4 pages
9f8e7d6 session: embedding-work
```

**已经在用 Obsidian？继续用。** Wenlan 可以把现有 vault 当作来源读取。要在 Obsidian 中使用 Wenlan 自己的页面，可以把 `~/.wenlan/pages/` symlink 到 vault，或从桌面 app 导出页面。Wenlan 会把你对这些页面的编辑视为用户拥有的内容；之后的机器更新会成为可审核的修订建议，不会覆盖你的文字。

<a id="what-makes-wenlan-distinct"></a>
<a id="why-is-wenlan-different"></a>
<a id="two-lifecycles"></a>

### 两套生命周期，一个持续维护的知识系统

一次生成的 wiki 会过时；只存记忆又容易碎成互不相连的事实。Wenlan 连接两套生命周期，但不把它们混成同一层。

<p align="center">
  <picture>
    <source media="(max-width: 600px)" srcset="./docs/assets/wenlan-lifecycle-zh-Hans-mobile.png">
    <img src="./docs/assets/wenlan-lifecycle-zh-Hans.png" alt="明确取代旧说法的新记忆仍会保留前后关联。页面过时后，Wenlan 会依当前来源与记忆重建、记录修订，并把对人工文字的改动变成审核提案。" width="100%">
  </picture>
</p>

#### 原子记忆

`CAPTURE -> CLASSIFY -> ENRICH -> LINK -> RECONCILE`

Capture 与明确的 supersession 属于核心流程。模型支持的阶段只会在配置相应模型后运行，Reconcile 默认关闭。

| 操作 | Wenlan 做什么 |
|---|---|
| **Capture** | AI agent 每次写入一条完整、自足的想法，遵循 Zettelkasten 的原子笔记原则，而不是保存整段对话。 |
| **Classify** | 配置本地模型后，Wenlan 将记忆分为 `identity`、`preference`、`decision`、`lesson`、`gotcha` 或 `fact`；调用方明确提供的准确类型优先。 |
| **Enrich** | 配置本地模型后，在可用时补充结构化字段、检索提示、事件日期、质量、重要性与标签。 |
| **Link** | 保留出处；启用 enrichment 后，把记忆连接到知识图谱中的实体与关系。 |
| **Reconcile** | 明确取代旧说法时保留 `supersedes` 链。可选的本地模型流程可以把受保护内容的冲突放入审核，而不是覆盖历史；它默认关闭，必须明确启用。 |

高级设置：使用 `WENLAN_ENABLE_DUAL_POOL_RESOLVE=1` 启用这个 Reconcile 流程。

#### 持续维护的页面

`DISTILL -> CITE -> TRACK -> REFRESH -> REVIEW`

| 操作 | Wenlan 做什么 |
|---|---|
| **Distill** | 把相关来源与记忆汇总成一个 Markdown 页面。 |
| **Cite** | 保留引用记录与验证状态；自动 refresh 若未通过引用支撑检查，就会丢弃草稿。 |
| **Track** | 记录哪些证据支撑页面、页面为何过时，以及有上限的变更记录。 |
| **Refresh** | 页面被标记为过时后，依当前证据重建符合条件、由机器维护的页面。 |
| **Review** | 对你编辑过的页面提出修订，而不是静默改写。 |

例如，导入一份设计文档，再让 Codex 捕获一次调试决策。Wenlan 可以把两者整理成一个同时引用两份依据的页面。这个页面 refresh 时，会依当前依据重建；如果你已经编辑过它，改动提案会等待审核。

<a id="what-wenlan-is-not"></a>

### 适合需要长期延续的工作

Wenlan 适合横跨多次会话、项目与数周的软件开发、研究、写作、咨询、产品决策和客户工作。它不是为一次性聊天、生活管理系统，或作为其他产品的记忆 SDK 而设计。

---

<a id="what-you-get"></a>
<a id="what-can-it-do"></a>

## 能力

- **整合多种来源：** 导入 ChatGPT 和 Claude 对话、索引 Obsidian 或文档文件夹、接收工作中的直接 captures，并让它们共同成为同一个页面的证据。
- **有证据支撑的知识：** 捕获的记忆保留 source agent、confidence、stability 与 supersession；经过 distill 或 refresh 的 Pages 保留对来源记录与记忆的链接、citation records 与 verification status、stale reasons、ownership 与 revision state。
- **持续维护、可审核的页面：** 自动 re-distill refresh 无法通过 citation verification gate 时会 fail closed。对用户拥有页面的更新会成为 pending revision，不会静默改写。
- **会话之间持续整理：** 可选的模型流程可以在你离开时 enrich captures、连接 entities，并 distill 或 refresh 符合条件的 Pages；具体阶段取决于配置的是本地模型还是 API provider。
- **可选的冲突审核：** 明确启用本地 Reconcile 流程后，受保护 memory 的冲突可以进入 review，而不会把每条 capture 都变成审批任务。
- **混合关联检索：** libSQL 以 reciprocal-rank fusion 结合 FTS5、vector search、pages、memories 与 graph context，也可选择本地 cross-encoder reranker。
- **跨工具连续性：** Claude Code、Codex、Cursor、桌面 app 与 MCP clients 查询同一个本地 daemon，因此在一个工具捕获的 context 可以在另一个工具回来。
- **显式空间：** 把 memories、pages 与 recall 限定在 work、personal 或 client contexts；默认可依 repo 判断，也永远能明确覆盖。
- **保留 Obsidian，也不被绑定：** 把现有 vault 当作只读来源索引，再用你熟悉的编辑器阅读、编辑、symlink 或导出 Wenlan 的 Markdown 页面。
- **本地优先的数据所有权：** daemon 默认只绑定 localhost；memories 与 graph data 留在本地 libSQL，长期 pages 与 session notes 则以用户拥有的 Markdown 保存在 `~/.wenlan/`，并留下本地 git 历史。

<a id="what-can-i-bring-in"></a>

### 支持来源

Wenlan 从你已经拥有的材料开始，并让每一项内容都能追溯到来源。

- **过去的 AI 对话：** 把 ChatGPT 或 Claude 导出的 ZIP 放进桌面 app。Wenlan 会批量导入对话，并自动跳过已经导入的内容。
- **笔记与文档：** 连接 Obsidian vault，或任何包含 `.md`、`.txt`、`.pdf` 的文件夹。Wenlan 只读取来源文件夹，不会回写；普通文件夹会在后台检查变化，Obsidian vault 则可从 app 重新同步。CLI 也能用 `wenlan sources add <path>` 登记单个支持的文件。
- **正在进行的 AI 工作：** Claude Code、Codex、Cursor、Claude Desktop、VS Code、Gemini CLI 与其他 MCP 客户端，都能在工作过程中把决策、经验和上下文存进同一个本地知识库。
- **自定义集成：** 需要接上其他采集流程时，本地 HTTP API 可以接收整理好的文本、网页内容与记忆。

一份文档、一段旧对话和一项新的 agent 决策，可以共同支撑同一个页面，而不再分散在不同孤岛中。

---

<a id="how-wenlan-works"></a>
<a id="how-does-it-work"></a>

## 日常流程

日常使用分成一个小循环：取回相关知识、保存工作重点、以 handoff 收尾，再由 Wenlan 整理下次需要的内容。每一轮都改善同一个知识库，不再累积互不相连的历史。

这个循环分成四步：

1. **找到最新知识。** 打开相关 Page、搜索，或使用 `/recall <query>`；`/brief [topic]` 可选择性汇总更完整的 session-start context。其他 AI 工具可使用等价的 page、search、recall 与 context 工具。
2. **工作时随手保存与查找。** `/capture <thing>` 保存决策、经验、踩坑或事实，并记录来源。`/recall <query>` 只取回相关内容，不加载全部历史。
3. **闭合循环。** `/handoff` 记录改动与待办，也指出下次工作的起点。
4. **让 wiki 保持最新。** `/distill` 主动建立或刷新页面。可选的模型流程会在两次工作之间补充已保存内容、连接相关知识，并刷新符合条件的页面。`/lint` 检查知识库健康状态；`/curate` 让你审核页面更新提案，以及可选 Reconcile 流程产生的冲突项目。

### 选择 Wenlan 如何整理

Capture、recall、混合搜索、graph context 与 Markdown store 不需要 API key 或 cloud account，就能在本地运行。需要汇总页面或进行更深入的补充时，可以使用设备上的模型、Ollama 或 LM Studio 等 OpenAI-compatible 本地端点，或已设置的云端 provider。Wenlan 不发送 telemetry。

完整 workflow 参考：[plugin/skills](plugin/skills/README.md)。

---

<a id="evaluation"></a>

## 评估

以下是 retrieval-only snapshot，不代表 end-to-end answer quality。方法、环境 receipts 与更新流程见 [docs/eval](docs/eval/README.md)。

<!-- EVAL_SNAPSHOT_START -->
| Benchmark | Recall@5 | MRR | NDCG@10 |
|---|---:|---:|---:|
| LME_Oracle (500 Q) | 93.6% | 0.857 | 0.883 |
| LME_S (deep, 90 Q) | 87.7% | 0.815 | 0.822 |
<!-- EVAL_SNAPSHOT_END -->

---

<a id="learn-more"></a>

## 进一步了解

更完整的文档、概念说明与比较：

### 文档

- [开始使用](https://wenlan.app/docs/get-started)：安装并验证第一个本地循环。
- [日常工作流程](https://wenlan.app/docs/daily-workflow)：brief、capture、recall、handoff、distill、lint 与 curate。
- [MCP 客户端](https://wenlan.app/docs/mcp-clients)：连接 Claude Code、Codex、Cursor、Claude Desktop 与其他工具。

### 概念

- [为什么需要持续演进的 wiki，而不只是 AI 记忆](https://wenlan.app/learn/ai-work-memory)：深入理解问题与产品模型。
- [MCP 记忆服务器](https://wenlan.app/learn/mcp-memory-server)：Wenlan 如何让知识跨 AI 工具使用。
- [本地优先的 AI 记忆](https://wenlan.app/learn/local-first-ai-memory)：数据、隐私与控制权。
- [Markdown 与本地索引](https://wenlan.app/learn/markdown-local-index-ai-memory)：存储、检索与所有权。
- [AI agent 的交接循环](https://wenlan.app/learn/ai-agent-handoff-loop)：把工作完整带到下一次会话。

### 比较

- [Wenlan 与 Basic Memory](https://wenlan.app/learn/wenlan-vs-basic-memory)
- [Wenlan 与 claude-mem](https://wenlan.app/learn/wenlan-vs-claude-mem)
- [Wenlan 与 Superlocal Memory](https://wenlan.app/learn/wenlan-vs-superlocal-memory)

---

## 贡献

欢迎 bug fixes、eval cases、文档与功能。先阅读 [CONTRIBUTING.md](CONTRIBUTING.md)。架构与开发规则在 [AGENTS.md](AGENTS.md)。安全性问题请见 [SECURITY.md](SECURITY.md)，也请阅读 [Code of Conduct](CODE_OF_CONDUCT.md)。

---

<a id="license"></a>

## 许可

Wenlan 采用 **Apache-2.0** 许可，包括本 repository 内的 local runtime、CLI、MCP server、shared types，以及 Claude Code/Codex plugin files。

---

<a id="acknowledgments"></a>

## 源流与同类项目

Wenlan（文澜）的名字来自文澜阁。这座皇家藏书楼收藏《四库全书》，曾是中国最大的藏书之一。

Wenlan 的 llm-wiki v2 模型是自己的产品方向，并受到 LLM-wiki 与 agent-memory 两条脉络启发：

- [Karpathy 的 LLM-wiki note](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) 建立了从 raw sources 到持续维护 wiki 的模式。
- [Rohitg00 的 LLM Wiki v2 proposal](https://gist.github.com/rohitg00/2067ab416f7bbe447c1977edaaa681e2) 加入 memory lifecycle、confidence、graph 与 retrieval mechanisms。[agentmemory](https://github.com/rohitg00/agentmemory) 是其具体的 agent-memory implementation。
- [nashsu/llm_wiki](https://github.com/nashsu/llm_wiki) 是以文档为核心的 LLM-wiki 完整桌面实现。
- [basic-memory](https://github.com/basicmachines-co/basic-memory)、[obsidian-mind](https://github.com/breferrari/obsidian-mind)、[mcp-memory-service](https://pypi.org/project/mcp-memory-service/)、[Memoria](https://github.com/matrixorigin/Memoria) 和 [OpenMemory](https://github.com/CaviraOSS/OpenMemory) 探索相邻的本地知识与 agent-memory 方向。
