<!-- README_SYNC: source=README.md sha256=48f15ffc51953d7eb90b7a80805d539ca88b5e2e041f9ee01cc75cbe681ef88d -->

<p align="center">
  <img src="./docs/assets/social-preview.png" alt="Wenlan：面向 AI 原生时代的、会生长的个人知识库。" width="100%">
</p>

<p align="center">
  <a href="https://github.com/7xuanlu/wenlan/actions/workflows/ci.yml?query=branch%3Amain"><img alt="CI" src="https://github.com/7xuanlu/wenlan/actions/workflows/ci.yml/badge.svg?branch=main&event=push"></a>
  <a href="https://github.com/7xuanlu/wenlan/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/7xuanlu/wenlan?sort=semver&label=release"></a>
  <a href="#license"><img alt="License: Apache-2.0" src="https://img.shields.io/badge/license-Apache--2.0-blue.svg"></a>
</p>

<p align="center">
  <a href="#claude-code-in-30-seconds"><img alt="Claude Code" src="https://img.shields.io/badge/Claude%20Code-plugin-5D4E75"></a>
  <a href="#codex-plugin"><img alt="Codex" src="https://img.shields.io/badge/Codex-plugin-111827"></a>
  <a href="#mcp-setup"><img alt="MCP clients" src="https://img.shields.io/badge/MCP-clients-2563EB"></a>
  <a href="#start-with-the-app"><img alt="Desktop app" src="https://img.shields.io/badge/Desktop-app-24C8DB"></a>
  <a href="#what-you-get"><img alt="Markdown pages" src="https://img.shields.io/badge/Markdown-pages-7C3AED"></a>
</p>

<p align="center">
  <a href="./README.md">English</a> | 简体中文 | <a href="./README.zh-Hant.md">繁體中文</a>
</p>

**面向 AI 原生时代的、会生长的个人知识库，由你的 agents 构建，并由来源支撑。**

不同于普通 llm-wiki 只根据固定文档集生成页面，Wenlan 会从实时 agent 工作和你信任的来源中，持续维护一套带来源引用的 wiki。它面向与 AI agents 一起推进的长期工作，从软件开发和研究，到写作、咨询、产品决策和客户工作。

你的 agents 在会话中捕获它们学到的内容；你也可以加入自己已经信任的页面和来源。Wenlan 会把两者提炼成 Markdown 页面，并在会话之间刷新。每条新 thread 都从这套更新后的 wiki 开始，用 brief 带回相关上下文，用 handoff 记录下一次该从哪里继续。

Wenlan（文澜）的名字来自文澜阁。这座皇家藏书楼藏有《四库全书》，曾是中国规模最大的藏书之一。

<p align="center">
  <img src="./docs/assets/desktop-wiki-preview.png" alt="Wenlan 桌面 app，展示带来源引用的 wiki 页面和 source memory 悬浮卡片。" width="100%">
</p>

---

<a id="start-with-the-app"></a>

## 先从桌面 app 开始

桌面 app 是阅读和整理这套带来源引用 wiki 的最快入口。Agents 仍然可以在 Claude Code、Codex、Cursor、VS Code、Claude Desktop 或任何 MCP client 中捕获和召回上下文；所有路径都连接同一个本地 daemon 和 Markdown store。

先设置一次 Wenlan：

```bash
npx -y wenlan setup
```

