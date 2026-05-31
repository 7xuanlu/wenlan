# Competitor Deep-Dive: Local-First / Git-Native AI Memory for Coding Agents

Scan date: 2026-05-31. Subject: `7xuanlu/origin` (34 stars).

Method note: GitHub search snippets and README badge text proved unreliable. WebFetch read the agentmemory README badge as "20k stars" but that needed confirmation. All star counts below are pulled from the live GitHub REST API (`search_repositories`, `minimal_output:false`) on 2026-05-31, not from rendered pages or shields.io badges. Feature claims come from fetched READMEs and repo descriptions, cited inline.

Verification tags: [VERIFIED url] = pulled from a fetched page or the API. [INFERRED] = my deduction from verified facts. [OPINION] = judgment call.

---

## 1. The fuller scan

The category splits into three layers. Origin sits in the middle layer.

- **Layer A — Memory infrastructure (huge, generic, not coding-specific):** mem0, the official MCP memory server. Thousands to tens of thousands of stars. Not direct competitors; they are upstream primitives.
- **Layer B — Coding-agent memory (Origin's lane):** sverklo, agentmemory, ghost, n2n-memory, pebble, contextforge, letta-code. This is the contested ground.
- **Layer C — Code intelligence (adjacent, retrieval-not-memory):** roam-code, GitNexus, CodeGraph. Symbol graphs over the codebase, not memory of agent work. Sverklo straddles B and C.

### Competitor table

All stars [VERIFIED via GitHub API 2026-05-31]. Positioning quotes are the repo's own `description` field unless noted.

| Repo | Stars | Lang | License | Friction | One-line positioning (verified) |
|---|---|---|---|---|---|
| modelcontextprotocol/servers (official memory) | 86,498 | TypeScript | MIT | npx, very low | Reference MCP servers incl. knowledge-graph memory. [VERIFIED https://github.com/modelcontextprotocol/servers] |
| mem0ai/mem0 | 57,172 | Python | (Apache) | pip/SaaS | "Universal memory layer for AI Agents." Generic, not coding-specific. [VERIFIED https://github.com/mem0ai/mem0] |
| rohitg00/agentmemory | 20,013 | TypeScript | Apache-2.0 | npx, low | "#1 Persistent memory for AI coding agents based on real-world benchmarks." [VERIFIED https://github.com/rohitg00/agentmemory] See anomaly note below. |
| letta-ai/letta-code | 2,618 | TypeScript | Apache-2.0 | npx, low | "The memory-first coding agent." Agent, not a memory layer. Git-tracked MemFS, optional cloud. [VERIFIED https://github.com/letta-ai/letta-code] |
| Cranot/roam-code | 466 | Python | (n/a) | CLI/MCP | "Local codebase intelligence CLI + MCP... SQLite code graph, 28 languages, 224 MCP tools, zero API keys." Code-intel, not memory. [VERIFIED https://github.com/Cranot/roam-code] |
| **sverklo/sverklo** | **70** | TypeScript | MIT | npx, low | "Repo memory for coding agents. Local-first MCP... symbol graph, blast radius, diff-aware review, and git-pinned decisions. MIT; no API keys or code upload." [VERIFIED https://github.com/sverklo/sverklo] |
| **7xuanlu/origin (subject)** | **34** | Rust | Apache-2.0 | daemon, higher | "Local-first Rust daemon for AI work. Git-versioned memories, distilled wiki pages, sessions for Claude Code, Cursor, Codex, and any MCP client." [VERIFIED https://github.com/7xuanlu/origin] |
| notkurt/ghost | 11 | TypeScript | (MIT) | npx, low | "Local AI session capture and semantic search for Claude Code. Records sessions as markdown, attaches to commits via git notes, indexes into QMD." [VERIFIED https://github.com/notkurt/ghost] |
| n2ns/n2n-memory | 5 | TypeScript | MIT | npx, low | "MCP Server for AI memory isolation. Project-local knowledge graph... Privacy-first, Git-friendly." [VERIFIED https://github.com/n2ns/n2n-memory] |
| mxfschr/pebble | 0 | TypeScript | MIT | npx, low | "Git-native AI memory for Claude Code. Open source, local-first, zero LLM API calls." [VERIFIED https://github.com/mxfschr/pebble] |
| alfredoizdev/contextforge-mcp | 0 | JavaScript | MIT | npx client, cloud backend | "Persistent memory MCP server... semantic search, Git sync, and team collaboration." Cloud-backed, thin client. [VERIFIED https://github.com/alfredoizdev/contextforge-mcp] |

Friction read [INFERRED]: every Layer B competitor except Origin ships TypeScript/JS over `npx`. Origin is the only one shipping a compiled Rust daemon you install and run as a service. That is the single biggest adoption tax in the lane.

### agentmemory star anomaly [VERIFIED + OPINION]

agentmemory is real at 20,013 stars [VERIFIED https://github.com/rohitg00/agentmemory API 2026-05-31]. But it was created 2026-02-25, so ~20k stars + 1,655 forks in ~3 months, with 192 open issues, is abnormal organic growth for a dev tool. The "#1... based on real-world benchmarks" framing plus a `.dev` marketing site suggests aggressive promotion. [OPINION] Treat its star count as marketing-amplified, not a pure quality signal. It is still a serious competitor on features and friction regardless of how the stars were earned.

### One-liner vs README contradictions

- **sverklo:** Search-engine snippets and one cached title claimed "code intelligence" and "43x fewer tokens." The actual repo description and README say "repo memory" and "about 35x fewer input tokens than naive grep" [VERIFIED https://github.com/sverklo/sverklo]. The 43x figure is stale. Use 35x.
- **sverklo benchmark depth:** BENCHMARKS.md holds only latency/index-size numbers and explicitly defers quality to `benchmark/` [VERIFIED https://github.com/sverklo/sverklo/blob/main/BENCHMARKS.md]. The `benchmark/` dir documents methodology (F1>=0.8 gate, tasks P1/P2/P4/P5, baselines naive-grep/smart-grep/sverklo, codebases sverklo+express) but the fetched tree view did not surface the headline F1 table [VERIFIED https://github.com/sverklo/sverklo/tree/main/benchmark]. The "0.58 F1 / 180 tasks / 6 codebases" figure appeared only in a search snippet and a README claim, not in a results file I could open. Tag that number [INFERRED, unconfirmed in raw results].
- **contextforge:** Description says "MCP server" implying local. README says it is a thin client over a hosted ContextForge API; knowledge lives server-side [VERIFIED https://github.com/alfredoizdev/contextforge-mcp]. It is cloud, not local-first. The one-liner misleads.

---

## 2. Deep-dive: sverklo (the star leader in Origin's lane)

[VERIFIED https://github.com/sverklo/sverklo, API + README, 2026-05-31]

70 stars, MIT, TypeScript, org-owned, created 2026-04-06. Site sverklo.com. Tagline: "Repo memory for coding agents."

### What it actually does

Sverklo is the retrieval layer an agent calls **before writing code**. It is half code-intelligence, half memory.

Code intelligence:
- **Symbol graph:** parses the codebase, builds a dependency graph, runs PageRank to surface load-bearing files.
- **Blast radius:** `refs`/impact tools walk the symbol graph to return ranked transitive callers, the real set that breaks if you change a symbol. Not grep matches.
- **Diff-aware review:** `review_diff` produces a risk-scored review of a git diff (touched-symbol importance x coverage x churn), output as markdown / JSON / GitHub review format.

Memory:
- **Git-pinned, bi-temporal:** every memory carries `valid_from_sha` / `valid_until_sha` / `superseded_by`, pinned to the commit it was authored on. Supports "what did this team believe about auth at commit abc123?"
- **Consolidation:** `sverklo prune` collapses similar episodic memories into one semantic note, preserving lineage.

Engineering:
- Local-only. Embedded SQLite + a local ONNX embedding model. No API keys, no code upload.
- 37 MCP tools across search / impact / review / memory / post-filter / index-health.
- Install: `npm i -g sverklo` or `npx sverklo /path` or one-click Cursor/VS Code badge.
- Clients: Claude Code, Cursor, Windsurf, Zed, VS Code, JetBrains, Codex CLI, Copilot CLI, any MCP client.
- Claim: "about 35x fewer input tokens than naive grep" [VERIFIED README]. F1 0.58 leaderboard claim [INFERRED, not found in raw results file].

### Positioning vs Origin

Sverklo and Origin are solving **different halves of the same workflow** and only partly overlap.

- Sverklo = memory **about the code** (symbols, callers, diffs) + decisions pinned to commits. Pre-code retrieval. Code-aware.
- Origin = memory **about the work and the user** (atomic memories, distilled wiki pages, sessions, knowledge graph). Cross-session knowledge. Code-agnostic.

Their only true overlap is "git-versioned memory of decisions." Everything else diverges.

### Where sverklo is genuinely stronger [VERIFIED + OPINION]

- **Install friction:** `npx`, one-click badges, zero binary. Origin needs a Rust daemon install + service registration. Sverklo wins adoption hands down. [VERIFIED both READMEs]
- **Code intelligence:** symbol graph, blast radius, diff review. Origin has none of this. For "help me change this code safely," sverklo is the better tool. [VERIFIED]
- **Bi-temporal git-SHA pinning:** memories scoped to commit ranges with supersession. Origin has git-versioned memories but not the same valid-from/valid-until SHA model. [VERIFIED] This is a real, hard-to-copy feature.
- **Tool breadth:** 37 MCP tools vs Origin's narrower surface. [VERIFIED]

### Where Origin is genuinely deeper [VERIFIED + OPINION]

- **Page composition:** Origin distills memory clusters into source-backed wiki pages that re-enter retrieval. Sverklo's `prune` consolidates memories into semantic notes, which is close, but Origin's pages are first-class retrieval objects with revision state, not just compressed notes. [VERIFIED both]
- **Enforced provenance:** Origin's daemon **refuses unsourced pages**; every page cites source memory IDs and carries `stale_reasons` / `revision_state` [VERIFIED https://github.com/7xuanlu/origin]. No competitor enforces provenance as a write-time gate. Sverklo preserves lineage but does not reject unsourced synthesis. [VERIFIED]
- **Eval rigor:** Origin publishes LongMemEval (93.6% R@5, 0.857 MRR) and LoCoMo (70.0% R@5) numbers against standard memory benchmarks [VERIFIED README]. Sverklo's eval is a self-built 4-task code-retrieval harness on 2 codebases, not a standard benchmark [VERIFIED benchmark/ dir]. Origin's numbers are comparable to published research; sverklo's are bespoke.
- **Retrieval stack:** vector + FTS5 + RRF + cross-encoder rerank + graph neighbors [VERIFIED]. Sverklo is SQLite + ONNX embeddings + symbol graph. Both hybrid; Origin's fusion is more layered. [VERIFIED]

---

## 3. The honest gap map

Capability x competitor. "Yes" requires a verified feature claim. Origin's column is bolded.

| Capability | Origin | sverklo | agentmemory | ghost | n2n | pebble | letta-code | Rare? |
|---|---|---|---|---|---|---|---|---|
| Local-first storage | **Yes** | Yes | Yes | Yes | Yes | Yes | Yes (local mode) | Table stakes |
| MCP server for Claude Code/Cursor | **Yes** | Yes | Yes | Yes | Yes | Yes | Yes | Table stakes |
| Persistent cross-session memory | **Yes** | Yes | Yes | Yes | Yes | Yes | Yes | Table stakes |
| Git-versioned / git-friendly memory | **Yes** | Yes | partial | Yes (git notes) | Yes | Yes | Yes (MemFS) | Common |
| Hybrid retrieval (vector+FTS+RRF) | **Yes** | partial (vec+graph) | Yes (BM25+vec+graph RRF) | partial (QMD) | partial | No | n/a | Common-ish |
| Knowledge graph | **Yes** | Yes (symbol) | Yes | No | Yes | No | No | Common |
| Cross-encoder rerank | **Yes** | No | unclear | No | No | No | No | Rare |
| Symbol graph / blast radius / diff review | No | **Yes (only here)** | No | No | No | No | No | Rare (sverklo owns) |
| Bi-temporal git-SHA pinning | No | **Yes (only here)** | No | No | No | No | No | Rare (sverklo owns) |
| Distilled wiki pages re-entering retrieval | **Yes (only here)** | No (prune notes, not pages) | No | partial (KB rebuild) | No | partial (consolidation) | No | **Rare (Origin owns)** |
| Enforced provenance (daemon rejects unsourced pages) | **Yes (only here)** | No | No | No | No | No | No | **Rare (Origin owns)** |
| Standard-benchmark eval (LoCoMo/LongMemEval) | **Yes (only here)** | No (bespoke code bench) | claims "real-world benchmarks", unverified standard | No | No | No | No | **Rare (Origin owns)** |
| npx / zero-binary install | No (Rust daemon) | Yes | Yes | Yes | Yes | Yes | Yes | **Table stakes Origin LACKS** |

### Cells only Origin occupies [VERIFIED]

1. **Enforced provenance at write time** — daemon rejects unsourced pages. No other repo gates synthesis on citations. [VERIFIED https://github.com/7xuanlu/origin]
2. **Distilled source-cited wiki pages as first-class retrieval objects** with revision state and stale-reason tracking. Closest is sverklo `prune` and pebble consolidation, but neither produces cited, refreshable pages that feed retrieval. [VERIFIED]

Bonus near-unique: **standard-benchmark eval discipline**. Origin is the only one in the lane citing LoCoMo/LongMemEval. agentmemory claims benchmark superiority but against an unverified bespoke harness. [VERIFIED + INFERRED]

### Cell Origin conspicuously lacks

**Low-friction install.** Every other Layer B repo is `npx`. Origin's Rust daemon is the lane's heaviest install. This is the gap that matters most for adoption.

---

## 4. Strategic read

[OPINION throughout, grounded in the verified facts above.]

The free lane is crowded and getting more so weekly (pebble and contextforge are days old). Stars cluster at the two ends: generic infra (mem0, official server, agentmemory) and full agents (letta-code). The pure "coding-agent memory layer" middle is a long tail of sub-100-star repos. Origin (34) is mid-pack in that tail. None of the tail has won. There is no incumbent in Origin's exact niche.

**Is there a defensible position?** Yes, but narrow, and not the one the product is currently optimizing for.

Origin's three owned cells (enforced provenance, cited wiki pages, eval rigor) are all **trust and correctness** features, not convenience features. They do not help a casual user who wants "remember my last session." They help someone who cannot tolerate a hallucinated summary entering long-term memory: regulated work, research, multi-month projects, teams. That is a real but smaller market than the npx crowd is chasing.

The Rust daemon is a liability for casual adoption and an asset for that trust-first buyer (single writer, real service, auditable). [INFERRED] Trying to win the broad free lane on convenience is a losing fight against `npx sverklo` and `npx agentmemory`. Origin will not out-friction TypeScript.

**Recommendation:** Do not demote Origin to a pure credibility artifact, but do narrow it. Lead with provenance + composition + eval as the wedge for the correctness-sensitive user (the researcher path), and treat broad coding-agent memory as table stakes you support, not the headline. The eval rigor is the credibility artifact that makes the provenance claim believable. Keep it loud. [OPINION]

**Single most threatening competitor: sverklo.** [OPINION, grounded]

Not agentmemory (different altitude, generic, marketing-amplified) and not letta-code (it is an agent, not a layer). Sverklo is the threat because it sits in Origin's exact lane, is the star leader there (70 vs 34), ships near-zero-friction over npx, and already has the one thing closest to Origin's moat: lineage-preserving consolidation (`prune`) plus git-SHA-pinned decisions. If sverklo adds cited, refreshable pages and a provenance gate, it closes Origin's two owned cells while keeping its install-friction and code-intelligence advantages. Origin would be left with only eval rigor. Sverklo is one feature away from eating the moat; Origin is many features away from matching sverklo's friction and code-intel. That asymmetry is the danger.

---

### Source list (all fetched 2026-05-31)

- https://github.com/sverklo/sverklo (API + README)
- https://github.com/sverklo/sverklo/blob/main/BENCHMARKS.md
- https://github.com/sverklo/sverklo/tree/main/benchmark
- https://github.com/7xuanlu/origin (API + README)
- https://github.com/rohitg00/agentmemory (API + README)
- https://github.com/letta-ai/letta-code (API + README)
- https://github.com/notkurt/ghost (API + README)
- https://github.com/n2ns/n2n-memory (API + README)
- https://github.com/mxfschr/pebble (API + README)
- https://github.com/alfredoizdev/contextforge-mcp (API + README)
- https://github.com/Cranot/roam-code (API)
- https://github.com/mem0ai/mem0 (API)
- https://github.com/modelcontextprotocol/servers (API)
