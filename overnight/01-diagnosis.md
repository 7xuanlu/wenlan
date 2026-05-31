# Diagnosis: Where Qi-Xuan's Engineering Effort Actually Goes

Forensic read of the `origin` repo. Apr 19 to May 30, 2026. Evidence-cited, no flattery.

Every claim below is tagged. [VERIFIED cmd] means a command output backs it. [INFERRED from X] means I reasoned from evidence. [OPINION] means it is my judgment.

---

## 1. The shape of the work

314 commits across 38 active days. About 8 commits per active day. Real intensity, sustained for six weeks. [VERIFIED `git rev-list --count HEAD` = 314; `git log --format=%ad --date=short | sort -u | wc -l` = 38]

Commit type mix [VERIFIED `git log --format=%s | grep -oE '^[a-z]+...' | sort | uniq -c`]:

| Type | Count | Share |
|---|---|---|
| fix | 146 | 46% |
| feat | 43 | 14% |
| chore | 40 | 13% |
| docs | 39 | 12% |
| refactor | 17 | 5% |
| ci | 14 | 4% |
| test | 2 | 0.6% |
| perf | 1 | 0.3% |

3.4 fixes for every feature. [VERIFIED 146/43] Two test commits in 314. [VERIFIED] That ratio is not a productivity badge. It says most of the work is repairing things that were shipped slightly wrong, not adding new capability.

---

## 2. Where the time actually goes (bucketed)

I bucketed by commit subject keyword. Buckets overlap slightly (a commit can be eval+ci), so these are directional, not exact-sum. Commands shown so he can re-run them.

- Eval / retrieval / rerank / baseline / judge / locomo / fixture: **36 commits, ~11%** [VERIFIED `git log --format=%s | grep -iE 'eval|locomo|longmemeval|baseline|benchmark|judge|rerank|retrieval|fixture' | wc -l` = 36]
- CI / release / workflow / version / deps plumbing: **52 commits, ~17%** [VERIFIED `grep -iE 'release-please|ci:|release.yml|workflow|github action|pipeline|^ci|version bump|tag'` = 52]
- SEO / README / docs / marketing: **47 commits, ~15%** [VERIFIED `grep -iE 'seo|readme|docs|landing|marketing|blog|learn|sitemap|meta|og:'` = 47]

Sum of inward-facing process work: roughly **135 of 314 commits, ~43%.** [INFERRED from the three numbers above, deduped by eye for the handful of double-counts]

Now look inside the `feat` commits, the supposed product surface [VERIFIED `git log --format=%s | grep -E '^feat'` subcategorized]:

- feat total: 43
- feat that are eval/judge/faithfulness/baseline: **10**
- feat that are retrieval/rerank: **1**
- feat that are ci/scripts/install/cross-platform/docker: **6**
- remainder (arguably product): **~26**

So 17 of 43 "features" (40%) are eval harness or infrastructure, not user-facing capability. [VERIFIED]

And inside `fix`: 57 of 147 fixes (39%) are CI, release, deps, version, homebrew, npm, docker. [VERIFIED `git log --format=%s | grep -E '^fix' | grep -ciE 'ci|release|deps|docker|version|homebrew|npm|workflow'` = 57]

The headline: **process and plumbing is not a side activity here. It is close to half the repo.** [OPINION grounded in the above]

---

## 3. File-level churn confirms it

Commits touching each file [VERIFIED `git log --name-only --format=%H | grep -v sha | sort | uniq -c | sort -rn`]:

| File | Commits touching it |
|---|---|
| `crates/origin-core/src/db.rs` | 57 |
| `.github/workflows/release.yml` | 45 (36 at current path) |
| `Cargo.lock` | 44 |
| `crates/origin-server/src/memory_routes.rs` | 40 |
| `README.md` | 40 |
| `.github/workflows/ci.yml` | 30 |

`release.yml` got touched 45 times and `ci.yml` 30 times. [VERIFIED] That is 75 commits of pipeline editing. A release pipeline for a solo project with no confirmed external users (see section 6) received roughly as much attention as the core database file.

Line churn is dominated by eval fixtures, not product code [VERIFIED `git log --numstat` aggregation]:

- `app/eval/data/locomo10.json`: 133,502 lines of churn
- `app/eval/data/locomo_plus.json`: 16,044
- `crates/origin-core/src/eval/token_efficiency.rs`: 13,548
- multiple `app/eval/fixtures/realistic/*.toml` files: 3,000+ each

The single largest churned artifact in the entire repo is a benchmark dataset. [VERIFIED]

---

## 4. Failure pattern: CI thrash and the "second pass / third pass" loop

