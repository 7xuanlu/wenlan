# The Stop-Doing List (ranked, with evidence)

You are good at adding discipline. You are bad at removing work. This list is the subtraction. Each item is
something the evidence says is low-leverage for you right now, what it costs, and what to do with the time.
Your own rule (AGENTS.md, "Surgical Changes"): touch only what the task requires. Apply it to your calendar.

Ranked by reclaimed-leverage. Do the top 3 this week.

## 1. STOP tuning retrieval until something ships to humans
Evidence: last 7 days were 71% inward; #202-#214 are all retrieval variants (page-channel, graph gate, query
decomposition) [self-dashboard; git log]. You have four retrieval methods (base, reranked, expanded,
decomposed) and zero users to tell you any of it matters. Retrieval quality is not your bottleneck. Demand
is. [03, 12, 16]
Cost of continuing: every day here is a day the field guide is not published and no human has tried the tool.
Reclaim: the field guide (10) ships in the time one more retrieval variant would take.

## 2. STOP editing release.yml / CI by hand
Evidence: release.yml edited 45 times, ci.yml 30 times [git churn]. The "second pass / third pass" same-day
loops (#167/#168/#170) are pure thrash [01]. This is real toil with no user value.
Cost: hours per release, recurring, plus context-switching tax.
Reclaim: install the two automations that already exist in this kit (automations/release-check,
automations/pr-hygiene). They run CI's exact gates locally and check the version-sync invariant before you
push. Most of the thrash was catching things locally that you instead discovered in CI. [05]

## 3. STOP rewriting the README and SEO copy
Evidence: README touched 40 times; a five-commit SEO backlink afternoon; the tagline set twice on consecutive
days [01]. SEO copy converts nobody when ~0 traffic arrives (0 HN/Reddit/PH, 34 stars) [03]. You are
polishing a storefront on an empty street.
Cost: real hours, zero return at current traffic.
Reclaim: one Show HN or one published essay drives more qualified traffic in a day than six weeks of backlink
edits. Distribution first, copy-polish later, and only against real traffic data.

## 4. STOP writing more rules into AGENTS.md
Evidence: AGENTS.md is 39KB of discipline, including elaborate eval-citation rules for benchmark numbers your
own docs admit are single-run and uncitable [01 argues this; 20 fixes the underlying gap]. Writing rules
feels like progress and produces nothing a user sees.
Cost: the meta-work crowds out the work.
Reclaim: the rules are good enough to PRODUCTIZE as-is (agent-rigor scaffold, bet #2). Ship the discipline as
a public artifact instead of extending it privately. [06, agent-rigor/]

## 5. STOP shipping releases nobody is waiting for
Evidence: 18 releases, v0.1.0 -> v0.7.0, changelogs written in engineer-to-self voice (Arc<MemoryDB>
clone-before-await, memory-type taxonomies) [08]. Versioning cadence implies users pulling updates. There are
~none.
Cost: release machinery is a large share of your CI thrash, for an audience of one.
Reclaim: cut release frequency to "when a user needs a fix." Spend the saved cycles on getting the first user.

## 6. STOP being your own only bug reporter
Evidence: 3 of 4 open issues are self-authored; the dialogue is entirely internal [08]. A closed feedback
loop optimizes toward your taste, not the market's.
Cost: you can perfect a product nobody asked for and never know.
Reclaim: answer issue #194 (a real human, waiting days), then get 5 strangers to file the next 5 issues. [09]

## The meta-pattern to stop
All six are the same move: choosing measurable, controllable, solo, inward work over ambiguous, uncontrolled,
public, outward work. The inward work is not wrong. It is just where you hide. The cure is not less rigor. It
is pointing the rigor at a target with a human on the other end.

## What NOT to stop (so this is honest)
- The engineering quality. The daemon, the hybrid retrieval, the cross-platform work are genuinely strong.
- The eval rigor itself. It is your rarest asset. Aim it outward (the field guide, a research-engineer
  portfolio), do not abandon it.
- The git-versioned provenance idea. It is your one un-copied cell. Keep it; just stop assuming users will
  ask for it unprompted.

## VERIFICATION
- Every "stop" item cites a specific evidence file or a git-churn number I measured this session (45/40/30
  edit counts from `git log --name-only | sort | uniq -c`, run in Wave 1). PASS.
- This list is opinion built on verified facts; the facts are checkable, the ranking is my judgment and
  labeled as such. [OPINION on ordering, VERIFIED on each underlying number]
