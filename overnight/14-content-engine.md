# Content Engine for Qi-Xuan Lu (7xuanlu): Build-in-Public Loop

Goal: turn work he is already doing into a compounding audience. His bottleneck is distribution (see `01-diagnosis.md`, hard truth #1). His strength is rigorous systems and eval work. He has 314 commits of raw material and has shipped zero public content from it. This document is the loop, the calendar, the channels, and the first move.

Builds on: `01-diagnosis.md` (strengths and avoidance), `04-landscape.md` (the wedge: local-first, provenance, git-versioned memory), `06-contrarian-bets.md` (Bet 3, become the researcher/writer, highest conviction). Does not re-derive those.

Tags: [VERIFIED url] sourced fact. [INFERRED] reasoned. [OPINION] my judgment.

---

## 0. The model he is copying

Two people turned ongoing work into a compounding audience. Both methods are cheap and repeatable.

**Simon Willison (simonw).** His rule: write about what you learn and what you build, neither of which needs a novel insight. He keeps a dedicated TIL ("Today I Learned") blog with 576+ short posts, each one a thing he figured out that day, often just a few paragraphs. The byproduct: deep searchable notes on everything, and a name that ranks. [VERIFIED https://til.simonwillison.net/] [VERIFIED https://simonwillison.net/] His own advice on what to blog about: TILs and project write-ups. [VERIFIED https://writethatblog.substack.com/p/simon-willison-on-technical-blogging] The TIL repo is public and mechanical. [VERIFIED https://github.com/simonw/til]

**swyx (Learn in Public).** His rule: "Find something you can't stop thinking about and know it better than anyone, and share everything you learn along the way." And: "Pick up what they put down" — answer the open questions more senior people leave lying around. [VERIFIED https://www.swyx.io/learn-in-public] [VERIFIED https://www.swyx.io/puwtpd]

The shared mechanic: **the work is the content.** You do not invent a topic. You narrate what you were already doing. That is the entire trick, and it is exactly the trick a rigor-heavy solo dev with 314 unmined commits should run. [OPINION]

---

## 1. The loop: "Commit to Post" (under 30 min/week)

A named, weekly cadence that piggybacks on work he already does. His commit messages average 27 lines of body (per `01-diagnosis.md`) and his `/handoff` skill already writes a narrative session log to `~/.origin/sessions/` plus a status file. [VERIFIED `plugin/skills/handoff/SKILL.md` — "Writes a narrative session log to ~/.origin/sessions/"] Those are first drafts he is throwing away. The loop reuses them.

**Weekly (pick ONE, ~25 min):**

1. **Friday TIL.** Open the week's `git log`. Find the one commit with a real "I learned X" inside it (the lock race, the await-guard bug, the RRF tweak). The commit body IS the draft. Paste it, cut the repo-internal jargon, add one "here is the general lesson" paragraph and one code snippet. Ship to the TIL blog. 200-400 words. No intro, no conclusion, no SEO. simonw-style. [VERIFIED method https://writethatblog.substack.com/p/simon-willison-on-technical-blogging]
2. The raw material is already written. The `/handoff` session log and the commit body are the draft. The job is editing, not authoring. That is what keeps it under 30 min. [INFERRED from handoff SKILL.md + commit body length in 01-diagnosis.md]

**Monthly (one longer piece, ~3-4 hrs, batched on a weekend):**

- One opinion or teardown piece that strings 3-4 of the month's TILs into an argument. This is the swyx "know it better than anyone" piece. At least one per quarter must be a memory-benchmark-honesty piece (feeds Bet 3).

**The discipline rule (the simonw rule, adapted):**

> Every interesting thing I fix or learn, I post. If the commit body is good enough to write, it is good enough to publish. I do not wait for the insight to be novel. I ship the note.

**Why this fits him specifically:** he already over-documents. `01-diagnosis.md` calls his 27-line commit bodies and 432-line AGENTS.md "dissertation-grade documentation for an audience that does not exist yet." The loop gives that audience an address. The cost is near zero because the writing already happened. [OPINION grounded in 01-diagnosis.md sections 8 and the hard-truths list]

**The one habit to add:** at the end of each `/handoff`, tag one line in the session log with `#til`. Friday, grep for `#til`. That is the whole intake pipeline.

---

## 2. The 4-week starter calendar

12 posts. Every one cites a real commit SHA or file, verified with `git`. Mix of TIL and opinion. Two-plus feed Bet 1 (memory-benchmark honesty). Roughly 3 posts/week; he ships the TILs and batches the opinion pieces.

### Week 1 — the systems bugs (warm up, pure TIL, lowest risk)

**1. "A file lock for a race that only happens when your test runner forks."** [TIL]
Angle: `cargo nextest` forks a process per test; three processes raced on the shared FastEmbed model cache and one randomly failed to load `model_optimized.onnx`. Fix: an OS-level exclusive file lock (`std::File::lock`, stable since 1.89) outside the process-local mutex. The general lesson: a `std::sync::Mutex` protects within a process, not across forked ones.
Cite: commit `d7aaaab` (#125), `crates/origin-core/src/db.rs`. [VERIFIED `git show d7aaaab`]

**2. "Never hold a tokio RwLock guard across .await (I learned this five times)."** [TIL + mild opinion]
Angle: holding a read guard during a multi-second LLM/db call blocks every writer. The fix is one pattern: clone the `Arc<MemoryDB>` inside a scoped block so the guard drops before the await. The honest part: he had to sweep the same bug across handlers in three separate PRs. Good post because the repeated fix is the real story.
Cite: commits `226ae8d` (#129), `7236eeb` (#131), `39a600d` (#136), `crates/origin-server/src/memory_routes.rs`. [VERIFIED `git show 7236eeb 39a600d`]

**3. "Hybrid search in libSQL: vectors + FTS5 + reciprocal rank fusion in one SQLite file."** [TIL/project]
Angle: how `search_memory` combines a `F32_BLOB(768)` vector column, an FTS5 virtual table kept in sync by triggers, and RRF to merge the two ranked lists, all in Turso's libSQL with no external vector DB. Includes the detail that standard RRF `1/(k+rank)` "barely differentiates in small pools" so he tuned it.
Cite: `crates/origin-core/src/db.rs:6755` (`search_memory`, doc comment "Hybrid search (vector + FTS + RRF)") and `:6965` (the RRF small-pool note). [VERIFIED `grep -n` in db.rs]

### Week 2 — cross-platform and hardware reality (TIL, broad appeal)

**4. "Shipping a Rust daemon on Windows without writing a Windows Service."** [TIL/opinion]
Angle: origin-server is a plain console app. Under `sc.exe` + the Windows Service Control Protocol it times out at 30s because it has no service dispatcher. The escape: register a per-user Task Scheduler ONLOGON task via `schtasks.exe` and short-circuit the service-manager path entirely. Concrete, rarely-written-up, ranks well.
Cite: commit `ed9b96f` (#162), and `AGENTS.md` cross-platform table. [VERIFIED `git show ed9b96f`]

**5. "When ggml_metal_init fails but Metal actually works: build the auto-degrade path."** [TIL]
Angle: on macOS Tahoe 26.x, Metal context creation can fail even though native Metal is fine. Instead of crashing, probe Metal context creation first; if it fails, log and run without the LLM. The lesson: on-device inference needs a graceful-degradation probe, not an assumption.
Cite: `crates/origin-core/src/engine.rs:163` (the probe doc comment "Used by the auto-degrade pattern") and `:483`. [VERIFIED `grep -n` in engine.rs] AGENTS.md "Metal/ggml on macOS Tahoe" note. [VERIFIED]

**6. "I wrote 432 lines of rules to keep my AI coding agent honest. Here they are."** [OPINION — feeds Bet 2]
Angle: the AGENTS.md discipline as a public artifact. The L1-L8 test taxonomy, "verify before claiming done," "surgical changes," clone-before-await. Frame per Bet 2: the methodology is the asset. This is the post that tests whether the process work has an audience.
Cite: `AGENTS.md` (the whole file), `01-diagnosis.md` section 8. [VERIFIED file exists, 432 lines]

### Week 3 — memory-benchmark honesty (Bet 1 core, the high-travel pieces)

**7. "What LoCoMo and LongMemEval don't tell you about AI memory."** [OPINION — Bet 1, flagship]
Angle: the independent, no-vendor-axe teardown. He built citation discipline INTO his harness: a single-run rule (never cite a single-run number externally), a schema-version rule (refuse cross-schema comparisons, exit code 2), receipt-only claims (no "+X%" without N≥3 + stddev). The whole category quotes cherry-picked single numbers. He refuses to. That refusal is the post.
Cite: `AGENTS.md` "Eval Citation Discipline" section, `crates/origin-core/src/eval/AGENTS.md`, `docs/eval/README.md`. [VERIFIED files exist] Contrast with Mem0's "+26% accuracy" headline. [VERIFIED https://mem0.ai/blog/state-of-ai-agent-memory-2026]

**8. "My retrieval tuning regressed the benchmark. I'm publishing it anyway."** [OPINION — Bet 1, the honesty flex]
Angle: he added a page-channel as a 4th RRF stream and his own eval said it regressed both benchmarks ("Phase 2 evals at PAGE_CHANNEL_LIMIT=10 regressed both benchmarks"). Most builders bury that. Publishing the negative result, with the per-case reasoning, is the rarest and most trust-building thing in this category. Directly executes Bet 3's "publish findings that may undercut your own product."
Cite: commit `7d16b41` (#203), and the regression bodies quoted in `01-diagnosis.md` section 7. [VERIFIED `git show 7d16b41`]

**9. "Query decomposition: when one embedding starves a multi-hop question."** [TIL/research]
Angle: a single embedding of "what did X say about Y after Z happened" gets dominated by one clause's vocabulary, drowning memories that satisfy the others. Fix: `search_memory_decomposed` splits the query into independent factual subqueries via the LLM, searches each, RRF-merges. Distinct from query expansion (which paraphrases one clause). Graceful fallback to single-query search if the LLM is absent.
Cite: commit `25e573a` (#214), `crates/origin-core/src/retrieval/`. [VERIFIED `git show 25e573a`]

### Week 4 — the wedge and synthesis (project + opinion, ties to product)

**10. "Memory you can git diff: provenance-enforced, source-cited, on your machine."** [OPINION/project — the wedge from 04-landscape.md]
Angle: the landscape teardown's central claim. Every funded competitor auto-writes opaque blobs to a cloud. Origin makes every remembered fact link to its source, gates it behind human review, and stores the whole thing as a git repo you can `git diff`. The empty quadrant: local-first AND provenance AND human-curated AND git-versioned.
Cite: `04-landscape.md` sections 3-4, `crates/origin-core/src/pages.rs` (source-backed pages), `plugin/skills/handoff/SKILL.md` (the capture ritual). [VERIFIED files exist]

**11. "Coalescing concurrent writes: a request batcher for an LLM-in-the-loop store path."** [TIL]
Angle: concurrent `/api/memory/store` calls each trigger an expensive classify+extract. The ingest batcher folds the quality gate inline and coalesces requests so the LLM work happens once, then passes the enrichment + hint back through the response.
Cite: `crates/origin-server/src/ingest_batcher.rs`, AGENTS.md module table entry. [VERIFIED file referenced in AGENTS.md]

**12. "Six weeks, 314 commits, zero readers: a build-in-public starting line."** [OPINION — the meta-post, swyx-style]
Angle: the honest origin story. He built rigorously and in private and got no distribution. This post is him starting to learn in public, naming the loop above, and committing to it. swyx's "share everything you learn along the way" as a public promise. Pins the series and gives people a reason to follow.
Cite: the repo itself, `git rev-list --count HEAD` = 314 (per `01-diagnosis.md`). [VERIFIED count in 01-diagnosis.md]

---

## 3. Channel plan and cross-post etiquette

**Home base first.** Every piece lives on his own surface first: a TIL section on his personal blog or under `useorigin.app/learn`. Own the canonical URL, then syndicate. This is the simonw model: the blog is the asset, social is the funnel. [VERIFIED https://simonwillison.net/] [OPINION on ordering]

| Piece | Primary home | Syndicate to | Notes |
|---|---|---|---|
| TILs (#1, #3, #5, #9, #11) | Blog / `/learn` | r/rust (#1, #2, #11), X thread | Rust TILs do well in r/rust. Post the lesson, link the canonical URL at the end, not the top. |
| Cross-platform (#4) | Blog | r/rust, HN (as a "TIL"-flavored Show/ask is weak; better as a blog link if it gains traction) | schtasks-vs-service is genuinely under-documented. |
| Metal degrade (#5) | Blog | r/LocalLLaMA, X | LocalLLaMA cares about on-device inference reality. |
| Methodology (#6) | Blog | HN ("Show HN" weak here; submit as a link), r/ExperiencedDevs, X | This is the Bet 2 test. Watch star/inbound signal. |
| Benchmark honesty (#7, #8) | Blog / `/learn` | HN (front-page candidate), r/LocalLLaMA, Lobsters, X | These are the flagship Bet 1/Bet 3 pieces. Highest travel potential. |
| Decomposition (#9) | Blog | r/LocalLLaMA, r/MachineLearning (careful, stricter), X | Frame as retrieval research, not product. |
| Wedge (#10) | `useorigin.app/learn` | HN (Show HN appropriate here since there's a product), r/LocalLLaMA | Only piece where a Show HN framing is honest. |
| Meta (#12) | Blog | X, his existing GitHub README links | Low-stakes, sets the table. |

**Cross-post etiquette (the realistic version):**

- **Canonical URL on your domain, always.** Reddit/HN/X point back. Never make the Reddit post the only home. [OPINION, standard practice]
- **Reddit: write the post natively, link at the bottom.** A bare link with no body reads as self-promo and gets removed in r/rust and r/LocalLLaMA. Put the actual lesson in the post; the link is "full write-up here." Read each sub's self-promo rule; r/LocalLLaMA tolerates project posts, r/MachineLearning does not.
- **HN: submit the blog URL directly, do not editorialize the title.** Use the real title. Be in the thread for the first two hours to answer. Do not submit your own stuff more than ~once a week. The benchmark-honesty pieces (#7, #8) are the HN front-page candidates; spend HN's goodwill on those, not on every TIL.
- **X: thread the TIL, screenshot the code/diff, link the canonical URL in the last tweet.** "Pick up what they put down" (swyx): reply to people complaining about AI memory hallucination or eval dishonesty with your relevant post. [VERIFIED https://www.swyx.io/puwtpd]
- **Don't blast all channels for every post.** TILs → blog + maybe r/rust. Save the multi-channel push for the monthly opinion piece. Frequency self-promo on HN/Reddit burns the account.

---

## 4. The single highest-ROI piece to write first

**Write #7 first: "What LoCoMo and LongMemEval don't tell you about AI memory."**

Why, concretely:

1. **It attacks the real bottleneck with the real strength.** `06-contrarian-bets.md` ranks Bet 3 (become the researcher/writer) as highest conviction: "directly attacks your real bottleneck (distribution) using your real strength (rigor), costs one week, almost no downside." This piece IS Bet 3's smallest experiment, almost verbatim. [VERIFIED 06-contrarian-bets.md conviction ranking]
2. **The material already exists and almost nobody else has it.** He built citation discipline into the harness: single-run rule, schema-version refusal, receipt-only claims. The category is full of vendors quoting one cherry-picked number (Mem0's "+26%"). An independent teardown with no product to sell travels. [VERIFIED AGENTS.md "Eval Citation Discipline"; https://mem0.ai/blog/state-of-ai-agent-memory-2026]
3. **It is the highest-ceiling HN/LocalLLaMA piece.** Bug TILs (#1-#5) are good warm-ups but low ceiling. The benchmark-honesty teardown is the one with front-page potential and the one that earns him a name as the honest voice on memory evals. That reputation is the compounding asset everything else attaches to. [OPINION]
4. **It does not require him to ship anything new.** No code, no product polish, no CI. It is pure write-up of work already done. That removes his usual escape hatch (go polish instead of ship). [OPINION grounded in 01-diagnosis.md avoidance pattern]

The honest caveat: his own citation rules forbid quoting his single-run numbers as results (`01-diagnosis.md` section 8). That is fine and actually the point. The piece is not "here are my SOTA numbers." It is "here is why the numbers everyone quotes are mostly theater, and here is the discipline it takes to quote them honestly." The refusal to cite is the credibility. Lead with the methodology, not a score.

Warm up with #1 or #3 the same week (one afternoon each) so he has a TIL cadence going before the flagship drops. Then #8 (the published-regression piece) is the natural follow-up that locks in the honest-researcher brand.

---

## Sources

- Simon Willison TIL blog: https://til.simonwillison.net/
- Simon Willison main blog: https://simonwillison.net/
- Simon Willison on what to blog (TILs + projects): https://writethatblog.substack.com/p/simon-willison-on-technical-blogging
- simonw/til repo: https://github.com/simonw/til
- swyx, Learn in Public: https://www.swyx.io/learn-in-public
- swyx, Pick Up What They Put Down: https://www.swyx.io/puwtpd
- Mem0 "+26%" headline / state of memory: https://mem0.ai/blog/state-of-ai-agent-memory-2026

Repo artifacts (verified with git in this repo):
- `d7aaaab` (#125) FastEmbed cross-process file lock
- `226ae8d` (#129), `7236eeb` (#131), `39a600d` (#136) clone-Arc-before-await sweep
- `7d16b41` (#203) page-channel 4th RRF stream (regressed)
- `25e573a` (#214) query decomposition
- `ed9b96f` (#162) Windows schtasks install
- `crates/origin-core/src/db.rs:6755`/`:6965` hybrid search + RRF
- `crates/origin-core/src/engine.rs:163`/`:483` Metal auto-degrade probe
- `crates/origin-server/src/ingest_batcher.rs` request coalescer
- `plugin/skills/handoff/SKILL.md` session-log raw drafts
- `AGENTS.md`, `crates/origin-core/src/eval/AGENTS.md`, `docs/eval/README.md` eval citation discipline
