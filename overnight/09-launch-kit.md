# Launch Kit — ready-to-post distribution assets

Purpose: remove the activation energy from the outward work he avoids. These are drafts to edit and post, not
advice to "consider launching." Grounded in the verified wedge (04-landscape) and footprint (03). His
bottleneck is distribution; these are the first 20-user moves, written out.

Rule before posting any of this: **fix issue #92 first.** Do not Show HN a product whose README differentiator
#2 ("review before trust") is broken end-to-end. One commenter will install, try /review, hit raw-SQL-only,
and say so in public. A half-day fix protects the whole launch.

---

## Asset A — Show HN post (the product wedge)

**Title options (pick one, keep it concrete and honest):**
- `Show HN: Origin – on-device AI memory for Claude Code, with source-cited, git-versioned pages`
- `Show HN: Local-first memory for AI coding agents that cites its sources and lets you git diff it`

**Body:**
```
Origin is a local-first memory layer for AI coding agents. It runs as a daemon on your machine
(macOS/Linux/Windows), no account, no cloud. Your agent writes what it learns; you curate it.

What's different from the cloud memory tools (mem0, Letta, Zep):

1. Provenance is enforced, not optional. Every distilled "page" cites the source memories it came
   from. The daemon refuses to store an unsourced page (HTTP 422). No silent hallucinated summaries
   entering your context.

2. Review before trust. Low-confidence captures and contradictions surface for you to accept or
   reject instead of silently entering memory.

3. It's a git repo. Memory, pages, and sessions commit into ~/.origin/.git/. You can diff, revert,
   branch, or symlink the markdown into Obsidian. Your memory has a history you can audit.

4. On-device by default. libSQL + an on-device embedder. A local LLM or an API key is optional and
   only used for distillation.

Claude Code install is two commands and /init:
  /plugin marketplace add 7xuanlu/origin
  /plugin install origin@7xuanlu
  /init

It also works as a plain MCP server in Cursor, Codex, Claude Desktop, VS Code, Gemini CLI.

I built this solo over the last six weeks. It's Apache-2.0. I'd genuinely like to hear where the
trust model breaks for you, and whether on-device + provenance is something you actually want or
just something I wanted to build.

Repo: https://github.com/7xuanlu/origin
```

Why this works (from 02-builder-benchmarks + 04-landscape): HN overindexes on open-source, privacy-first,
local tools. The honesty ("whether this is something you actually want or just something I wanted to build")
is a known HN-resonant move and doubles as the user-validation question he has never asked. Post Tue-Thu,
~8-10am ET. Reply to every comment in the first 3 hours.

VERIFY: install commands match README exactly [VERIFIED README quickstart]. The 422-on-unsourced and
git-versioning claims match README differentiators #1, #3, #4 [VERIFIED README]. Differentiator #2 (review)
claim is gated behind the #92 fix — DO NOT make the review claim until #92 ships. Flagged inline.

---

## Asset B — The field guide (bet #1, the researcher path)

Working title: **"What AI memory benchmarks actually measure, and what they hide"**

