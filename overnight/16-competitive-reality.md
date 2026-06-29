# Competitive Reality Check (operator-verified via GitHub search)

This is the hardest finding of the run, and it is independently verified from the GitHub API, not one
agent's recollection. Pulled 2026-05-31 via `search_repositories "memory mcp claude code local git"`.

## The lane Origin is in is crowded, and Origin is not winning it

Origin's README sells four differentiators: local-first, git-versioned, provenance-enforced, Claude Code
native. Three of those four are now table stakes in a specific micro-category that already has multiple
shipping competitors. Real results from one search:

| Repo | Stars | Positioning (their words) | Created |
|---|---|---|---|
| **sverklo/sverklo** | **70** | "Repo memory for coding agents. Local-first MCP for Claude Code, Cursor, Windsurf, Codex. symbol graph, blast radius, diff-aware review, git-pinned decisions. MIT; no API keys or code upload." | 2026-04-06 |
| **7xuanlu/origin** | 34 | "Local-first Rust daemon. Git-versioned memories, distilled wiki pages, sessions for Claude Code, Cursor, Codex, any MCP client." | 2026-04-19 |
| notkurt/ghost | 11 | "Local AI session capture and semantic search for Claude Code. Records sessions as markdown, attaches to commits via git notes." | 2026-02-12 |
| n2ns/n2n-memory | 5 | "MCP for AI memory isolation. Project-local knowledge graph. Privacy-first, Git-friendly." | 2025-12-19 |
| mxfschr/pebble | 0 | "Git-native AI memory for Claude Code. Open source, local-first, zero LLM API calls." | 2026-05-20 |
| onebrain-ai | 0 | "persistent memory, 24+ skills. Plain Markdown. Local-first." | 2026-04-30 |
| vibemem/vibemem | 0 | "persistent memory... Local-first, MCP-compatible, git-committable summaries." | 2026-02-24 |

[VERIFIED github search_repositories, 2026-05-31]

## What this means, bluntly

1. **"Local-first + git-versioned + Claude Code memory" is not a wedge. It is a category** with at least 6
   entrants. Origin is one of several, and it is NOT the star leader. sverklo has 2x Origin's stars, was
   created 2 weeks earlier, is MIT, TypeScript (lower install friction than a Rust daemon), and ships
   developer-specific features Origin lacks (symbol graph, blast radius, diff-aware review). [VERIFIED]

2. **Origin's genuinely-uncommon attributes are the ones it under-sells:** the Rust daemon depth, the
   distilled source-cited wiki pages (composition, not just storage), the provenance enforcement, and the
   eval rigor. None of the TS competitors do page-composition with enforced provenance. But per the wedge
   validation (12), provenance is the pillar with the weakest stated user demand. So Origin's real
   differentiation sits exactly where user pull is weakest. That is the trap.

3. **The install-friction asymmetry matters.** Five of six competitors are TypeScript / npx / plain markdown.
   Origin is a compiled Rust daemon that registers an OS service. In a crowded free category, higher install
   friction loses the casual try. (See 07-onboarding-audit.) [INFERRED from repo languages + install flow]

4. **This reinforces the contrarian read, not the ship-the-product read.** When your product wedge is
   contested by 6 free tools and your true edge (rigor, provenance, composition) has weak consumer pull, the
   higher-leverage move is the one that uses the rigor directly: become the researcher/writer in the
   category (publish the field guide, 10-field-guide.md), or productize the process. The product can stay
   alive as the credibility artifact behind the writing.

## Reconciliation with the launch kit (09)

09-launch-kit.md leads the Show HN with provenance + git-versioning. Given this finding, REVISE:
- Do NOT lead with "git-versioned local-first Claude Code memory." That headline reads as "me too" against
  sverklo and five others. A skeptical HN commenter will list the competitors in the first hour.
- Lead instead with the ONE thing none of them do: **"memory that distills into source-cited wiki pages and
  refuses to store a claim it can't trace."** Composition + enforced provenance is the only un-copied
  ground. Even if demand is narrower, it is the honest differentiator and it is defensible.
- Better still, per the convergent verdict: lead with the FIELD GUIDE (researcher path), not the product.
  The guide has no direct competitor. The product does.

## UPDATE: it is worse than "crowded" (live radar run, 2026-05-31)

I built `overnight/automations/competitor-radar/radar.sh` and ran it live. It runs 5 GitHub searches and
ranks the lane by stars. The result is not a handful of competitors. It is a feeding frenzy. Repos describing
themselves as "persistent memory for AI coding agents," many newer than Origin and most with MORE stars
(Origin = 34):

| Repo | Stars | Note |
|---|---:|---|
| shaneholloman/mcp-knowledge-graph | 861 | persistent memory for Claude via local knowledge graph |
| Goldentrii/AgentRecall-MCP | 276 | "correction-driven memory," cross-session, cross-platform |
| clay-good/OpenLore | 143 | persistent architectural memory from codebase |
| iamtouchskyer/memex | 127 | Zettelkasten persistent memory, Claude Code + Cursor |
| roboticforce/sugar | 79 | persistent memory, autonomous |
| atomicstrata/atomicmemory | 77 | portable semantic memory + TS SDK |
| Eshaan-Nair/ArcRift | 76 | persistent local memory layer |
| JKHeadley/instar | 64 | persistent Claude Code agents w/ memory |
| crisandrews/ClawCode | 56 | persistent agents as a plugin |
| iikarus/Dragon-Brain | 47 | persistent long-term memory via MCP |
| hyxnj666-creator/ai-memory | 45 | git-trackable Markdown from Cursor/Claude convos |
| aouicher/graphmind | 37 | Rust, local-first code intelligence knowledge graph |
| **7xuanlu/origin** | **34** | you |
| zero8dotdev/smriti | 32 | shared memory for eng teams |

[VERIFIED live radar.sh run, GitHub search API, 2026-05-31. Broad agent-harness repos like ECC/holaOS that
the query also surfaced are excluded as off-category.]

Implications, harder than before:
- Origin ranks roughly 13th-15th by stars in its own micro-category. At least 12 tools have more traction
  with (in most cases) less engineering. [VERIFIED]
- Several already claim Origin's "unique" features: "git-trackable Markdown" (hyxnj666/ai-memory),
  "correction-driven memory" (AgentRecall, close to your review/contradiction story), knowledge-graph memory
  (mcp-knowledge-graph 861*, graphmind in Rust). The provenance/composition gap is narrower than it looked.
- This is the single strongest argument in the whole kit for the researcher path over the product launch.
  You cannot out-ship 15 teams in a frenzy by tuning retrieval. You CAN be the one person who writes the
  honest, rigorous account of why most of their memory claims are unmeasured. Nobody owns that.
- The radar automation should run weekly (see competitor-radar.yml). The fact that you did not know the lane
  looked like this is exactly the blind spot it closes.

## VERIFICATION
- Source: GitHub `search_repositories` live result, 2026-05-31. Star counts and descriptions are the API's,
  not estimated. PASS.
- The wedge agent's specific example `yuvalsuede/memory-mcp` did NOT appear in my search and is therefore
  UNCONFIRMED. I am not citing it. The broader claim it made (free competitors cover 3 of 4 pillars) is
  OVER-confirmed by 6 other repos, so the conclusion stands on stronger evidence than the agent's single
  example. [VERIFIED replacement evidence]
- Limitation: one search query, default relevance sort. There may be more entrants and the ranking is not
  exhaustive. The directional conclusion (crowded lane, Origin not leading) is robust to that. A fuller
  scan is queued for a later wave.