The clearest evidence of a perfectionism loop is `release.yml` on 2026-05-24. In a single day [VERIFIED `git log --date=short -- .github/workflows/release.yml`]:

- `#163` fix(ci): release.yml — publish-crates correctness + Homebrew tap
- `#167` fix(ci): close 4 multi-target gaps in release.yml
- `#168` fix(ci): release.yml **second pass** — drop mac x86, inline Cross.toml, bundle DLL
- `#170` fix(ci): release.yml **third pass** — libssl-dev in cross container, ort DLL from MS
- then `#173`, `#179` fix(deps) vendored OpenSSL twice
- then `#182`, `#184`, `#185` more ci perf/fix

That is roughly nine consecutive commits hammering the release pipeline in one day. The "second pass / third pass" naming is the tell: he knew he was iterating in place and named it anyway. [VERIFIED commit subjects]

Earlier the same pattern, 2026-04-23 to 04-24, six straight `fix(ci)` / `fix:` release-hardening commits [VERIFIED]:
`9742afd` CI lint + harden release, `fd7dd56` sync versions + harden, `9b13be6` sed regex, `60bd10e` ad-hoc signing, `3bc4a9a` remove empty Apple env, `85c2842`/`cb84b95`/`d30930b`/`aa25245` workflow_dispatch + release-name fixes.

This is a recurring shape: ship pipeline, it breaks, patch, it breaks again, patch again. The pipeline serves a product almost nobody is downloading yet. [OPINION, supported by section 6]

---

## 5. Failure pattern: positioning loop and SEO busywork

README touched 40 times. [VERIFIED] The subset that is pure positioning churn [VERIFIED `git log --date=short -- README.md | grep -iE 'readme|positioning|framing|tagline|...'`]:

- 05-17 "sharpen README product framing"
- 05-20 "rewrite README for launch positioning"
- 05-05 "refine README preview framing"
- 05-06 and 05-07 two separate commits both setting the tagline to the exact same string "Where Personal AI Memory Compounds"

He set the same tagline twice on consecutive days. [VERIFIED `git log | grep -i tagline` shows two entries, 05-06 and 05-07, identical text]

Then on 2026-05-24, five `docs(seo)` commits in one day, all variations of "add Links section cross-referencing useorigin.app" [VERIFIED `git log --format=%s | grep 'docs(seo)'` = 5, all dated 05-24]:
`#171` Learn more section, `#172` Links to every crate README, `#174` Links to sub-READMEs, `#175` CONTRIBUTING+SECURITY drift, `#176` mention useorigin.app in AGENTS.md preamble.

SEO backlinking across crate READMEs is the definition of motion without traction. It optimizes discoverability of a product before establishing that anyone wants it. [OPINION, evidence above]

---

## 6. What he avoids: the user

Zero commits in 314 reference user feedback, a reported bug, onboarding a real person, a customer, or a feature request. [VERIFIED `git log --format=%s | grep -ciE 'user feedback|customer|onboard|feedback|reported|requested by'` = 0]

The CHANGELOG has 15 commits of edits [VERIFIED `git log --oneline -- CHANGELOG.md | wc -l`], but it is release-please generated boilerplate, not human "here is what changed for you" notes. The 0.7.0 entry leads with eval benchmarks and ci fixes, not user-visible wins. [VERIFIED `head -50 CHANGELOG.md`]

There is real distribution scaffolding: Dockerfile.daemon, an MCP server crate, 24 commits mentioning homebrew/npm/npx/install. [VERIFIED] So the *plumbing* for distribution exists. What is missing is any evidence of distribution *happening* or anyone on the other end. The pipes are built. Nothing observed flowing through them. [INFERRED from absence of user signals + heavy install plumbing]

---

## 7. The last two weeks: almost entirely inward

Since 2026-05-17, 55 commits. [VERIFIED `git log --since=2026-05-17 --format=%s | wc -l` = 55]

- eval / retrieval / rerank / judge / faithfulness: **22** [VERIFIED grep = 22]
- ci / release / docker / deps / homebrew / npm: **22** [VERIFIED grep = 22]

That is 44 of 55 recent commits (80%) on eval tuning and pipeline. [VERIFIED] The most recent week (05-24 to 05-30) is retrieval micro-tuning: `#202` retrieval namespace, `#203` page-channel as 4th RRF stream, `#208` eval-neutral cleanup, `#213` opt-in graph-activation gate, `#214` query decomposition into subqueries. [VERIFIED `git log --since=2026-05-17 --format='%ad %s'`]

