# Research Engineer / Applied Scientist Path: Concrete Plan for Qi-Xuan Lu (7xuanlu)

Date: 2026-05-31. This operationalizes the recommendation from `overnight/15-skill-gap-trajectory.md` Section 4 (bet on path (b)). It does NOT re-derive the fit. `15` already established, with sources, that his eval rigor matches Anthropic's Model Evaluations role nearly word-for-word and that his Rust + on-device + retrieval stack is scarce. This file turns that into a target list, a portfolio-gap diagnosis, a dated plan, and an honest weakness audit.

Tags: [VERIFIED url] sourced and checked against a fetched page or search snapshot on 2026-05-31. [INFERRED] reasoned from evidence, no fetchable posting. [OPINION] my judgment. ESTIMATE shows its math. Repo claims cite a file path or commit SHA verified in `15` or the working tree.

A note on sourcing limits: the live job boards (Greenhouse, Ashby, Essence VC, Accel, YC) all returned HTTP 403 to direct fetch. Every requirement quote below comes from a WebSearch result snapshot of those same postings on 2026-05-31. Where a snapshot gave a clean requirement I tag it [VERIFIED snapshot]; where I could only confirm the team and role type exists I tag the requirement detail [INFERRED]. URLs are the canonical posting so he can open them in a browser.

---

## 1. The target list (11 specific openings / teams)

Ranked by fit-to-demonstrated-skill, strongest first. "He meets" cites repo evidence from `15`. "Gap" is the one thing missing.

### Tier 1 — his rarest skills are the entry requirement