This is the highest-conviction bet (06-contrarian-bets #1). It attacks distribution with his actual edge.
The category is full of vendors quoting single cherry-picked numbers (Mem0's 93.4% LongMemEval, etc.). He has
spent weeks inside LoCoMo and LongMemEval and built citation discipline (the AGENTS.md "Eval Citation
Discipline" section) precisely about this. He is unusually qualified to write the honest version.

Outline (each section is a thing he already knows from the repo):
1. The two benchmarks everyone quotes (LoCoMo, LongMemEval): what task they actually pose.
2. The single-run trap. Why one number is marketing, not measurement. (His own rule: N>=3 + stddev. Quote his
   AGENTS.md discipline.)
3. The adversarial-category contamination he hit (LoCoMo adversarial-cat-5, named in his AGENTS.md). A real
   war story: how a headline number hid a regression.
4. Schema-version comparisons that are not comparable. Why "we beat X by 12%" across schema versions is noise.
5. What these benchmarks do NOT measure: whether anyone wants the memory, whether retrieval helps a real
   coding session, latency under load, trust.
6. A reproducible harness others can run. Point at his eval/ code. This is the credibility anchor.

Distribution: post the essay on useorigin.app/learn, then Show HN it separately from the product
("Show HN: An honest field guide to AI memory benchmarks") and cross-post r/LocalLLaMA + r/MachineLearning.
The essay sells the product without pitching it.

VERIFY: every claim in the guide must come from his own repo or a cited paper. The adversarial-cat-5 and
single-run/schema-version points are [VERIFIED present in AGENTS.md "Eval Citation Discipline"]. Do not publish
any headline accuracy number for Origin itself until it passes his own N>=3 gate — the guide's credibility
depends on him following the rule he is preaching. This is a feature: the guide can say "I won't quote you a
single-run number, here's why" and that IS the differentiator.

---

## Asset C — r/LocalLLaMA post (the ICP's home turf)

04-landscape names the ICP's hangout. This subreddit rewards local-first, anti-cloud, runs-on-my-hardware.

```
Title: I built a fully on-device memory layer for AI coding agents (no cloud, git-versioned, Apache-2.0)

Body:
Tired of cloud memory startups wanting my codebase, I built a local-first alternative. Runs as a
daemon, stores in libSQL on disk, embeds on-device. Optional local LLM (Qwen) for distillation, or
just run it embedder-only with no model at all.

The part I care about most: it's a git repo under the hood. Every memory and every distilled page
commits to ~/.origin/.git. You can git diff your AI's memory. Pages cite their source memories or
the daemon refuses to store them.

Works with Claude Code (plugin), and as an MCP server in Cursor/Codex/etc.

Honest ask: does on-device memory matter to you, or is cloud fine for this? And is provenance
(source-cited pages) something you'd actually use or just a nice idea?

[repo link]
```

VERIFY: embedder-only / no-model-required claim matches README + init SKILL ("local memory mode, no model, no
key") [VERIFIED]. Qwen-for-distillation is opt-in [VERIFIED AGENTS.md stack].

---

## Asset D — Warm outreach to the one engaged human + adjacent community

03/08 footprint: exactly one external person engaged (kiluazen, issue #194), and he referenced
`yologdev/yopedia`, a repo in the same provenance/memory space. That is not nothing. That is the seed of a
community of people who care about the exact thing Origin does best (enforceable provenance).

Moves (do these manually, they are 20 minutes total):
1. Reply substantively to issue #194. He asked whether per-claim provenance is on the roadmap vs page-level by
   design. Give him a real answer (it is a genuine design fork). This is a real user with a real question and
   it has sat for days. Answering it well is the cheapest user-retention act available.
2. Look at yologdev/yopedia. If they are solving adjacent provenance problems, open a thoughtful issue or DM.
   These are exactly the 5-10 people who would be Origin's first real users and evangelists.
3. Find the 3-5 most active people in r/ClaudeAI / the Claude Code plugin community who have complained about
   losing context between sessions. Reply with the tool, not a pitch.

VERIFY: issue #194 is real and open, asked by kiluazen, last updated 2026-05-27, unanswered substantively
[VERIFIED github MCP list_issues]. yopedia reference is in the #194 body [VERIFIED].

---

## Sequencing (one week, not all at once)

- Day 1: Fix #92 (/review). Half a day. Removes the launch landmine.
- Day 2: Reply to issue #194. Write Asset D outreach. (Outward muscle, low stakes.)
- Day 3-4: Draft the field guide (Asset B). This is the durable asset.
- Day 5: Show HN the field guide (Asset B), not the product. Lower risk, builds audience, sells product
  sideways.
- Following week, if the guide lands: Show HN the product (Asset A).

Rationale: lead with the researcher path (lower downside, plays to proven strength), let it pull attention to
the product, then launch the product into warmed air. This is the reverse of launching a cold product to
silence.

## VERIFICATION SUMMARY
- All install commands and product claims cross-checked against README / init SKILL / AGENTS.md. PASS, with one
  hard gate: the "review before trust" claim is blocked until #92 ships (flagged in Asset A).
- All traction/community facts (#194, kiluazen, yopedia, one external human) cross-checked against the live
  GitHub MCP pull. PASS.
- Not verified: actual HN/Reddit reception (unknowable pre-post). The success metric for each asset is in
  06-contrarian-bets (front page / 10k views / N installs). Treat these as experiments with kill criteria, not
  guaranteed wins.
