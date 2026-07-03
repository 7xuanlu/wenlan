<!-- README_SYNC: source=README.md sha256=3a41ad1a7514869e438ba094a42c17a8248dcdd03e1f156142120e85767971e2 -->

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
  <a href="#mcp-only-setup"><img alt="MCP clients" src="https://img.shields.io/badge/MCP-clients-2563EB"></a>
  <a href="#desktop-app"><img alt="Desktop app" src="https://img.shields.io/badge/Desktop-app-24C8DB"></a>
  <a href="#what-you-get"><img alt="Markdown pages" src="https://img.shields.io/badge/Markdown-pages-7C3AED"></a>
</p>

<p align="center">
  <a href="./README.md">English</a> | 简体中文 | <a href="./README.zh-Hant.md">繁體中文</a>
</p>

**面向 AI 原生时代的、会生长的个人知识库，由你的 agents 构建，并由来源支撑。**

Wenlan（文澜）的名字来自一座皇家藏书楼，它曾收藏中国规模最大的典籍之一。你的 agents 在工作时捕获它们学到的内容；你也可以加入自己已经信任的页面和来源，让这座知识库同时自下而上、自上而下地生长。Wenlan 会自行保持内容更新，并把两类材料提炼成带来源引用的 wiki 页面。

每次会话开始时都有简报，结束时都有交接记录，所以上下文会延续，而不是从头再来。

它不同于静态的 llm-wiki，因为它会在会话之间持续演进；也不同于黑盒记忆，因为每个页面都会显示来源，你可以阅读、信任或修正它。