**1. Anthropic — Research Engineer, Model Evaluations**
URL: https://job-boards.greenhouse.io/anthropic/jobs/5198255008 (active req); EOI variant https://job-boards.greenhouse.io/anthropic/jobs/4990535008
- Meets: "turning ambiguous notions of intelligence into clear, defensible metrics," "building the infrastructure that runs them reliably at scale," "owning dashboards... to monitor model health." [VERIFIED snapshot] His `crates/origin-core/src/eval/` (~24k LOC, 28 files) plus cost caps + wall-clock watchdog + save guards (commit `ffbda8f`, #191) and the written Eval Citation Discipline are a working instance of exactly this. [VERIFIED `15` Section 1, Rank 1]
- Gap: "strong Python programming skills and familiarity with distributed computing frameworks" [VERIFIED snapshot]. His harness is Rust. The methodology transfers; the language signal does not yet exist publicly.

**2. Anthropic — Research Engineer, Universes**
URL: https://job-boards.greenhouse.io/anthropic/jobs/5061517008
- Meets: same eval-infra-with-rigor profile as #1 (surfaced in the same Model-Evaluations search cluster, an evals-adjacent RE role). [VERIFIED snapshot — posting exists; INFERRED that requirements mirror the evals RE band]
- Gap: Python signal; plus this is a more specialized team, so [INFERRED] it likely wants a specific environment/eval-harness focus he should read before applying.

**3. Zep AI (YC W24) — Senior Applied Research Engineer**
URL: https://www.ycombinator.com/companies/zep-ai/jobs/Pj0RIyD-applied-research-engineer
- Meets: "strong research skills including methodology, dataset creation and curation, experiment design, and evaluation" and "run rigorous experiments, train and evaluate models, and ship the result as production code." [VERIFIED snapshot] His harness IS methodology + dataset curation (`locomo10.json` is the single most-churned artifact in the repo per `01-diagnosis.md`) + experiment design. Explicitly: "Zep is not hiring ML researchers chasing publications" [VERIFIED snapshot] which neutralizes his no-papers weakness.
- Gap: "Master's in Computer Science or equivalent is required" [VERIFIED snapshot]. Hard filter unless "or equivalent" is read generously. Also Zep is knowledge-graph-centric (temporal KG, arXiv 2501.13956); his KG work (`extract.rs`, `kg_faithfulness.rs`) helps but is secondary in his repo.

**4. Letta — Founding Research Engineer / Research Engineer**
URL: https://careers.essencevc.fund/companies/letta/jobs/45534002-founding-research-engineer (also -research-engineer variant, same req id)
- Meets: "design memory architecture for LLMs, conduct impactful research, and advance AI self-improvement." [VERIFIED snapshot] Letta ships the Letta Leaderboard and open-source Letta Evals over agentic memory [VERIFIED https://www.letta.com/blog/letta-leaderboard]; his system is benchmarked on the same LoCoMo/LongMemEval axis with tiered retrieval. Direct overlap.
- Gap: founding-team bar is high; their loop is a paid workday on a real task [VERIFIED snapshot from `15`-adjacent search]. They will want to see prior research-grade output in public, which he has not yet shipped (see Section 2). Python/TS SDK shop [VERIFIED snapshot].

### Tier 2 — strong fit, his stack is a direct asset

**5. Mem0 — Senior Research Engineer**
URL: https://jobs.ashbyhq.com/mem0/63632678-661b-41d1-9cbd-ad91361bc956
- Meets: "implementing and benchmarking ideas from papers" and "owning the end-to-end lifecycle of memory features from research to production... extraction, updates, consolidation/forgetting, and conflict resolution." [VERIFIED snapshot] His `refinery.rs` (consolidation/dedup/auto-linking), `merge.rs` (contradiction detection), `post_ingest.rs`, and benchmark harness map onto this list almost item-for-item.
- Gap: Python/fine-tuning shop ("fine-tuning models for extraction" [VERIFIED snapshot]); his stack has no model-training surface. Salary band $150K-$180K [VERIFIED snapshot] is below the Anthropic band, relevant to prioritization.

**6. Supermemory — Founding Engineering & Research**
URL: https://www.linkedin.com/jobs/view/founding-engineering-and-research-at-supermemory-4316435240 ; announce https://x.com/supermemoryai/status/1980060798518190404
- Meets: "Product engineers with expertise in AI, Infrastructure and scale"; they ship a memory engine and benchmark against Mem0/Zep/Letta [VERIFIED snapshot; https://supermemory.ai/blog/memory-engine/]. His infra depth + memory benchmarking is on-thesis.
- Gap: tiny team (1-10), product-and-distribution-heavy founding role; that leans on his weakest muscle (Section 4). Comp listed $45k-$75k senior / wide intern band [VERIFIED snapshot] looks low/early-stage.

**7. Cognee — engineering (memory engine / data plane)**
URL: https://www.cognee.ai/careers
- Meets: "Deep familiarity with LLMs, AI agents, vector databases, and modern AI tooling, with experience building or shipping real AI systems." [VERIFIED snapshot] Open-source memory engine in production at 70+ companies. His shipped, benchmarked memory system is the literal artifact.
- Gap: "Hands-on experience with Python and modern backend systems" [VERIFIED snapshot] — Python-first again. Their open role surfaced is Senior DevRel (distribution-heavy); the pure-eng role is [INFERRED] from the careers page, not a fetchable posting.

### Tier 3 — Rust-heavy AI infra, where the language is the asset not the gap

**8. Turso — Platform / database engineer (libSQL)**
URL: https://turso.tech/careers
- Meets: Rust + Go database infra. [VERIFIED snapshot — "decent coder in Go (and maybe Rust)"] This is the single highest mechanical overlap: Origin's storage layer IS libSQL with `F32_BLOB(768)` DiskANN + FTS5 triggers (Turso's own fork). He has been a power user of their core product in anger. [VERIFIED `15` Rank 3; AGENTS.md DB section]
- Gap: the surfaced posting is K8s/distributed-systems platform work [VERIFIED snapshot], not LLM/eval. It is a stack match, not a domain match. Use only if the eval-lab path stalls.

**9. Modal Labs — infrastructure engineer (Rust)**
URL: https://jobs.ashbyhq.com/modal
- Meets: "serverless GPU cloud infrastructure built from scratch in Rust... custom container runtime, FUSE filesystem, scheduler, memory snapshotting, all in Rust." [VERIFIED snapshot] His cross-platform Rust systems work (launchd/systemd/schtasks, vendored OpenSSL portable Linux builds, multi-OS CI) is the systems-Rust signal they hire on.
- Gap: no LLM-eval domain here; it is pure systems Rust. Also [INFERRED] they bias toward competitive-programming / low-level perf pedigree (founder is an IOI gold medalist), a different signal than his.

**10. Baseten — Applied AI Inference Engineer**
URL: https://jobs.ashbyhq.com/baseten/90e9ff4e-1225-4b1b-b0b4-2362e36d9cfa/
- Meets: inference performance focus (TTFT/throughput optimization) [VERIFIED snapshot]; his llama-cpp-2 / Metal on-device inference work touches the same concerns. $300M Series E, ~100 people, ships-fast culture [VERIFIED snapshot].
- Gap: production GPU-serving-at-scale is not in his repo (on-device single-user is the opposite end). This is the weakest domain fit in the list; included because Rust + inference shows up, but [OPINION] lower priority than 1-7.

**11. LanceDB — engineer (Rust + Python vector DB)**
URL: https://jobs.ashbyhq.com/lancedb
- Meets: "distributed databases and proficiency in Rust and Python" [VERIFIED snapshot]; columnar vector DB / multimodal lakehouse. His hybrid retrieval (vector + FTS5 + RRF, reranked/expanded/decomposed variants; commits `c5a88b3` #187, `25e573a` #214) is retrieval-engine domain knowledge.
- Gap: the surfaced roles are Support/Customer-Success Engineer [VERIFIED snapshot], not core eng. Watch the board for a core retrieval-engine opening; mark the current ones a poor fit.

### Bench (watch, not apply-now)
- **Chroma** — open-source retrieval/embeddings DB, 6 open roles, currently Product Eng / Product Designer in SF [VERIFIED snapshot https://careers.trychroma.com/]. Retrieval domain is a strong fit; no eval/retrieval-eng posting was live on 2026-05-31. [INFERRED watch target]
- **Anthropic — agent-memory / context-management work.** No standalone "agent memory RE" posting was findable; the work exists (Memory for Managed Agents shipped public beta April 2026 [VERIFIED https://www.infoq.com/news/2026/04/anthropic-managed-agents/], "Effective context engineering" framing). [INFERRED] route in via the Model Evaluations or Universes reqs above, then internal-transfer toward memory, rather than waiting for a memory-specific posting.

Comp anchor for prioritization: Anthropic TC ~$300k-$490k, REs up to ~$690k in outlier cases [VERIFIED https://jobsbyculture.com/blog/anthropic-compensation-2026; https://www.levels.fyi/companies/anthropic/salaries]. Mem0 RE $150k-$180k [VERIFIED snapshot]. Tier-1 labs dominate on comp and on signal value; the startups are faster to land and validate the resume. [OPINION]

---

## 2. The portfolio gap: what a hiring manager wants to SEE that his footprint does not show

His private artifact is excellent. His public footprint is near-empty (`15` Gap A/D: ~0 external traction, no blog, no launched post). A hiring manager for the roles above cannot see inside the repo's `eval/` dir during a resume screen. Four specific gaps, each tied to a deliverable already drafted in this kit so the path reuses existing work.

**Gap 2.1 — No public, reproducible eval writeup.**
What they want to see: a candidate who can publish a defensible benchmark result with a repro command, not just claim rigor. This is the #1 screen-able proxy for the Anthropic, Zep, Letta, and Mem0 roles, all of which name "evaluation" and "benchmarking ideas from papers" in their reqs [VERIFIED snapshots above].
Already drafted: `overnight/10-field-guide.md` ("What AI memory benchmarks actually measure, and what they hide") is a finished, sourced essay on LoCoMo/LongMemEval traps. `overnight/20-citable-number-protocol.md` is the exact procedure to GET one honestly-citable NDCG@10 number (N>=3, mean +/- stddev, env receipt). Ship `10` with a number produced by `20`. That single act closes the largest gap.

**Gap 2.2 — No Python signal.**
What they want to see: Mem0, Cognee, Letta, the Anthropic evals RE, and LanceDB all name Python explicitly [VERIFIED snapshots]. His entire public corpus is Rust. A hiring filter that greps "Python" returns nothing.
Already drafted: `15` move #3 calls for a thin Python eval-runner over the same LoCoMo/LongMemEval fixtures that reproduces his numbers. This is small (the fixtures and metrics already exist; per `20`, the harness emits NDCG@10/MRR/recall) and kills the one real cross-cutting gap. Pair it with the `10` writeup so the repro command in the post is the Python runner.

**Gap 2.3 — No external-facing writing with a thesis / no findable name.**
What they want to see: evidence he can communicate rigor to humans, the simonw/swyx signal; and a name that surfaces when they search "LoCoMo reproducibility" or "agent memory eval honesty."
Already drafted: `overnight/14-content-engine.md` is the full build-in-public loop (Friday TIL from commit bodies, monthly teardown), and `15` move #2 is the specific first essay ("How not to lie with LLM eval numbers," externalizing the AGENTS.md Eval Citation Discipline). The content already exists as internal policy and commit bodies; `14` is the machine to publish it on cadence.

**Gap 2.4 — No conference-adjacent / community visibility.**
What they want to see: that other practitioners take his work seriously, a Show HN with comments, a PR into a popular memory repo, a citation by Mem0/Letta's own benchmark posts.
Already drafted: `overnight/18-launch-playbook.md` / `09-launch-kit.md` cover the HN + r/LocalLLaMA fire. The LoCoMo answer-key audit angle (`10` cites the 6.4% error-rate audit) is a credible, non-self-promotional hook that the memory-vendor community already argues about. [OPINION] This is the lowest-priority of the four because for the lab roles a writeup + Python runner outweigh upvotes.

The through-line: he does not need to build anything new for the portfolio. `10`, `14`, `20` are drafted. The gap is entirely "ship the drafts publicly," which is the exact Gap A/C (distribution / finishing-for-strangers) that `15` already diagnosed.

---

## 3. The 30 / 60 / 90-day plan

Premise: convert one private repo into a credible external candidate. Constraint from `21-stop-doing.md` and `15`: no more `release.yml` passes, no SEO commits, no provenance-pillar polish. Every hour goes to public proof or applications.

**Days 0-30 — Produce the headline proof.**
- Run `overnight/20-citable-number-protocol.md`: N>=3 LoCoMo base NDCG@10 runs, mean +/- stddev, env receipt. (Base variant: no API key, no GPU, fewest caveats, per `20`.)
- Publish `overnight/10-field-guide.md` with that number inserted, on a personal blog/domain + cross-post. Include the repro command.
- Open the GitHub repo's eval harness story in the README (link the essay).
- Outcome: one defensible, public, reproducible result with his name on it. This is the artifact the Tier-1 reqs screen for.

**Days 31-60 — Close the Python gap + start the cadence.**
- Build the thin Python eval-runner (`15` move #3) over the same fixtures; make IT the repro command in the day-30 post (edit the post). Now the public artifact is bilingual: Rust system + Python eval reproduction.
- Publish essay #2: "How not to lie with LLM eval numbers" (`15` move #2, from the AGENTS.md discipline).
- Start the `14-content-engine.md` Friday-TIL loop (one short post/week from commit bodies). Cost ~25 min/week.
- Begin applications to Tier-1 + Tier-2 (Anthropic Model Evaluations, Zep, Letta, Mem0). Each application links the day-30 writeup as the work sample.
- Outcome: Python signal exists; two essays live; weekly cadence started; first applications out with a real artifact attached.

**Days 61-90 — Distribute and convert.**
- Fire the HN + r/LocalLLaMA launch (`18`/`09`) for the benchmark-honesty essay, not for the product. The LoCoMo audit hook is the wedge.
- One monthly teardown piece (`14`) stitching the TILs into an argument.
- Second application wave (Tier-3 Rust infra: Turso, Modal, LanceDB watch) as a hedge; reach out to Zep/Letta founders directly since both run small founding teams that respond to a strong public artifact.
- Outcome: a findable name, an audience seed, applications in flight with proof, and a fallback Rust-infra lane.

### The 3 highest-ROI proof artifacts (ranked)

1. **The reproducible benchmark writeup** (`10` + the number from `20`). Highest ROI: it is the single artifact every Tier-1/Tier-2 req screens for, it is already drafted, and his existing citation discipline makes it more honest than the category's marketing. One deliverable closes Gaps 2.1 and most of 2.3.
2. **The Python eval-runner over LoCoMo/LongMemEval** (`15` move #3). Closes the one real cross-cutting gap (2.2) named by Mem0/Cognee/Letta/Anthropic-evals/LanceDB, and proves the methodology transfers across languages. Small to build because fixtures + metrics already exist.
3. **The "How not to lie with LLM eval numbers" essay** (`15` move #2, via `14`). Externalizes the AGENTS.md Eval Citation Discipline, signals exactly the judgment the Anthropic eval interview probes (reproducibility, failure-mode iteration), and seeds the findable-name gap. Lowest build cost (the policy is already written); compounds via the `14` cadence.

---

## 4. The honest counter: where he is WEAK, and how much it actually matters

No sugarcoating. Four real weaknesses, each scored by how much it costs him per role type.

**4.1 — No published papers.**
The reality: zero peer-reviewed or arXiv output. Some research roles weight this heavily.
How much it matters, by role: Anthropic Model Evaluations / Universes are *engineering* roles ("Research Engineer," not "Research Scientist") that hire on infra + experimental rigor, not publication count [VERIFIED snapshot framing]. Zep states outright it is "not hiring ML researchers chasing publications" [VERIFIED snapshot]. Mem0 wants "benchmarking ideas from papers," i.e., reading not authoring [VERIFIED snapshot]. So for the RE/applied lanes this matters LITTLE. It bites only if he aimed at a Research *Scientist* title (he should not, per `15`). Net: low cost on the recommended path.

**4.2 — No big-company / production-at-scale experience.**
The reality: solo project, single-user on-device, no team, no prod traffic, no on-call.
How much it matters: this is his most expensive weakness for the *infra* roles. Modal (distributed GPU cloud), Baseten (inference at scale), Turso (distributed DB), LanceDB (distributed vector DB) all assume production-scale systems experience he cannot show [VERIFIED snapshots emphasize "real throughput and latency," "scalable... deployed with proper monitoring"]. For the eval-lab roles it matters MODERATELY: Anthropic wants infra "at scale" but the differentiator is the eval *science*, which he has. For memory startups (Mem0/Letta/Zep founding-ish) it matters LESS because they are small and value the demonstrated artifact over a big-company logo. Net: high cost for Tier-3, moderate for Tier-1, low for memory startups. This is the main reason to prioritize eval-labs + memory startups over pure infra.

**4.3 — Solo-only, no collaboration signal.**
The reality: 310 of 314 commits are his; no external PRs, no code review with peers, no team-fit evidence [VERIFIED `15` Section 1].
How much it matters: founding roles (Letta, Supermemory) explicitly want people who thrive on a "small, tight-knit team" [VERIFIED snapshot]; an interviewer will probe collaboration. For Anthropic and larger orgs it is a standard interview-loop concern, manageable. Mitigation is cheap and on the day-90 plan: a couple of merged PRs into a popular memory repo (Mem0/Letta are Apache-2.0) convert "solo" into "collaborates in public" fast. Net: moderate cost, cheapest to mitigate of the four.

**4.4 — Distribution-light / no audience.**
The reality: `15` Gap A. ~0 external traction, no launched post, no findable name.
How much it matters: for the *hiring* path specifically, LESS than `15` implies for the *founder* path. A hiring manager needs ONE strong work sample, not an audience. The day-30 writeup supplies it. Where it still costs him: warm inbound, referrals, and the founder-fallback. So it is a real weakness for trajectory (a) and (c) but a minor one for trajectory (b), which is precisely why `15` recommended (b). Net: low cost on the recommended path, and the Section-3 plan addresses it as a byproduct.

### The honest bottom line
On the *recommended* path (eval-lab + memory-startup RE/applied), his weaknesses are concentrated in areas those specific roles discount (papers, audience) or that are cheaply mitigated (collaboration via PRs). His one genuinely expensive weakness, no production-at-scale experience, is exactly why the Tier-3 pure-infra roles (Modal/Baseten/Turso/LanceDB) are listed as hedges, not primary targets. The plan does not ask him to fix all four; it routes around the expensive ones and spends effort only where ROI is highest. [OPINION grounded in the role-by-role evidence above]

---

## Sources

Postings (canonical URLs; requirement text from WebSearch snapshots on 2026-05-31, boards 403 to direct fetch):
- Anthropic Model Evaluations RE — https://job-boards.greenhouse.io/anthropic/jobs/5198255008 ; EOI https://job-boards.greenhouse.io/anthropic/jobs/4990535008
- Anthropic Universes RE — https://job-boards.greenhouse.io/anthropic/jobs/5061517008
- Zep Applied Research Engineer — https://www.ycombinator.com/companies/zep-ai/jobs/Pj0RIyD-applied-research-engineer ; careers https://www.getzep.com/careers/
- Letta Founding/Research Engineer — https://careers.essencevc.fund/companies/letta/jobs/45534002-founding-research-engineer
- Mem0 Senior Research Engineer — https://jobs.ashbyhq.com/mem0/63632678-661b-41d1-9cbd-ad91361bc956 ; careers https://mem0.ai/careers
- Supermemory Founding Eng/Research — https://www.linkedin.com/jobs/view/founding-engineering-and-research-at-supermemory-4316435240
- Cognee careers — https://www.cognee.ai/careers
- Turso careers — https://turso.tech/careers
- Modal jobs — https://jobs.ashbyhq.com/modal ; company https://modal.com/company
- Baseten Applied AI Inference Engineer — https://jobs.ashbyhq.com/baseten/90e9ff4e-1225-4b1b-b0b4-2362e36d9cfa/
- LanceDB jobs — https://jobs.ashbyhq.com/lancedb
- Chroma careers — https://careers.trychroma.com/

Context / comp / domain:
- Anthropic comp — https://jobsbyculture.com/blog/anthropic-compensation-2026 ; https://www.levels.fyi/companies/anthropic/salaries
- Letta Leaderboard / Evals — https://www.letta.com/blog/letta-leaderboard
- Zep KG architecture — https://arxiv.org/abs/2501.13956
- Supermemory engine — https://supermemory.ai/blog/memory-engine/
- Anthropic Managed Agents memory — https://www.infoq.com/news/2026/04/anthropic-managed-agents/
- Agent-memory benchmark landscape — https://mem0.ai/blog/state-of-ai-agent-memory-2026

Repo evidence cited inline via `15-skill-gap-trajectory.md`, `01-diagnosis.md`, and file/commit references verified against the working tree on 2026-05-31. Kit deliverables reused: `10-field-guide.md`, `14-content-engine.md`, `18-launch-playbook.md`, `20-citable-number-protocol.md`, `09-launch-kit.md`, `21-stop-doing.md`.
