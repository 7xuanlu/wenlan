<!-- README_SYNC: source=README.md sha256=e83d5bdc07a08743e44032489a3792122fec2e3adf026c0c9d00d9c60b6e878a -->

<p align="center">
  <picture>
    <source media="(max-width: 600px)" srcset="./docs/assets/readme-banner-mobile.png">
    <img src="./docs/assets/readme-banner.png" alt="Wenlan：有来源支撑、会持续积累的知识库。" width="100%">
  </picture>
</p>

和 AI 聊出的成果，不该在对话结束后消失。

Wenlan 会建立真正需要的页面，并在来源变化时让它们保持最新；只有需要判断时才找你。

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
  <a href="#how-does-it-work">如&#8288;何&#8288;运&#8288;行</a> ·
  <a href="#evaluation">评&#8288;估</a> ·
  <a href="#learn-more">进&#8288;一&#8288;步&#8288;了&#8288;解</a>
</p>

<p align="center">
  <img src="./docs/assets/desktop-wiki-preview.png" alt="Wenlan 桌面 app，展示有来源支撑的 wiki 页面与可检查的来源记忆。" width="100%">
</p>

---

<a id="quickstart"></a>
<a id="start-in-30-seconds"></a>

## 开始使用

<a id="start-with-the-app"></a>
<a id="open-the-wiki"></a>

### 桌面 app