[![观看 Wenlan 演示](./docs/assets/demo-preview.gif)](https://youtu.be/k37gjWVPHwI)

---

## Wenlan 有什么不同

1. **自己就值得信任。** 每个页面都会引用它来自哪些 memories；daemon 会拒绝没有来源的页面，而不是让幻觉摘要混进来。事实会被去重，变化时旧版本会被取代，所以你看到的是干净、最新的 wiki，而不是一堆重复片段。日常流程不会停下来等审批；只有真正冲突的捕获才会浮现给你处理。
2. **自己持续演进。** 大多数记忆工具只是把你放进去的内容再拿出来。Wenlan 会在会话之间继续工作，把捕获内容聚类为带来源引用的 wiki 页面，并和原始 atomic notes 一起参与检索。它不同于静态的 llm-wiki，因为你不需要手动维护也能保持更新。
3. **一个家，不锁定任何工具。** 每个 MCP client 都查询同一个本地 daemon，所以在一个工具里积累的上下文会出现在下一个工具里。Obsidian 只是一个可选视图，可以 symlink 进去，但你的工作并不住在那里。
4. **真正的 git 版本管理。** Memory、page 和 session 写入都会提交到 `~/.wenlan/.git/`，所以你可以 inspect、diff、revert 或 branch 这些 Markdown artifacts。
   ```text
   a1b2c3d page: embedding-retrieval refreshed (4 sources)
   9f8e7d6 session: handoff embedding-work
   5a4b3c2 capture: decision mem_abc123
   ```

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

然后可以在 Claude Code 里试试 `/brief`、`/capture <decision>` 或 `/handoff`。

插件细节和日常命令见：[plugin/](plugin/.claude-plugin/README.md)。

### Codex plugin

```bash
npx -y wenlan setup
codex plugin marketplace add .
codex plugin add wenlan@wenlan-local
```

安装后开启新的 Codex thread，让 skills 和 MCP server 加载。然后可以试试 `/setup`、`/brief`、`/capture <memory>`、`/recall <query>`、`/pages <query>` 或 `/handoff`。

Plugin 细节和开发说明见：[plugin-codex/](plugin-codex/README.md)。

<a id="mcp-only-setup"></a>

### 仅 MCP 设置

如果你想在不安装插件的情况下把 Wenlan tools 接入 Claude Code，或接入 Codex、Cursor、Claude Desktop、VS Code、Gemini CLI，请使用这种方式。

```bash
npx -y wenlan setup
~/.wenlan/bin/wenlan connect claude-code      # or: codex, cursor, claude-desktop, vscode, gemini
```

仅 MCP 模式会给 agents 提供 capture、recall、context、doctor 和 page distillation 工具。它不会安装 plugin slash skills，例如 `/brief`、`/handoff`、`/distill` 或 `/setup`。

### 终端运行时设置

设置本地 Wenlan runtime：

```bash
npx -y wenlan setup
```

然后从 `~/.wenlan/bin/wenlan status`、`~/.wenlan/bin/wenlan recall <query>` 或 `~/.wenlan/bin/wenlan capture <text>` 开始。CLI 细节见：[crates/wenlan-cli](crates/wenlan-cli/README.md)。

运行时控制：

```bash
wenlan background on      # 启动或重启后台 runtime
wenlan restart            # 在 setup 之外升级后重新加载
wenlan status
wenlan background off
```

大多数用户只需要 `npx -y wenlan setup`。当你希望 Wenlan 持续在后台运行，或在 setup 之外升级后，再使用这些 runtime 命令。

<a id="desktop-app"></a>

### 桌面应用

桌面应用是运行在同一个本地 daemon 之上的原生 UI。要使用它，需要同时安装两部分：

```bash
npx -y wenlan setup   # 安装并启动本地 daemon
```

然后下载并安装当前 macOS Apple Silicon 应用：[wenlan-app-darwin-arm64.dmg](https://github.com/7xuanlu/wenlan/releases/latest/download/wenlan-app-darwin-arm64.dmg)。应用会连接到你机器上的 daemon，并读取和 CLI、MCP clients 相同的本地记忆库。

---

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

- **Atomic memory layer**：每条 capture 首先作为带类型的 memory 存储，包含 source agent、confidence、stability 和 supersession metadata。
- **Source-backed pages**：页面保留 source memory IDs、stale reasons 和 revision state，这样 distillation 可以刷新页面而不丢 provenance。
- **基于 libSQL 的 hybrid retrieval**：memories、pages、FTS5 text、vector embeddings 和 graph context 位于同一个本地 store，供你的 MCP clients 查询，并通过 reciprocal-rank fusion 融合。可选的本地 cross-encoder reranker 会进一步优化 top results。
- **Connected recall**：people、projects、tools 和 decisions 会带着链接回来，所以一条 memory 不会孤零零出现，而是带着周围上下文。
- **Distill cycles**：今天可以手动运行 `/distill`；也可以添加本地模型或 API key，启用 background extraction、page refreshes、recaps 和更丰富的 graph links。
- **自己保持新鲜**：后台 pass 会链接 entities、扩展匹配页面，并根据 type、access 和 age 更新每条 memory 的 effective confidence，所以最近和关键的 memories 会浮现，陈旧内容会淡出。
- **先 review，再 trust**：低置信度 captures、pending revisions、contradictions 和 supersessions 可以浮现出来，而不是静默进入 context。
- **Explicit spaces**：用 `space=work | personal | client-X` 给 memories、pages 和 recalls 打标签，避免日常工作捕获流入副项目 brief。未设置时会从当前 repo 或 workspace 自动检测；也始终可以覆盖。
- **数据归你**：所有内容都是 `~/.wenlan/` 下的 plain Markdown，并由 git 版本管理。可以 grep、symlink 到 Obsidian，或随时带走文件。无 lock-in。

### Spaces

Memories 属于某个 **space**，例如 `wenlan`、`career` 或 `ideas`。可在每个 shell 中设置 active space：

    WENLAN_SPACE=career claude

也可以通过 `~/.wenlan/spaces.toml` 声明式配置（见 `plugin/examples/spaces.toml`）。使用 CLI 管理 spaces：

    wenlan spaces list
    wenlan spaces add ideas --default
    wenlan spaces show ideas
    wenlan spaces move scratch career

`wenlan doctor` 会打印当前 resolver state，让你精确看到是哪一层选择了 active space。

---

## Evaluation

**Hybrid retrieval，透明 eval。** BGE-Base-EN-v1.5-Q + FTS5 + Reciprocal Rank Fusion；启用 rerank 时，本地 BGE-Reranker-Base cross-encoder rerank 是默认路径，BGE-Reranker-V2-M3 则是更高质量选项。下表是 retrieval 指标，不是端到端 answer quality。每次 recall query 约 168 tokens。Eval harness 位于 [`crates/wenlan-core/src/eval/`](crates/wenlan-core/src/eval/)。你可以自己运行。

更新流程和 answer-quality snapshots 位于 [docs/eval](docs/eval/README.md)。


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

关于 AI work memory 以及 Wenlan 对比的长文在 [useorigin.app/learn](https://useorigin.app/learn)：

**Concepts**
- [What is AI work memory?](https://useorigin.app/learn/ai-work-memory)：Wenlan 要解决的问题形态
- [MCP memory server](https://useorigin.app/learn/mcp-memory-server)：Wenlan 如何通过 Model Context Protocol 暴露 memory
- [Local-first AI memory](https://useorigin.app/learn/local-first-ai-memory)：data、privacy 和 control
- [Markdown + local index](https://useorigin.app/learn/markdown-local-index-ai-memory)：storage model
- [AI agent handoff loop](https://useorigin.app/learn/ai-agent-handoff-loop)：防止 context loss 的 session-end discipline

**Comparisons**
- [Wenlan vs Basic Memory](https://useorigin.app/learn/origin-vs-basic-memory)：Markdown knowledge base vs AI work-session memory
- [Wenlan vs claude-mem](https://useorigin.app/learn/origin-vs-claude-mem)：observer-style Claude Code memory vs MCP-first cross-tool memory
- [Wenlan vs Superlocal Memory](https://useorigin.app/learn/origin-vs-superlocal-memory)：与另一种本地记忆形态的 tradeoffs

**Docs**
- [Get started](https://useorigin.app/docs/get-started)：install + verify 第一个本地 memory loop
- [Daily workflow](https://useorigin.app/docs/daily-workflow)：capture、handoff、distill
- [MCP clients](https://useorigin.app/docs/mcp-clients)：连接 Claude Code、Cursor、Codex、Claude Desktop、Gemini CLI

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