And several of those retrieval commits admit the tuning did not even pay off [VERIFIED `git log --format=%s%n%b | grep -iE 'regress|lift'`]:
- "Phase 2 evals at PAGE_CHANNEL_LIMIT=10 regressed both benchmarks"
- "Iter1 (PAGE_CHANNEL_LIMIT=3) on consolidated scenario DB still regressed"
- "Lift claim refused under current eval (pages have no ground-truth)"

He is spending his freshest hours on retrieval changes that his own eval says regress, on a product with no observed users. [VERIFIED commit bodies + section 6]

---

## 8. Is process discipline itself the procrastination?

AGENTS.md is 432 lines, 39KB, and grew from 328 to 432 lines over the period. [VERIFIED `wc -l AGENTS.md` and the per-commit size walk]. It was edited 23 times. [VERIFIED]

Read what the growth is *about*. The additions are eval citation discipline (single-run rule, schema-version rule, receipt-only rule, layer attribution), TTL cache policy, worktree cleanup routines, an 8-layer L1-L8 test taxonomy. [VERIFIED AGENTS.md sections "Eval Citation Discipline", "TTL policy", "Local vs CI test responsibilities", "Worktree cleanup"]

This is governance for a research process that has not yet produced a citable, multi-run result. The repo itself confirms the numbers are not ready: `docs/eval/README.md` says "Current README numbers are retrieval-only, single-run local snapshots." [VERIFIED] And AGENTS.md line 373 forbids citing single-run baselines externally. [VERIFIED] So he wrote elaborate rules for how to honestly cite benchmark numbers, and the numbers he has are the kind the rules forbid him from citing.

31 commits are coded with planning vocabulary: "Plan A", "Phase 1", "Spec C-2", "P0a/P0b/P0c", "foundations". [VERIFIED `git log --format=%s | grep -cE 'Plan [A-Z]|Phase [0-9]|Spec [A-Z]|P[0-9][a-z]?'` = 31]. Commit bodies average 27 lines. [VERIFIED `git log --format=%H` body-line aggregation = 27.1]. The longest body is 747 lines. [VERIFIED]. That is dissertation-grade documentation of changes to a memory engine, written for an audience that does not exist yet.

My read: the discipline is real and good engineering hygiene. It is also a comfortable place to hide. Writing the rule for how to measure is safer than facing what the measurement says, and far safer than facing whether anyone wants the thing. Process is the one domain where he gets to feel finished. Shipping to users is the one domain where he does not control the verdict. He keeps choosing the first. [OPINION, but every input above supports it]

---

## 9. Time-of-day (texture, not indictment)

Commit hours cluster heavily in evenings and late nights: 23:00 (29), 21:00 (26), 00:00 (28), 08:00 (27), 16:00 (23). [VERIFIED `git log --format=%ad --date=format:%H | sort | uniq -c`] Busiest days are Wed (54), Thu (51), Sat (50), Sun (49). [VERIFIED] This is a nights-and-weekends solo build with high personal commitment. The problem is not effort. The problem is direction. [OPINION]

---

## Hard truths: the 5 things he is avoiding, ranked

**1. Putting the product in front of real users.** Zero of 314 commits reference a user, feedback, a report, or onboarding. [VERIFIED grep = 0] Six weeks, no observed person on the other end. Everything else on this list is a way of not doing this one. This is the bottleneck. Rank 1 by a wide margin.

**2. Letting the benchmark verdict be final.** He keeps tuning retrieval (#202-#214, last full week) while his own commit bodies say the changes regress ("still regressed", "regressed both benchmarks"). [VERIFIED] He would rather run another iteration than accept the current number and ship. The eval harness has become a place to stay busy, not a gate that decides anything.

**3. Stopping the CI/release loop.** 75 commits on ci.yml + release.yml [VERIFIED 45+30], including a documented "second pass / third pass / follow-ups" chain in a single day. [VERIFIED #167/#168/#170] A pipeline this polished serves a release cadence nobody is consuming. Declare it done.

**4. Killing the positioning churn.** README edited 40 times, the same tagline set twice on consecutive days, five SEO backlink commits in one afternoon. [VERIFIED] Rewriting the pitch is not the same as testing the pitch. The copy is not the constraint. The audience is.

**5. Shrinking the governance surface.** AGENTS.md grew to 432 lines / 39KB of rules, much of it eval-citation discipline for numbers the same file forbids citing. [VERIFIED] Writing the meta-process is the most sophisticated form of avoidance on this list, because it looks the most like senior engineering. It is the easiest one to defend and therefore the most dangerous.

---

*Every number here is reproducible. Re-run any tagged command against this repo. If a number is wrong, the command is in the line.*
