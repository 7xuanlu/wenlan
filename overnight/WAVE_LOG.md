# Overnight Self-Leverage Run - Qi-Xuan Lu (7xuanlu)

Operator: autonomous overnight agent. Start ~00:20 PT, 2026-05-31. Target run until ~05:00 PT.
Subject: Qi-Xuan himself - his leverage, workflow, skills, trajectory. Origin repo is evidence, not the point.

---

## STATE (updated as evidence accumulates)

**Honest verdict (one line):** He is an exceptional *process engineer* who has spent ~6 intense weeks
polishing a memory engine almost nobody is using yet - the bottleneck is not rigor, it is that he keeps
choosing inward-facing work (eval tuning, CI, SEO copy) over the terrifying outward-facing work of
putting the thing in front of users and finding a wedge.

**Highest-leverage move right now (REVISED after Wave 2 evidence):** Lead with the RESEARCHER path, not a
cold product launch. The product wedge is contested: "local-first + git + Claude Code memory" is a crowded
category with 6+ free entrants, and one (sverklo, 70 stars) has 2x Origin's stars with lower install friction.
Origin's only un-copied ground is provenance + source-cited wiki-page composition, which is also the pillar
with the weakest user demand (wedge validation: YELLOW). So: publish the honest AI-memory-benchmark field
guide (already drafted, 10-field-guide.md) to build the audience he lacks, using the rigor that is his real
edge. Let the writing pull attention to the product. Fixing the product's last gaps (review docs-sync) is a
half-day, not the headline.

**CORRECTION on a Wave 1 claim:** I earlier called his /review verb "broken end-to-end." Verified false now:
5 of 7 bugs in issue #92 are fixed in current code (router.rs:109,113,434; tools.rs:916,1616). He did the
backend work; he left the docs-sync + atomic-supersede + issue-closing undone. Pattern holds, story softened.
See 08 correction + 13.

**CONVERGENCE (4 independent agents + operator agree):** diagnosis, benchmarks, footprint, and the GitHub
#92 finding all point to one thing: he runs the effective builder loop backwards. Effective solo builders
ship an ugly wedge first, get users, then apply rigor to real pain. He front-loaded six weeks of rigor
(evals, CI, SEO, process) and skipped validation + distribution. His own core verb (/review) is broken while
he tunes a 4th retrieval variant. The strength (rigor) is real; it is just aimed inward.

**The single move, sharpened:** Pick ONE of two paths this week, not both. (a) SHIP the wedge - fix /review,
then Show HN the provenance + git-versioned Claude Code memory angle to 20 users. (b) BECOME THE
RESEARCHER - publish one brutally honest field guide to what AI-memory benchmarks actually measure, using his
real eval rigor, to build the audience he lacks. Both attack distribution (his bottleneck) with rigor (his
strength). (b) has lower downside and plays to his proven edge; (a) validates the product. Do not tune
retrieval again until one of these ships.