然后下载当前 macOS Apple Silicon build：[wenlan-app-darwin-arm64.dmg](https://github.com/7xuanlu/wenlan/releases/latest/download/wenlan-app-darwin-arm64.dmg)。

App 源码：[wenlan-app](https://github.com/7xuanlu/wenlan-app)。产品详情：[wenlan.app](https://wenlan.app)。

---

## Wenlan 有什么不同

1. **值得信任的来源。** 每个页面都会引用背后的 memories；Wenlan 会拒绝没有来源的页面，而不是让幻觉摘要混进来。事实会被去重，变化时旧版本会被取代，所以 wiki 保持干净，而日常捕获不需要变成审批队列。
2. **会话之间保持更新。** Wenlan 会在会话之间把新的捕获聚类为带来源引用的页面，并让页面和背后的 atomic notes 一起参与检索。这套 wiki 会反映你的最新工作，而不是停在过期快照。
3. **一个家，不锁定任何工具。** 每个 MCP client 都查询同一个本地 daemon，所以在一个工具里积累的上下文会出现在下一个工具里。Obsidian 只是一个可选视图，可以 symlink 进去，但你的工作并不住在那里。
4. **真正的 git 版本管理。** Memory、page 和 session 写入都会提交到 `~/.wenlan/.git/`，所以你可以 inspect、diff、revert 或 branch 这些 Markdown artifacts。
   ```text
   a1b2c3d page: embedding-retrieval refreshed (4 sources)
   9f8e7d6 session: handoff embedding-work
   5a4b3c2 capture: decision mem_abc123
   ```

下面这段短 reel 展示完整产品循环：带来源支撑的页面、source cards、graph structure、agent capture，以及进入召回前的整理。

<p align="center">
  <img src="./docs/assets/feature-reel.gif" alt="Wenlan 五段式 feature reel，展示带来源支撑的页面、source cards、graph structure、agent capture 和整理 review。" width="100%">
</p>

---

## 快速开始

<a id="claude-code-in-30-seconds"></a>

### Claude Code 30 秒接入

```text
/plugin marketplace add 7xuanlu/claude-plugins
/plugin install wenlan@7xuanlu
/setup
```

如果 Claude Code 在安装后要求重启，请重启一次，然后运行 `/setup`。插件会处理本地 runtime 设置、MCP wiring、本地记忆初始化，以及第一次 round-trip 检查。

插件细节和日常命令见：[plugin/](plugin/.claude-plugin/README.md)。

### Codex plugin

```bash
npx -y wenlan setup
codex plugin marketplace add .
codex plugin add wenlan@wenlan-local
```

安装后开启新的 Codex thread，让 plugin 和 MCP server 加载。

Plugin 细节和开发说明见：[plugin-codex/](plugin-codex/README.md)。

<a id="mcp-setup"></a>

### MCP 设置

两个插件底层都会调用同一个本地 MCP server。核心工具是 `context`、`capture`、`recall`、`pages` 和 `doctor`。

如果你想在不安装插件的情况下把 Wenlan tools 接入 Claude Code，或接入 Codex、Cursor、Claude Desktop、VS Code、Gemini CLI，请使用这种方式：

```bash
npx -y wenlan setup
wenlan connect claude-code      # or: codex, cursor, claude-desktop, vscode, gemini
```

仅 MCP clients 会直接使用相同的核心工具，用于 context、capture、recall、doctor checks 和 page distillation。

### CLI

先设置一次 Wenlan：

```bash
npx -y wenlan setup
```

然后直接使用 CLI：

```bash
wenlan status
wenlan recall <query>
wenlan capture <text>
```

CLI 细节见：[crates/wenlan-cli](crates/wenlan-cli/README.md)。

## Wenlan 如何工作

每次会话都会运行同一个 loop：工作时捕获，让 daemon 在会话之间 refine，然后在下一次带着已有知识回来。

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

每一轮都会让 store 更清晰。放在别处会变成松散 snippets 的捕获内容，会在这里被去重、链接到相关的人和项目，并提炼成引用来源的页面；所以下一次会话带回来的不是原始历史，而是知识。这就是这个 loop 所说的 compounding。

这五个动作驱动它：

1. **会话开始。** `/brief [topic]` 加载项目状态、身份、偏好，以及和主题相关的 memories，让 agent 带着上下文进入工作。
2. **工作过程中。** `/capture <thing>` 在 flow 中保存 decision、lesson、gotcha 或 project fact。`/recall <query>` 用来查找任何内容。
3. **会话结束。** `/handoff` 写下发生了什么、还剩什么、从哪里继续，让下一轮干净接上。
4. **会话之间。** daemon 在后台去重重叠捕获，并链接相关 ideas。想做一次有意的整理时，可以用 `/distill` 从相关 memories 的 clusters 中合成 wiki 页面。
5. **下一次会话。** Claude Code 和 Codex plugins 里的 `/brief` 会把上下文带回来；仅 MCP clients 调用 `context` tool 获得同样的 memory。Recall 只拉回相关切片，而不是你的全部历史，所以 context window 会留给当前工作。

完整 skill reference：[plugin/skills](plugin/skills/README.md)。

完全本地可用，不需要 API key、cloud account 或 signup。Capture、recall、hybrid search 和 graph context 都不需要外部服务；只有自动 page distillation 才需要添加本地模型或 API key。无 telemetry。

---

<a id="what-you-get"></a>

## 你会得到什么

- **Typed captures**：每条 capture 都会带着 source agent、confidence、stability 和 supersession metadata 存储。
- **Source-backed pages**：页面保留 source memory IDs、stale reasons 和 revision state，这样 distillation 可以刷新页面而不丢 provenance。
- **基于 libSQL 的 hybrid retrieval**：memories、pages、FTS5 text、vector embeddings 和 graph context 位于同一个本地 store，供你的 MCP clients 查询，并通过 reciprocal-rank fusion 融合。可选的本地 cross-encoder reranker 会进一步优化 top results。
- **Connected recall**：people、projects、tools 和 decisions 会带着链接回来，所以一条 memory 不会孤零零出现，而是带着周围上下文。
- **Distill cycles**：今天可以手动运行 `/distill`；也可以添加本地模型或 API key，启用 background extraction、page refreshes、recaps 和更丰富的 graph links。
- **会话之间刷新**：后台 pass 会链接 entities、扩展匹配页面，并根据 type、access 和 age 更新每条 memory 的 effective confidence，所以最近和关键的 memories 会浮现，陈旧内容会淡出。
- **先 review，再 trust**：低置信度 captures、pending revisions、contradictions 和 supersessions 可以浮现出来，而不是静默进入 context。
- **Explicit spaces**：用 `space=work | personal | client-X` 给 memories、pages 和 recalls 打标签，避免日常工作捕获流入副项目 brief。未设置时会从当前 repo 或 workspace 自动检测；也始终可以覆盖。
- **数据归你**：所有内容都是 `~/.wenlan/` 下的 plain Markdown，并由 git 版本管理。可以 grep、symlink 到 Obsidian，或随时带走文件。无 lock-in。

---

## Evaluation

下表是 retrieval-only snapshot，不是端到端 answer quality。方法和更新流程见 [docs/eval](docs/eval/README.md)。


<!-- EVAL_SNAPSHOT_START -->
| Benchmark | Recall@5 | MRR | NDCG@10 |
|---|---:|---:|---:|
| LME_Oracle (500 Q) | 93.6% | 0.857 | 0.883 |
| LME_S (deep, 90 Q) | 87.7% | 0.815 | 0.822 |
<!-- EVAL_SNAPSHOT_END -->

---

## 从源码构建

Wenlan 可以在 macOS（Apple Silicon + Intel）、Linux（x86_64 + ARM64；glibc）和 Windows（x86_64）上原生构建。npm wrapper（`wenlan`、`wenlan-mcp`）和 `install.sh` 会自动检测平台并拉取匹配的 prebuilt release。大多数用户应该通过 Claude Code 插件或 `npx` 安装。本地开发：

```bash
git clone https://github.com/7xuanlu/wenlan.git
cd wenlan
cargo build --workspace
cargo run -p wenlan-server
```

daemon、MCP server、CLI 和 core crates 的构建细节在上面链接的 crate READMEs 中。跨平台细节（service registration、paths、Windows install limitation）见 [AGENTS.md](AGENTS.md#cross-platform)。

---

## 了解更多

关于 AI work memory 以及 Wenlan 对比的长文在 [wenlan.app/learn](https://wenlan.app/learn)：

**Concepts**
- [What is AI work memory?](https://wenlan.app/learn/ai-work-memory)：Wenlan 要解决的问题形态
- [MCP memory server](https://wenlan.app/learn/mcp-memory-server)：Wenlan 如何通过 Model Context Protocol 暴露 memory
- [Local-first AI memory](https://wenlan.app/learn/local-first-ai-memory)：data、privacy 和 control
- [Markdown + local index](https://wenlan.app/learn/markdown-local-index-ai-memory)：storage model
- [AI agent handoff loop](https://wenlan.app/learn/ai-agent-handoff-loop)：防止 context loss 的 session-end discipline

**Comparisons**
- [Wenlan vs Basic Memory](https://wenlan.app/learn/origin-vs-basic-memory)：Markdown knowledge base vs AI work-session memory
- [Wenlan vs claude-mem](https://wenlan.app/learn/origin-vs-claude-mem)：observer-style Claude Code memory vs MCP-first cross-tool memory
- [Wenlan vs Superlocal Memory](https://wenlan.app/learn/origin-vs-superlocal-memory)：与另一种本地记忆形态的 tradeoffs

**Docs**
- [Get started](https://wenlan.app/docs/get-started)：install + verify 第一个本地 memory loop
- [Daily workflow](https://wenlan.app/docs/daily-workflow)：capture、handoff、distill
- [MCP clients](https://wenlan.app/docs/mcp-clients)：连接 Claude Code、Cursor、Codex、Claude Desktop、Gemini CLI

---

## Wenlan 不是什么

- **不是 Life OS。** 没有 habits、calendar、journal 或 life-management modules。Wenlan 只聚焦 AI work artifacts。如果你想要完整 personal OS，可以看 [PAI](https://github.com/danielmiessler/PAI)。
- **不是 workflow suite。** 一个 daemon 上约 30 个 MCP tools。如果你想要打包好的 30+ skills、8+ agents 和 auto-research loop，可以看 [pro-workflow](https://github.com/rohitg00/pro-workflow)。Wenlan 用 breadth 换 focus。
- **不是 memory infrastructure SDK。** 它面向每天使用 AI 的人，而不是给其他 app 构建 memory features 的 backend。
- **不适合一次性聊天。** 当工作跨越 sessions、projects 和 weeks 时，它最有价值。

---

## Contributing

欢迎 bug fixes、eval cases、docs 和 features。先看 [CONTRIBUTING.md](CONTRIBUTING.md)。Architecture 和 development rules 在 [CLAUDE.md](CLAUDE.md)。Security reports：[SECURITY.md](SECURITY.md)。也请阅读 [Code of Conduct](CODE_OF_CONDUCT.md)。

---

<a id="license"></a>

## License

Wenlan 使用 **Apache-2.0** 授权。这包括本 repo 中的 local runtime、CLI、MCP server、shared types，以及 Claude Code/Codex plugin files。

这个宽松许可证让 daemon boundary 对 MCP clients 和下游本地工具保持可用。

---

## Acknowledgments

前辈：

- [Karpathy's LLM-wiki note](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f)。Raw-to-wiki distillation pattern。
- Claude Code 的 `MEMORY.md`。这个想法最简单的版本。

同路项目：

- [agentmemory](https://github.com/rohitg00/agentmemory)。Agent-side memory framework。
- [basic-memory](https://github.com/basicmachines-co/basic-memory)。面向 Claude 的 local-first knowledge management。
- [obsidian-mind](https://github.com/breferrari/obsidian-mind)。面向 coding agents 的 Obsidian-native memory and review loop。
- [pro-workflow](https://github.com/rohitg00/pro-workflow)。Claude Code productivity suite。
- [mcp-memory-service](https://github.com/doobidoo/mcp-memory-service)。MCP memory service。
- [Memoria](https://github.com/matrixorigin/Memoria)。通过 Copy-on-Write 实现的 "Git for AI Agent Memory"。
- [OpenMemory](https://github.com/CaviraOSS/OpenMemory)、[claude-memory-compiler](https://github.com/coleam00/claude-memory-compiler)、[PAI](https://github.com/danielmiessler/PAI)、Palinode。相邻形态。

它们是同一个问题的不同形状。选择适合你的那个。
