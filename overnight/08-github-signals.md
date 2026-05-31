# Authoritative GitHub Signals (operator-pulled via GitHub MCP)

These are ground-truth from the GitHub API, not web scraping. Pulled 2026-05-31.

> CORRECTION (post-verification, added after a sub-agent re-read the current code): my "broken end-to-end /
> he shipped retrieval instead of fixing it" framing below is STALE and too strong. I verified the current
> source: 5 of #92's 7 bugs are FIXED. The accept/dismiss routes are registered (router.rs:109,113), the
> pending-revisions list route exists (router.rs:434), the MCP wrapper POSTs correctly with a dedicated
> `list_pending_revisions_impl` (tools.rs:916,1616), and list responses carry inline content. [VERIFIED grep
> 2026-05-31]. He DID do the backend work. What remains: the issue is still open and unclosed, the user-facing
> SKILL.md / MCP tool descriptions are still out of sync about the two pending buckets (bug 5), and the edit
> flow is still two non-atomic writes (bug 6). See 13-review-fix-plan.md for the verified per-bug status.
>
> The honest, narrower version of the point still stands: he did the satisfying backend engineering, then
> left the user-facing finishing (docs sync, atomic supersede, closing the loop on his own issue) undone for
> 2+ weeks while moving to retrieval tuning. That is a real pattern, just subtler than "core verb broken."
> Keeping the original text below, struck-through in spirit, so the correction is auditable. Evidence over a
> punchy story.

## Issues: near-zero external engagement

`list_issues` returns **totalCount: 4** open issues. Breakdown:
- #194 — external user `kiluazen` (kushal), a thoughtful per-claim-provenance design question. The ONLY
  substantive external technical engagement in the tracker. [VERIFIED github list_issues]
- #92 — self-authored. `/review` skill broken end-to-end.
- #79 — self-authored. handoff per-project isolation feature.
- #1  — self-authored. demo clips.

So: one engaged external person. That matches the "almost no traction" hypothesis. [VERIFIED]

The kiluazen thread is a lead. He references `yologdev/yopedia`, another repo in the
provenance/AI-memory neighborhood. There is a tiny community of people who care about exactly Origin's
differentiator (enforceable provenance). That is the thread to pull for early users. [VERIFIED issue #194 body]

## The single most damning artifact: issue #92

Self-authored, 2026-05-13. Title: "/review skill broken end-to-end: pending-revisions list/accept/dismiss
missing." His own words:

- "`/origin:review` (plugin v0.5.2) is non-functional today."
- "Backend routes + MCP wrapper + skill doc are all out of sync."
- "Today's only working flow is raw SQL on ... origin_memory.db. That is not a review UX."
- "Meta-memory captured 2026-04-24 already flagged the listing endpoint gap as P1; no PR has landed since."

He wrote a 7-bug, 6-step fix plan with exact file:line citations. Then, per the commit log, the next two
weeks (#202-#214) went to retrieval micro-tuning: page-channel RRF, graph-activation gates, query
decomposition. [VERIFIED git log + issue #92]

Read that sequence again. A CORE USER-FACING VERB of his product — the "review before trust" feature that
the README sells as differentiator #2 — has been broken for over two weeks. He diagnosed it in surgical
detail. Then he chose to build a fourth retrieval-ranking variant instead of fixing it.

This is the thesis in one data point: he reaches for the rigorous, inward, measurable work (retrieval evals)
over the messy, outward, user-facing work (make the review flow actually function). The review flow has no
benchmark; retrieval has LoCoMo. He optimizes what he can score.

"Review before trust" is sold in the README as a top-3 differentiator. It does not work. [VERIFIED README
differentiator #2 vs issue #92]

## Release cadence: fast version churn, lots of release plumbing

10 releases visible from v0.3.0 (May 5) to v0.7.0 (May 25). ~20 days, 5+ minor/patch versions. Many release
notes are dominated by `fix(ci): release.yml ...`, `fix(release): ...`, version-sync fixes. [VERIFIED
list_releases]. The release machine itself consumed a large share of effort. v0.5.0 alone is "merge
origin-mcp + origin-plugin into monorepo" plus 6 version-sync/CI commits.

The releases are real and the engineering is clean. But the changelog reads as an engineer talking to
himself: memory-type taxonomies, Arc<MemoryDB> clone-before-await fixes, supersede-relation soft-archives.
Almost nothing in the user-visible voice of "here is a new thing you can do today."

## Verdict from authoritative data

- External demand signal: ~1 person. [VERIFIED]
- Self-authored issues = he is his own primary user and bug reporter. Healthy for dogfooding, but it means
  the feedback loop is entirely internal. No outside reality is pushing on the product.
- The #92 / retrieval-tuning sequence is the cleanest evidence in the whole run that he avoids user-facing
  finishing work in favor of measurable inward work.

## VERIFICATION
- Source: GitHub MCP `list_issues` (totalCount 4) and `list_releases` (10 entries), pulled live 2026-05-31.
  PASS. Numbers are not estimated; they are the API response.
- One caveat: closed issues not enumerated here; there may be more historical external engagement in closed
  issues. The footprint agent (03) covers stars/forks/downloads for a fuller picture. The open-issue count
  and the #92 content stand regardless.