**Evidence so far (raw, pre-verification):**
- 314 commits / 38 active days (Apr 19 – May 30, 2026). Avg ~8 commits/active day. Real intensity.
- Commit mix: 147 fix, 43 feat, 40 chore, 39 docs, 17 refactor, 14 ci, 2 test, 2 perf. → 3.4x more fixing than featuring.
- ~115 commits (37%) on eval (32) + SEO/docs (45) + CI (38). Process/polish dominates.
- `release.yml` edited 45 times; `ci.yml` 30 times → heavy CI thrash. README 40 times.
- Last 7 days (May 24–30): almost entirely retrieval micro-tuning (#202–#214: page-channel, RRF streams, graph gates, query decomposition). Inward.

---

## WAVE INDEX
- Wave 1: Recon + diagnosis + benchmarks + landscape + footprint + automation + contrarian bets

---

## [Wave 1] Recon, diagnosis, landscape, footprint, automation, contrarian

GOALS: build an honest evidence model of how he works and where he stands; produce the first batch of
verified artifacts across all three streams (diagnosis, automation, distribution).

AGENTS DISPATCHED (6, parallel): git-forensics diagnosis · top-builder benchmarks · public footprint/traction ·
competitive landscape · AI-workflow automation kit · contrarian bets. Plus operator-authored artifacts:
onboarding audit (07), authoritative GitHub signals (08), working self-dashboard tool.

ARTIFACTS (files under overnight/):
- 01-diagnosis.md - 43% process work; 3.4 fixes/feat; ZERO of 314 commits reference a user/feedback/onboarding;
  CI thrash + README-rewrite loops; argues the 39KB AGENTS.md is itself productive procrastination.
- 03-footprint.md - 34 stars, 2 forks, 1 external human (kiluazen). origin-mcp repo archived w/ 1 star.
  0 HN / Reddit / Product Hunt / Lobsters / X posts. npm download count un-gettable (403 in sandbox).
- 04-landscape.md - WEDGE: provenance-enforced, git-versioned memory pages for Claude Code devs, fully
  on-device. Empty quadrant nobody owns: local + provenance + human-curated + git. Funded players (mem0 $24M,
  Letta $10M, Supermemory $2.6M, Cognee EUR 7.5M) are all cloud-default + auto-write. ICP: privacy-conscious
  senior eng / indie hacker living in Claude Code, burned by silent wrong memory.
- 07-onboarding-audit.md (operator) - happy path real (~30s, 2 actions); 7 enumerated leak points; the deeper
  finding is he optimizes a funnel he has no telemetry to measure.
- 08-github-signals.md (operator) - issue #92: his own /review verb (README differentiator #2) broken
  end-to-end since May 12, P1 since Apr 24, no fix PR; meanwhile shipped a 4th retrieval variant. The thesis
  in one data point.
- tools/self-dashboard.sh (operator, RUNS) - weekly inward/outward mirror.
- 02-builder-benchmarks.md - effective solo loop (Levels, Postma, Karpathy, Willison, Howard, swyx): wedge ->
  ship ugly -> users -> talk -> distribute -> THEN rigor. He runs it backwards. YC: "no market need" kills 42%.
  Evals answer "is my code correct," never "does anyone want this." HN overindexes on his exact category.
- 05-automation-kit.md - top 3: (1) /release-check skill (wraps validate-versions.sh + bump-intent audit),
  (2) /pr-hygiene skill (runs CI gates locally, dents ci.yml thrash), (3) eval-citation commit guard
  (PreToolUse). Hook event names verified vs code.claude.com/docs; JSON jq-validated. Operator correction
  appended (no ORIGIN_BASE_URL env var; use --origin-url / 7878 default).
- 06-contrarian-bets.md - top by conviction: (1) become the researcher/writer, publish honest memory-benchmark
  field guide; (2) productize the process (agent-rigor repo); (3) reposition to provenance/audit-for-agents.
  Anti-bet: keep polishing -> 6 months, ~600 commits, higher benchmark, same ~0 traction.
- 09-launch-kit.md (operator) - ready-to-post Show HN, field-guide outline, r/LocalLLaMA post, warm outreach to
  the one engaged human (#194 / kiluazen / yopedia). Gated on fixing #92 first. One-week sequence.

OPERATOR VERIFICATIONS THIS WAVE:
- MCP base-URL env var: agent unsure -> I checked source. No ORIGIN_BASE_URL exists. Uses --origin-url flag /
  7878 default (client.rs:6,18). Correction written into 05. PASS.
- validate-versions.sh: confirmed real, checks version.txt + Cargo.toml + lock + 2 npm package.json + plugin
  manifest against RELEASE_TAG. PASS (matches automation agent's description).

WAVE 1 STATUS: COMPLETE. 6 agents + 4 operator artifacts + working tool. All findings converge on one verdict.

---

## [Wave 3] Deep competitive + launch mechanics + decision memo (in progress)

GOALS: pin down the competitive reality and the launch mechanics, and synthesize everything into a
top-of-kit decision memo.

AGENTS DISPATCHED (2): competitor deep-dive (17) · dev-tool launch playbook (18). Both running.

OPERATOR ARTIFACTS:
- 00-START-HERE.md - the decision memo. One-line verdict, compressed verified evidence, the ONE decision
  (researcher path A vs ship-to-20 path B; recommend A then B), and the plain statement of what he avoids.
- 14-content-engine.md (agent, landed) - "Commit to Post" loop <30min/wk; 12 dated posts each tied to a real
  commit; first piece = the LoCoMo/LongMemEval honesty teardown.
- 15-skill-gap-trajectory.md (agent, landed) - recommended path: research engineer / applied scientist at an
  AI lab or memory-infra co; his eval rigor matches Anthropic Model Evaluations role nearly word-for-word;
  weaknesses off the critical path. Rust+on-device+retrieval is a scarce, high-paid stack.
- automations/competitor-radar/ (operator) - radar.sh + competitor-radar.yml. RAN LIVE: surfaced 15+ rival
  memory tools, a dozen with more stars than Origin. Tool paid for itself on first run. Folded into 16.

VERIFICATION: radar.sh - bash -n clean, YAML parses (python yaml.safe_load), LIVE run returned a real ranked
table from the GitHub API. PASS (executed, not asserted). The finding (Origin ranks ~13th) is now the
strongest single argument for the researcher path.

WAVE 3 COMPLETIONS:
- 17-competitor-deepdive.md - sverklo is top threat (70*, MIT, npx, ships lineage-preserving consolidation +
  git-SHA-pinned decisions, one feature from closing Origin's moat). Origin owns exactly 2 cells: enforced
  provenance + distilled source-cited wiki pages; eval discipline near-unique. Flagged agentmemory's 20k stars
  as anomalous/marketing-amplified, sverklo's "43x tokens / 0.58 F1" as unconfirmed. Good skepticism.
- 18-launch-playbook.md - Show HN mechanics (188k-post timing analysis), 5 real post-mortems (incl. MemPalace
  7k-stars-then-debunked = matches our inflated-star caution), per-channel etiquette, hour-by-hour runbook.
  Highest-impact mechanic: be present and reply to every comment in the first ~3h (star impact ~92% spent in
  48h, half-life ~24h). All tagged VERIFIED/INFERRED/OPINION.

OPERATOR FACT-CHECK ON THE FLAGSHIP ASSET (field guide, 10):
- Independently web-verified the essay's most load-bearing external claim (Zep LoCoMo correction). Found the
  agent's "84% -> 58.44%" was a real but INCOMPLETE cut: the dispute is three-way (Zep 84, Mem0 recompute
  58.44, Zep corrected 75.14 +/- 0.17). For an essay about benchmark honesty, citing only the Mem0-favorable
  number would itself be the cherry-pick it condemns. FIXED line 60 + the source footer to present all three
  and added a self-aware beat. This is the single most important correction of the run for a publishable asset.
  [VERIFIED via WebSearch + getzep blog + zep-papers issue #5]

---

## [Wave 4] Productize the process + make the field guide self-citable + the kill list (in progress)

GOALS: keep producing buildable/decision artifacts. Turn the rigor into a shippable product; give the field
guide a way to honestly cite Origin's own number; tell him what to STOP.

AGENTS DISPATCHED (2): agent-rigor scaffold (bet #2, real files in overnight/agent-rigor/) · citable-number
multi-run eval protocol (20).

OPERATOR ARTIFACT:
- 21-stop-doing.md - ranked subtraction list. Top 3 to stop this week: retrieval tuning, hand-editing CI,
  README/SEO rewrites. Each cites a measured git-churn number; honest about what NOT to stop (engineering
  quality, eval rigor aimed outward, the provenance idea).

WAVE 4 COMPLETIONS:
- 20-citable-number-protocol.md - runnable N>=3 protocol for one honest headline. SHARP code-grounded catch:
  the LoCoMo harness does NOT compute QA accuracy (qa_accuracy: None, locomo.rs:321-323); only NDCG@10 is
  serializable, so calling it "accuracy" would itself break the discipline. Recommends LoCoMo base / NDCG@10
  (no API key, no GPU model). Python aggregator (mean, N-1 stddev, Student-t 95% CI) included. 2 over-broad
  AGENTS.md claims flagged.
- agent-rigor/ - README, AGENTS.template.md (13 [VERIFIED AGENTS.md section] tags, no Origin identifiers
  survive grep), docs/eval-citation-discipline.md, docs/test-layers.md, generalized hooks + setup-hooks.sh,
  WHY.md. The reusable crown jewel is the eval-citation-discipline doc.

OPERATOR VERIFICATION: release-check + pr-hygiene skills cross-checked against real interfaces - validate-
versions.sh takes RELEASE_TAG env (script line 5), the skill sets it (line 38); pr-hygiene boundary grep
matches AGENTS.md "no tauri/axum in origin-core" verbatim. PASS.

WAVE 4 STATUS: COMPLETE.

---

## DRAFT PR OPENED

Branch `overnight/self-leverage-kit` (commit 0fa6b7f) pushed; draft PR #217 opened, docs-only, titled
"docs: overnight self-leverage kit (DO NOT MERGE)". Verified: `git diff --cached` showed ONLY overnight/
staged, no product code touched. local == remote == 0fa6b7f, all 36 files present in the pushed tree.
https://github.com/7xuanlu/origin/pull/217

---

## [Wave 5] Red-team my own conclusions + wire the weekly routine + concretize the role path (in progress)

GOALS: this run has been hard on him and confident in its verdict. Rigor demands I attack my own conclusions,
build the one automation that ties the kit together (a scheduled weekly self-review), and turn the abstract
"research-engineer path" into concrete, actionable next steps.

AGENTS DISPATCHED (2): research-engineer path concretizer (real roles + portfolio gap) · a second-opinion
adversarial reviewer of the kit's own claims. OPERATOR: steelman "keep building Origin" + weekly-routine
automation.

---

## [Wave 2] Build the durable assets + make the right work measurable (dispatched)

GOALS: stop diagnosing, start producing buildable/publishable artifacts. (a) write the actual field-guide essay
(durable researcher asset, bet #1); (b) extract the top-3 automations into real tested files; (c) validate or
kill the wedge with demand evidence; (d) turn the user-facing work he avoids into something measurable
(review-flow fix plan + a benchmark for it); (e) a build-in-public content engine from his real history; (f)
skill-gap + trajectory with market evidence.

AGENTS DISPATCHED (6, parallel): field-guide essay · automation file-builder · wedge demand-validation ·
review-flow fix-plan + benchmark · content engine · skill-gap/trajectory.

ARTIFACTS:
- 10-field-guide.md - DONE. 2205-word publishable essay "What AI memory benchmarks actually measure, and what
  they hide." Honest LoCoMo/LongMemEval explainer; 4 traps anchored to his own AGENTS.md discipline rules;
  the Zep 84%->58.44% correction + cat-5 contamination grounded; closes refusing to quote a single-run number
  for his own tool. 13 VERIFIED URLs; discipline quotes map to real AGENTS.md lines. This is the durable asset.
- automations/ - DONE + operator-verified. release-check + pr-hygiene SKILL.md, eval-citation-guard hook
  (JSON+sh). I independently ran: jq valid, bash -n clean, and EXERCISED the hook (deny bare metric / allow
  with provenance / allow non-metric) - all pass. Real working artifact.
- 12-wedge-validation.md - DONE. Verdict YELLOW. Pain is real + loud (Claude Code 4.2M WAD, Ollama 52M/mo
  downloads, context-loss complaints stated). But provenance = weak stated demand; market ~indie-scale
  ($120K-900K ARR ceiling), not venture.
- 16-competitive-reality.md (operator) - crowded lane confirmed via live GitHub search. sverklo 70*, ghost
  11*, n2n 5*, pebble/onebrain/vibemem. Origin (34*) not leading. 3 of 4 pillars are table stakes.
- 13-review-fix-plan.md - DONE. Re-read current code: only 2 of #92's 7 bugs remain (docs-sync, atomic
  supersede). Plus a designed `eval::review_curation` benchmark so the user-facing flow becomes measurable.
- 14-content-engine.md, 15-skill-gap-trajectory.md - agents running.

OPERATOR VERIFICATIONS THIS WAVE:
- Automation hook: RAN 3 test cases, behavior exactly as specified. PASS (strongest kind of verify: executed).
- Competitor claim: agent's specific example (yuvalsuede/memory-mcp) UNCONFIRMED in my search; replaced with
  6 verified repos that confirm the pattern more strongly. Honest substitution.
- #92 status: agent said 5/7 fixed; I spot-checked router.rs + tools.rs + db.rs and confirmed. Corrected my
  own earlier overstatement in 08. PASS, and self-correction logged.

VERIFICATION:
- self-dashboard.sh: RAN successfully. 7-day output: 71% inward, 0 user mentions, days-since-feat 7. 30-day:
  46% inward - independently matches diagnosis agent's 43% process figure (two methods agree). PASS.
  KNOWN LIMITATION: the 30-day "user mentions: 5" are grep false positives (matches `user_edited` etc.); the
  7-day 0 is accurate. Metric is a directional mirror, not exact. Documented, not hidden.
- Diagnosis vs operator git counts: both land at ~43-46% inward independently. CONVERGENT. PASS.
- Footprint numbers: GitHub stars/issues cross-checked against my own GitHub MCP pull (4 open issues, 1
  external human) - CONSISTENT. PASS. npm downloads: FAIL to obtain, 403 confirmed twice (network policy),
  flagged not fabricated.
- Landscape funding figures: agent tagged each [VERIFIED url]; not independently re-checked this wave (queued).
- #92 finding: read directly from GitHub MCP issue body. PASS, primary source.
