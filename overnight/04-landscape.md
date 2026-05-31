# AI Memory Tools: Competitive Landscape and Origin's Wedge

As of mid-2026. Tags: [VERIFIED url] = sourced fact. [INFERRED] = reasoned from facts. [OPINION] = strategic judgment.

Origin's distinctive claims, for reference:
- Local-first. Runs on-device. libSQL plus on-device LLM (Qwen).
- Composition into source-backed wiki pages with mandatory provenance.
- Review-before-trust. Human curates before memory is trusted.
- Real git versioning of memory artifacts.
- MCP plus Claude Code plugin distribution.

---

## 1. Competitor Table

| Tool | Positioning | Funding / Traction | OSS? | Local or Cloud | Distribution |
|---|---|---|---|---|---|
| **mem0** (mem0.ai) | "Memory layer for AI apps and agents." Cloud API plus OSS SDK. | $24M total ($20M Series A led by Basis Set + $3.9M seed), Oct 2025. 41k+ GitHub stars, 13M+ PyPI downloads, 80k+ devs, 186M API calls Q3 2025. Exclusive memory provider for AWS Agent SDK. [VERIFIED https://techcrunch.com/2025/10/28/mem0-raises-24m-from-yc-peak-xv-and-basis-set-to-build-the-memory-layer-for-ai-apps/] [VERIFIED https://github.com/mem0ai/mem0] | Yes (Apache-style OSS core) plus managed cloud | Cloud-default. Self-host possible. | PyPI SDK, cloud API, AWS Agent SDK integration |
| **Letta** (ex-MemGPT) | "Platform for stateful agents." Memory-first agent OS. UC Berkeley spinout. | $10M seed led by Felicis at ~$70M post, Sept 2024. ~21.7k GitHub stars. Letta Code shipped Dec 2025, ranked #1 TerminalBench. [VERIFIED https://www.prnewswire.com/news-releases/berkeley-ai-research-lab-spinout-letta-raises-10m-seed-financing-led-by-felicis-to-build-ai-with-memory-302257004.html] [VERIFIED https://github.com/letta-ai/letta] [VERIFIED https://www.letta.com/blog/letta-code] | Yes (OSS MemGPT repo) plus Letta Cloud | Cloud-default. Self-host the OSS server. | OSS repo, Letta Cloud REST API, Letta Code agent |
| **Zep** (getzep.com) | "Agent memory at enterprise scale." Temporal knowledge graph (Graphiti). | $500K seed, Apr 2024 (small disclosed round). Graphiti OSS is the traction vehicle. [VERIFIED https://www.crunchbase.com/organization/zep-ai] [VERIFIED https://github.com/getzep/graphiti] | Graphiti OSS; Zep platform managed | Cloud-default. Graphiti self-hostable. | Graphiti OSS, Zep cloud platform, SDKs |
| **Supermemory** (supermemory.ai) | "Best memory engine for LLMs." Started as consumer "second brain," now infra. | ~$2.6M seed led by Susa Ventures, Oct 2025. Angels incl. Jeff Dean, Cloudflare, OpenAI/Meta/Google execs. 50k+ users, 10k+ stars as OSS phase. Founder Dhravya Shah, age 19. [VERIFIED https://techcrunch.com/2025/10/06/a-19-year-old-nabs-backing-from-google-execs-for-his-ai-memory-startup-supermemory/] | Was OSS; commercial product now | Cloud-default. | API, SDKs, ships an Anthropic memory-tool adapter |
| **Cognee** (cognee.ai) | "Memory control plane for AI agents." ECL pipeline to knowledge graph. | EUR 7.5M seed led by Pebblebed, announced Feb 19 2026. 70+ companies (Bayer, U. Wyoming). Building a Rust engine for on-device/edge. [VERIFIED https://www.eu-startups.com/2026/02/german-ai-infrastructure-startup-cognee-lands-e7-5-million-to-scale-enterprise-grade-memory-technology/] [VERIFIED https://github.com/topoteretes/cognee] | Yes (OSS core) plus cloud | Cloud-default. Rust edge engine in progress. | PyPI SDK, OSS repo, cloud |
| **Honcho** (honcho.dev, Plastic Labs) | "Memory for stateful agents." Models user/agent identity over time. Peer-based. | OSS, ~4.3k GitHub stars and trending mid-2026. SOTA on agent-memory benches: 89.9% LoCoMo, 90.4% LongMem S. Funding not clearly disclosed. [VERIFIED https://github.com/plastic-labs/honcho] [INFERRED bench numbers from project marketing] | Yes (OSS FastAPI server) plus managed | Self-host FastAPI or api.honcho.dev | OSS, managed API, Claude Code integration |
| **Memary** (kingjulio8238/Memary) | "Open source memory layer for autonomous agents." Human-memory simulation, dashboard. | OSS project, no disclosed funding. Supports local models via Ollama (Llama 3). [VERIFIED https://github.com/kingjulio8238/Memary] | Yes (OSS) | Local-capable via Ollama | GitHub repo only |
| **basic-memory** (basicmachines-co) | "AI conversations that actually remember." Markdown on disk, Obsidian-native. | OSS, AGPL-3.0, ~3.1k GitHub stars. MCP-native. Ships an official Claude Code plugin. [VERIFIED https://github.com/basicmachines-co/basic-memory] | Yes (AGPL-3.0) | **Local-first.** Plain markdown on disk. | MCP server, Claude Code plugin, Obsidian |
| **ChatGPT memory** (OpenAI) | Built-in product memory. Saved memories + reference chat history. | Two layers; chat-history recall launched Apr 10 2025, free tier June 2025. [VERIFIED https://openai.com/index/memory-and-new-controls-for-chatgpt/] | No | Cloud only | In-product, ChatGPT only |
| **Claude memory** (Anthropic, product) | Automatic memory for Pro/Max; Memory Files. Cross-session memory for Managed Agents (public beta Apr 23 2026). | Auto memory for Pro/Max announced Oct 23 2025. Managed Agents cross-session memory public beta Apr 2026 (Rakuten, Netflix cited). [VERIFIED https://www.macrumors.com/2025/10/23/anthropic-automatic-memory-claude/] [VERIFIED https://www.edtechinnovationhub.com/news/anthropic-brings-persistent-memory-to-claude-managed-agents-in-public-beta] | No | Cloud (managed). Memory tool is client-side. | In-product + API |
| **Anthropic Memory Tool** (`memory_20250818`) | API primitive. Claude reads/writes files in `/memories`. Client-side, dev controls storage. | Beta, header `context-management-2025-06-27`. Internal benches: 84% token savings, 39% lift on 100-turn task. [VERIFIED https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool] | N/A (API feature) | **Client-side** storage you control | Anthropic API, SDK helpers |

---

## 2. The Claude Code / MCP Memory Ecosystem

The Claude Code plugin marketplace launched in public beta October 2025 (Claude Code 2.0.13), then stabilized. The official Anthropic marketplace shipped ~36 curated plugins. Community directories list hundreds of plugins, thousands of skills. [VERIFIED https://www.petegypps.uk/blog/claude-code-official-plugin-marketplace-complete-guide-36-plugins-december-2025] [VERIFIED https://github.com/anthropics/claude-plugins-official]

A plugin bundles slash commands, subagents, skills, hooks, and MCP servers in one installable unit. Distribution backends include GitHub, npm, GitLab, local paths. [VERIFIED https://code.claude.com/docs/en/discover-plugins]

Memory/context MCP servers that already exist (the crowded part):
- **mcp-memory-keeper** (mkreyman): cross-session context, stores in `~/mcp-data/`. [VERIFIED https://github.com/mkreyman/mcp-memory-keeper]
- **mcp-memory-service** (doobidoo): persistent memory, REST + knowledge graph + consolidation. [VERIFIED https://github.com/doobidoo/mcp-memory-service]
- **MemCP**: blocks `/compact` until insights are saved. [VERIFIED https://dev.to/dalimay28/how-i-built-memcp-giving-claude-a-real-memory-15co]
- Official Anthropic **knowledge-graph memory server**: local SQLite KG. [VERIFIED https://lobehub.com/mcp/randall-gross-claude-memory-mcp]
- Neo4j-based memory servers tracking decisions/patterns. [VERIFIED https://github.com/mkreyman/mcp-memory-keeper search results]
- **basic-memory**: markdown-on-disk, Obsidian-native, official Claude Code plugin. [VERIFIED https://github.com/basicmachines-co/basic-memory]

What's crowded: "remember my project context across sessions" via a key-value or KG store you can't see. Many of these are weekend-project MCP servers. Quality and trust vary.

What's missing [OPINION]:
- **Provenance you can audit.** None of the popular memory MCP servers make every remembered fact link back to the source conversation or document. They store distilled blobs. When the memory is wrong, you can't trace why.
- **Review-before-trust.** Every server above auto-writes. The agent decides what to remember and writes it silently. No human gate.
- **Versioned, diffable memory.** Storage is a SQLite blob or a KG. You cannot `git diff` what your agent learned this week vs last week.
- **Composition into readable pages.** They store atoms (facts, triples). None compose atoms into a curated, source-cited wiki page a human actually reads.
- basic-memory is the closest neighbor: local, markdown, Obsidian, Claude Code plugin. But it is a passive note store. It does not do provenance-enforced composition, review gates, or treat git versioning as a product surface. [INFERRED from repo description]

---

## 3. Whitespace Analysis

Map the field on two axes that matter for Origin.

**Axis A: Local-first vs Cloud.**
Almost everyone funded is cloud-default. mem0, Letta, Zep, Supermemory, Cognee, ChatGPT, Claude product memory all push you to a hosted backend. The reason is obvious: cloud is how you monetize and how you get telemetry. The local-first lane is sparse: basic-memory, Memary (via Ollama), the Anthropic memory tool (client-side files), and Origin. [INFERRED from the funding/positioning facts above]

**Axis B: Trust model (auto-write vs review-before-trust) and provenance.**
Everyone auto-writes. The agent decides, the store records, nobody checks. Provenance is the differentiator nobody owns. Zep/Graphiti claims provenance to source data, but it is a graph for agents to query, not a human-reviewed artifact. [VERIFIED https://github.com/getzep/graphiti] No competitor combines: mandatory source-backing + human review gate + git history.

The empty quadrant: **local-first AND provenance-enforced AND human-curated AND git-versioned.** Origin is alone there. [OPINION]

The funded money is fighting over "memory API for agents at scale" (mem0, Cognee, Zep, Supermemory, Letta). That is a horizontal infra war Origin cannot and should not win. The product memory war (ChatGPT, Claude) is owned by the model vendors and Origin cannot win that either.

What is uncontested: **memory as a trustworthy, human-owned, auditable knowledge artifact that lives on your machine.** Not a memory API. A memory you can read, correct, diff, and trust because every claim cites its source.

---

## 4. The Single Sharpest Wedge

**Provenance-enforced, git-versioned memory pages for Claude Code developers, running entirely on your machine.**

Reasoning [OPINION]:

The sharpest cut is not "local-first" alone (basic-memory has that) and not "Claude Code native" alone (a dozen MCP servers have that). The cut is the combination that creates *trust*: **every remembered fact links to its source, a human reviews before it is trusted, and the whole memory is a git repo you can diff.**

Why this is the wedge and not the others:
- **Provenance is the anti-hallucination story.** The single loudest complaint about AI memory is "it confidently remembered something wrong." Origin's mandatory source-backing makes every page auditable. When a page is wrong, you click through to the source. No competitor on this list enforces that.
- **Review-before-trust matches how developers already work.** Devs review PRs before merge. "Review your agent's memory before it is trusted" is the same mental model. It is a feature, not friction, for this audience.
- **Git versioning is a developer-native superpower nobody ships.** Memory as a git repo means `git diff`, `git blame`, branches, rollback on your knowledge. Anthropic's own long-agent guidance even references git-based recovery, but the memory tool itself stores opaque files. Origin makes the git repo the product. [VERIFIED https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool]
- **Local-first is the privacy and ownership moat the funded players structurally can't copy.** Their business model needs your data in their cloud. Origin's does not.

One sentence: Origin is the only tool that makes your AI's memory a **local git repo of source-cited, human-reviewed wiki pages** - auditable memory you own, native to Claude Code.

The wedge is narrow on purpose. Do not chase "memory API for all agents." Win "trustworthy local memory for Claude Code power users," then widen.

---

## 5. The Realistic First User (ICP)

**Persona: the privacy-conscious senior engineer / indie hacker who lives in Claude Code and already distrusts cloud memory.**

Specifically: a staff-or-senior backend/infra engineer or a solo technical founder, 3+ years shipping, who:
- Uses Claude Code daily as a primary coding tool. [INFERRED Claude Code plugin ecosystem is large and growing]
- Has felt the pain of re-explaining their project every session.
- Has tried an auto-memory MCP server and got burned by silent, wrong, or noisy memory.
- Cares about data ownership. Will not pipe their codebase and decisions into a startup's cloud.
- Already runs local models (Ollama) or keeps an Obsidian vault. Comfortable with "it's a git repo on my disk."

Where they hang out:
- The Claude Code plugin marketplace and community directories (claudemarketplaces.com, tonsofskills.com, the official Anthropic plugin repo). [VERIFIED https://claudemarketplaces.com/] [VERIFIED https://github.com/anthropics/claude-plugins-official]
- r/ClaudeAI, r/LocalLLaMA, Hacker News "Show HN," the Anthropic Discord, MCP server directories (mcpservers.org, lobehub).
- Obsidian and local-first software communities. basic-memory's Discord is a direct watering hole. [VERIFIED https://github.com/basicmachines-co/basic-memory]

What makes them install in 60 seconds:
- **One-line install as a Claude Code plugin.** `/plugin install origin` from a marketplace. No account, no API key, no cloud signup. The "no signup, runs local" line is the hook for this persona.
- **An immediate, visible artifact.** After install, they see a real folder of markdown pages with citations, in a git repo they can open in their editor. The payoff is tangible in the first session, not after weeks of accumulation.
- **A 30-second demo that shows the diff.** "Here's what your agent learned today" as a `git diff`. That single screenshot sells the whole product to this persona.

The anti-ICP (do not target first): enterprises wanting a hosted memory API (that's mem0/Cognee/Zep), and non-technical ChatGPT users (that's OpenAI's built-in memory). Origin's review-and-git workflow is a feature for engineers and friction for everyone else. [OPINION]

---

## 6. Honest Competitive Threats

**What happens if Anthropic ships native cross-session memory in Claude Code?**

Partly already happening. Anthropic shipped automatic memory for Claude Pro/Max (Oct 2025), the client-side memory tool (`memory_20250818`), and cross-session memory for Managed Agents (public beta Apr 2026). [VERIFIED https://www.macrumors.com/2025/10/23/anthropic-automatic-memory-claude/] [VERIFIED https://www.edtechinnovationhub.com/news/anthropic-brings-persistent-memory-to-claude-managed-agents-in-public-beta] [VERIFIED https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool] The threat is real and live, not hypothetical.

The bear case [OPINION]: If Anthropic makes Claude Code remember your project across sessions for free, "remember my context" - the table-stakes feature - is commoditized. The dozen memory MCP servers in section 2 are most exposed. Origin's "never re-explain your project" pitch gets weaker.

Why Origin survives anyway [OPINION]:
- **Anthropic's memory tool stores opaque files Claude manages, with no provenance, no review gate, no git surface.** The docs describe view/create/str_replace/delete on a `/memories` directory. It is a scratchpad for the agent, not an auditable, human-curated, source-cited knowledge base. [VERIFIED https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool] Origin's differentiation is precisely the layer Anthropic is unlikely to build: human trust, provenance, versioning.
- **Anthropic's incentive is lock-in to Claude.** Their memory lives in their product, tied to their models. Origin's local git repo is model-agnostic and portable. The privacy/ownership crowd specifically wants the thing Anthropic won't give them.
- **Vendor memory is cloud and account-bound.** The local-first persona in section 5 actively rejects that.

But be honest about the squeeze [OPINION]: Anthropic owns "good-enough automatic memory for the median user." Origin must NOT compete there. Origin's only durable ground is the trust/provenance/ownership layer for the minority who care. That minority is real and underserved, but it is a wedge, not the mass market. If Anthropic ever ships provenance + review + git-versioned memory natively, Origin's moat shrinks hard. Bet is they won't, because it conflicts with their lock-in and their "it just works invisibly" UX philosophy. [OPINION]

---

## Sources

- mem0 funding: https://techcrunch.com/2025/10/28/mem0-raises-24m-from-yc-peak-xv-and-basis-set-to-build-the-memory-layer-for-ai-apps/
- mem0 repo: https://github.com/mem0ai/mem0
- Letta seed: https://www.prnewswire.com/news-releases/berkeley-ai-research-lab-spinout-letta-raises-10m-seed-financing-led-by-felicis-to-build-ai-with-memory-302257004.html
- Letta repo: https://github.com/letta-ai/letta
- Letta Code: https://www.letta.com/blog/letta-code
- Zep Crunchbase: https://www.crunchbase.com/organization/zep-ai
- Graphiti repo: https://github.com/getzep/graphiti
- Supermemory funding: https://techcrunch.com/2025/10/06/a-19-year-old-nabs-backing-from-google-execs-for-his-ai-memory-startup-supermemory/
- Cognee funding: https://www.eu-startups.com/2026/02/german-ai-infrastructure-startup-cognee-lands-e7-5-million-to-scale-enterprise-grade-memory-technology/
- Cognee repo: https://github.com/topoteretes/cognee
- Honcho repo: https://github.com/plastic-labs/honcho
- Memary repo: https://github.com/kingjulio8238/Memary
- basic-memory repo: https://github.com/basicmachines-co/basic-memory
- ChatGPT memory: https://openai.com/index/memory-and-new-controls-for-chatgpt/
- Claude auto memory: https://www.macrumors.com/2025/10/23/anthropic-automatic-memory-claude/
- Claude Managed Agents cross-session memory: https://www.edtechinnovationhub.com/news/anthropic-brings-persistent-memory-to-claude-managed-agents-in-public-beta
- Anthropic memory tool docs: https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool
- Claude Code plugin marketplace: https://www.petegypps.uk/blog/claude-code-official-plugin-marketplace-complete-guide-36-plugins-december-2025
- Official Anthropic plugins: https://github.com/anthropics/claude-plugins-official
- Discover plugins docs: https://code.claude.com/docs/en/discover-plugins
- mcp-memory-keeper: https://github.com/mkreyman/mcp-memory-keeper
- mcp-memory-service: https://github.com/doobidoo/mcp-memory-service
- MemCP writeup: https://dev.to/dalimay28/how-i-built-memcp-giving-claude-a-real-memory-15co
- Claude marketplaces directory: https://claudemarketplaces.com/