桌面 app 是最快看到完整工作流程的方式：阅读页面、检查来源，并整理知识体系。从 [wenlan-app releases](https://github.com/7xuanlu/wenlan-app/releases/latest) 下载最新 macOS Apple Silicon DMG，打开 Wenlan，再跟随内置设置向导。App 自带本地运行环境，也能设置检测到的 AI 工具，不需要先使用终端。

App 源码：[wenlan-app](https://github.com/7xuanlu/wenlan-app)。产品与文档：[wenlan.app](https://wenlan.app)。

<a id="claude-code-in-30-seconds"></a>

### Claude Code

```text
/plugin marketplace add 7xuanlu/wenlan
/plugin install wenlan@7xuanlu-wenlan
/setup
```

如果 Claude Code 安装后要求重新启动，请重启一次，再执行 `/setup`。Plugin 会完成本地 runtime、MCP 连接、知识库与第一次 round-trip 检查。

Plugin 指令与工作流程：[plugin/](plugin/.claude-plugin/README.md)。

<a id="codex-plugin"></a>

### Codex plugin

```bash
npx -y wenlan setup
codex plugin marketplace add .
codex plugin add wenlan@7xuanlu-wenlan
```

安装后开启新的 Codex thread，让 plugin 与 MCP server 加载。详细说明：[plugin-codex/](plugin-codex/README.md)。

<a id="mcp-setup"></a>
<a id="mcp-clients"></a>

### 无 GUI 与其他 MCP 客户端

偏好只使用终端，或所用客户端不需要 Wenlan 的图形界面？

```bash
npx -y wenlan setup
wenlan connect claude-code  # or: codex, cursor, claude-desktop, vscode, gemini
```

核心工具是 `context`、`capture`、`recall`、`pages` 和 `doctor`。所有连接的 client 都使用同一个本地 daemon 与 store。

<a id="cli"></a>

偏好直接用 CLI？

```bash
wenlan status
wenlan recall <query>
wenlan capture <text>
```

CLI 详细说明：[crates/wenlan-cli](crates/wenlan-cli/README.md)。

---

<a id="what-does-wenlan-build"></a>
<a id="why-it-compounds"></a>

## 这是什么？

<p align="center">
  <img src="./docs/assets/feature-reel.gif" alt="Wenlan feature reel，展示有来源支撑的页面、来源检查、graph context、agent capture 与 curation。" width="100%">
</p>

Wenlan 把散落的文档、笔记和 AI 对话汇入同一个 local-first 知识库。这就是 llm-wiki v2：两个相连的知识层，一层是随时可查的证据，另一层是会随时间持续积累的 wiki。

知识可以在不同 AI 工具里再次派上用场，又不会失去来源。日常整理和更新由 Wenlan 在后台处理；只有来源彼此矛盾，或需要修改你亲自写的内容时，才会等你判断。

### 随时可查的证据

来源文档、导入的对话和工作中捕获的决策，会保留成可追溯的记忆与来源记录。你能知道它们来自哪里、目前有多稳定，以及后来有哪些信息修正或取代了它们。

### 会持续复利的 wiki

Wenlan 把相关证据汇总成带有来源引用的 Markdown 页面。即使换了 AI 工具，`/brief` 和 `/recall` 也能把当前页面及其证据带回后续工作。新材料会继续改善同一个页面，而不是又产生一个互不相连的答案。

页面和 session notes 都以纯 Markdown 保存在 `~/.wenlan/`。Distill 与 handoff workflows 会把文件的逻辑批次提交到本地 git repository，留下可检查、可携带的历史。

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

### llm-wiki v2：两套相连的生命周期

生成出来的 wiki 只是一份快照。Wenlan 把它当作持续维护的知识体系，让证据与由它整理成的文章各自演进，又彼此相连。

- **记忆生命周期：** 证据可以被确认、连接、修正或取代，而不失去原有历史。近似重复可以安静合并；矛盾则等待审核。
- **文章生命周期：** 每篇文章都保留引用，也知道来源何时变化。由 Wenlan 维护的文章可以刷新；你亲自编辑过的文章只会收到修订提案，不会被静默改写。
- **两者的连接：** 记忆变化时，Wenlan 知道哪些文章可能已经过时，并能在保留证据脉络的前提下更新它们。

例如，导入一份设计文档，再让 Codex 捕获一次调试决策。Wenlan 可以把两者整理成一篇同时引用两份证据的文章。任一来源变化时，文章可以刷新；如果你已经编辑过文章，更新提案会等待审核。

这就是 Wenlan 所说的 llm-wiki v2：证据与文章会一起持续改善，而不是生成一次就停止。

<a id="what-wenlan-is-not"></a>

### 适合需要长期延续的工作

Wenlan 适合横跨多次会话、项目与数周的软件开发、研究、写作、咨询、产品决策和客户工作。它不是为一次性聊天、生活管理系统，或作为其他产品的记忆 SDK 而设计。

---

<a id="what-you-get"></a>
<a id="what-can-it-do"></a>

## 能力

- **整合多种来源：** 导入 ChatGPT 和 Claude 对话、索引 Obsidian 或文档文件夹、接收工作中的直接 captures，并让它们共同成为同一个页面的证据。
- **有证据支撑的知识：** captures 保留 source agent、confidence、stability 与 supersession；pages 保留 source memories、citation records、stale reasons、ownership 与 revision state。
- **持续维护、可审核的页面：** 自动 re-distill refresh 无法验证其 claims 时会 fail closed。对用户拥有页面的更新会成为 pending revision，不会静默改写。
- **会话之间持续整理：** 配置本地模型或 API provider 后，后台流程可以 enrich captures、连接 entities、合并重复内容，并在下一次 session 前刷新符合条件的页面。
- **仅在需要时审核：** 矛盾与受保护 memory 的冲突可以进入明确 review，而不会把每条 capture 都变成审批任务。
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

## 如何运行

每次 session 都会运行同一个循环：工作时捕获，让 Wenlan 在 sessions 之间整理，下一次再带着最新知识回到工作。

```text
      ┌──────── loops back · /handoff closes each pass ─────────┐
      ▼                                                         │
┌─────┴─────┐    ┌─────────────┐    ┌────────────────┐    ┌─────┴─────┐
│ CAPTURE   │    │ DAEMON      │    │ ONE STORE      │    │ RECALL +  │
│  in flow  │ ─▶ │  refines    │ ─▶ │  (local)       │ ─▶ │  BRIEF    │
│  /capture │    │  between    │    │  · memories    │    │  next     │
│           │    │  sessions   │    │  · wiki pages  │    │  session  │
│           │    │  dedup·link │    │  · graph       │    │  /recall  │
│           │    │  /distill   │    │                │    │  /brief   │
└───────────┘    └─────────────┘    └────────────────┘    └───────────┘
   one local daemon · one store · every MCP client reads it
   Claude Code · Cursor · Codex · Claude Desktop · VS Code · Gemini
```

每一轮都让知识库更清晰：零散的捕获内容可以被去重、连接并整理成有来源支撑的页面，因此下一次带回的是知识，而不是原始历史。

四个公开 workflow 驱动整个系统：

1. **带着 context 开始。** `/brief [topic]` 加载项目状态、偏好与相关知识。只使用 MCP 的 clients 可调用 `context` 完成同一件事。
2. **在工作中捕获与查找。** `/capture <thing>` 以 typed provenance 存储决策、经验、gotcha 或事实。`/recall <query>` 只取回相关部分，不把全部历史塞进 context。
3. **闭合循环。** `/handoff` 记录改了什么、还有哪些未解问题，以及下一次 session 该从哪里继续。
4. **维护 wiki。** `/distill` 主动建立或刷新页面。后台流程可以在 sessions 之间补充 captures、连接 entities、合并重复内容，并刷新符合条件的页面。`/lint` 与 `/curate` 则公开完整性与审核工作。

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

<a id="build-from-source"></a>

## 从源码构建

Wenlan 可从源码构建于 macOS（Apple Silicon 与 Intel）、Linux（x86_64 与 ARM64，glibc）和 Windows（x86_64）。目前预构建 releases 支持 macOS Apple Silicon、Linux x86_64/ARM64 with glibc 与 Windows x86_64；macOS Intel 仍需从源码构建。多数用户应通过 plugin 或 `npx` 安装。

```bash
git clone https://github.com/7xuanlu/wenlan.git
cd wenlan
cargo build --workspace
cargo run -p wenlan-server
```

Runtime、crates 与平台细节见 [AGENTS.md](AGENTS.md#cross-platform) 和各 crate README。

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

Wenlan 吸收 LLM-wiki 与 agent-memory 两条脉络，但不声称完整实现 LLM Wiki v2：

- [Karpathy 的 LLM-wiki note](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) 建立了从 raw sources 到持续维护 wiki 的模式。
- [Rohitg00 的 LLM Wiki v2 proposal](https://gist.github.com/rohitg00/2067ab416f7bbe447c1977edaaa681e2) 加入 memory lifecycle、confidence、graph 与 retrieval mechanisms。[agentmemory](https://github.com/rohitg00/agentmemory) 是其具体的 agent-memory implementation。
- [nashsu/llm_wiki](https://github.com/nashsu/llm_wiki) 是以文档为核心的 LLM-wiki 完整桌面实现。
- [basic-memory](https://github.com/basicmachines-co/basic-memory)、[obsidian-mind](https://github.com/breferrari/obsidian-mind)、[mcp-memory-service](https://pypi.org/project/mcp-memory-service/)、[Memoria](https://github.com/matrixorigin/Memoria) 和 [OpenMemory](https://github.com/CaviraOSS/OpenMemory) 探索相邻的本地知识与 agent-memory 方向。
