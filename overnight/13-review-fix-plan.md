# /review fix plan + review-curation benchmark

Author target: Qi-Xuan Lu (solo dev). Two deliverables. READ ONLY pass on
product code; nothing under `crates/` or `plugin/` was modified.

Scope note up front: issue #92 was filed 2026-05-13 against plugin v0.5.2.
Substantial backend + MCP work has landed since (PR #91 plus follow-ups that
wired routes, the db method, and typed MCP wrappers). Most of #92's backend
claims are now stale. The remaining real gaps are in the **skill doc**, the
**semantics decision**, and the **atomic edit flow**. Every claim below cites a
file:line I actually read.

---

## DELIVERABLE 1 - /review fix plan

### Per-bug verification against current code

#### Bug 1 - MCP `list_pending_impl` GETs against a POST route; stale `/// GET` doc
**Status: FIXED (mechanics) / PARTIAL (semantics).**

- `list_pending_impl` now POSTs, not GETs: it builds a `ListMemoriesRequest`
  and calls `self.client.post("/api/memory/list", &req)`.
  [VERIFIED crates/origin-mcp/src/tools.rs:916] It sets `confirmed: Some(false)`.
  [VERIFIED crates/origin-mcp/src/tools.rs:913]
- The daemon route is `POST /api/memory/list`.
  [VERIFIED crates/origin-server/src/router.rs:90] The 405 mismatch is gone.
- The stale `/// GET` doc comment is now correct: `/// POST /api/memory/list`.
  [VERIFIED crates/origin-server/src/memory_routes.rs:1185]

So the wire mechanics #92 flagged are fixed. What remains is the **bucket
semantics**: `list_pending` targets `confirmed=false` (the ~1050-row legacy
unconfirmed bucket), NOT `pending_revision=1`. That is the bug-5 collision, not
bug 1. Bug 1 itself: **FIXED.**

#### Bug 2 - `list_filtered` has no `confirmed` parameter
**Status: FIXED.**

- `list_filtered` is now a thin shim delegating to `list_filtered_confirmed`
  with `confirmed = None`. [VERIFIED crates/origin-core/src/db.rs:9354-9363]
- `list_filtered_confirmed` takes `confirmed: Option<bool>` and applies it:
  `confirmed = 1` for `Some(true)`, and `(confirmed = 0 OR confirmed IS NULL)`
  plus archive/recap exclusions for `Some(false)`.
  [VERIFIED crates/origin-core/src/db.rs:9366-9408]
- The route handler passes `req.confirmed` straight through.
  [VERIFIED crates/origin-server/src/memory_routes.rs:1199-1205]

The `confirmed=false` filter is honored, not silently ignored. **FIXED.**

#### Bug 3 - no daemon list-pending-revisions route exists
**Status: FIXED.**

- `GET /api/memory/pending-revisions` is registered.
  [VERIFIED crates/origin-server/src/router.rs:434-435]
- Handler `handle_list_pending_revisions` reads a `limit` query (default 50,
  clamped 1..500) and returns `Vec<PendingRevisionItem>`.
  [VERIFIED crates/origin-server/src/memory_routes.rs:3004-3018]
- db method `list_pending_revisions` enumerates the bucket:
  `WHERE pending_revision = 1 AND supersedes IS NOT NULL AND source = 'memory'
  ORDER BY last_modified DESC LIMIT ?1`.
  [VERIFIED crates/origin-core/src/db.rs:12823-12838]

The per-id route `GET /api/memory/pending-revision/{source_id}` also still
exists alongside it. [VERIFIED crates/origin-server/src/router.rs:438-439]
**FIXED.**

#### Bug 4 - accept/dismiss handlers exist but are unrouted
**Status: FIXED.**

- `POST /api/memory/revision/{id}/accept` -> `handle_accept_revision`.
  [VERIFIED crates/origin-server/src/router.rs:109-110]
- `POST /api/memory/revision/{id}/dismiss` -> `handle_dismiss_revision`.
  [VERIFIED crates/origin-server/src/router.rs:113-114]
- Handlers call `origin_core::post_write::accept_pending_revision` /
  `dismiss_pending_revision` and return typed responses
  (`RevisionAcceptResponse` / `RevisionDismissResponse`).
  [VERIFIED crates/origin-server/src/memory_routes.rs:1631-1657]

Note the path shape differs from #92's suggestion
(`/pending-revision/{id}/accept`). Current code uses
`/memory/revision/{id}/accept`. The `{id}` here is the **target_source_id** (the
memory being revised), confirmed by the wrapper building
`format!("/api/memory/revision/{}/accept", req.target_source_id)`.
[VERIFIED crates/origin-mcp/src/tools.rs:1503] **FIXED.**

#### Bug 5 - two pending semantics collide; doc/tool don't say which /review targets
**Status: STILL TRUE (this is the live bug).**

Both buckets exist and both are reachable, but nothing reconciles them for the
user:

- `list_pending` (confirm/forget verbs) targets the `confirmed=false` bucket.
  [VERIFIED crates/origin-mcp/src/tools.rs:913]
- `list_pending_revisions` (accept/dismiss verbs) targets the
  `pending_revision=1` bucket. [VERIFIED crates/origin-core/src/db.rs:12832]
- SKILL.md `/review captures` uses `list_pending` + `confirm_memory` / `forget`;
  `/review revisions` uses `list_pending_revisions` + `accept_revision` /
  `dismiss_revision`. [VERIFIED plugin/skills/review/SKILL.md:36-43]

So the skill DOES now split the two, which is better than #92 described. But the
split is undocumented as to *why* the two buckets differ, and the MCP
`list_pending` tool description still says "all unconfirmed captures"
[VERIFIED crates/origin-mcp/src/tools.rs:2182] while `list_pending_revisions`
says "memories awaiting human accept/dismiss because a newer version was
proposed". [VERIFIED crates/origin-mcp/src/tools.rs:2310-2315] A reader of the
skill cannot tell that a Protected-tier supersede lands in the *revisions*
bucket while a fresh low-confidence capture lands in the *captures* bucket. The
semantic boundary is real and load-bearing; it just is not written down.
**STILL TRUE as a documentation/clarity gap.** The routing/code half of bug 5 is
resolved.

#### Bug 6 - edit flow is non-atomic (capture supersedes=old, then forget old)
**Status: STILL TRUE.**

- SKILL.md still prescribes the two-write edit:
  "edit (`capture` with `supersedes=<old_id>` then `forget(old_id)`)".
  [VERIFIED plugin/skills/review/SKILL.md:37-38]
- No atomic supersede endpoint exists. `grep` for `atomic` in router.rs returns
  nothing. [VERIFIED crates/origin-server/src/router.rs - no match]
- The store handler does resolve `supersedes` server-side (agent-declared takes
  priority, else topic-match auto-set).
  [VERIFIED crates/origin-server/src/memory_routes.rs:411-425] But that is a
  single store; the skill's *edit* path is still capture-then-forget, two HTTP
  round trips, both rows live in between. **STILL TRUE.**

#### Bug 7 - list response lacks content, forcing N+1 lookups
**Status: FIXED.**

- `IndexedFileInfo` now carries `content: String`, documented as "Populated by
  `list_filtered_confirmed` for unconfirmed-review surfaces."
  [VERIFIED crates/origin-types/src/memory.rs:201-205]
- `list_filtered_confirmed` selects `MAX(content) as content` and populates it.
  [VERIFIED crates/origin-core/src/db.rs:9431]
- For the revisions bucket, `PendingRevisionItem` carries `revision_content`
  directly, selected in the same query.
  [VERIFIED crates/origin-types/src/responses.rs:805]
  [VERIFIED crates/origin-core/src/db.rs:12830,12855]

One round trip renders a review for both buckets. **FIXED.**

### Verdict table

| # | #92 claim | Current status | Evidence |
|---|---|---|---|
| 1 | MCP GETs a POST route; 405; stale `/// GET` | FIXED | tools.rs:916; router.rs:90; memory_routes.rs:1185 |
| 2 | `list_filtered` ignores `confirmed` | FIXED | db.rs:9354-9408 |
| 3 | no list-pending-revisions route | FIXED | router.rs:434; memory_routes.rs:3004; db.rs:12823 |
| 4 | accept/dismiss handlers unrouted | FIXED | router.rs:109-114; memory_routes.rs:1631-1657 |
| 5 | two pending semantics collide, undocumented | STILL TRUE (doc/clarity) | tools.rs:2182,2310; SKILL.md:36-43 |
| 6 | edit flow non-atomic | STILL TRUE | SKILL.md:37-38; router.rs (no atomic route) |
| 7 | list response lacks content -> N+1 | FIXED | memory.rs:201; db.rs:9431,12855 |

**2 of 7 still real** (bugs 5 and 6); 5 fixed by post-#92 work.

### Ordered, surgical implementation plan

The end-to-end flow already works over HTTP and typed MCP wrappers. The
remaining work is small. Do it in this order.

**Step 1 - SKILL.md: document the two buckets (closes bug 5, doc half).**
File: `plugin/skills/review/SKILL.md`. Add a short "Two buckets" subsection
under "Scoped invocation" that states plainly:

- `/review captures` walks `confirmed=false`: fresh auto-classified memories
  awaiting first approval. Verbs: `confirm_memory` (keep) / `forget` (drop).
- `/review revisions` walks `pending_revision=1`: a *Protected-tier* memory got
  a newer proposed version (a supersede). Verbs: `accept_revision` (apply the
  new version) / `dismiss_revision` (keep the original, drop the proposal).
- One line on which to use: "If you just imported, use `captures`. If `/brief`
  flagged pending revisions, use `revisions`."

This is the highest-value, lowest-risk change. Pure doc. Edit the plugin
source, not the cache copy at `~/.claude/plugins/cache/...`
(#92 reference, [VERIFIED issue #92 body fix-step 6]).

**Step 2 - MCP tool descriptions: align the wording (closes bug 5, tool half).**
File: `crates/origin-mcp/src/tools.rs`. Tighten the `list_pending` description
at line 2182 to say "first-approval captures (`confirmed=false`)" and
cross-reference: "for proposed *revisions* to existing memories use
`list_pending_revisions`." The `list_pending_revisions` description at 2310 is
already accurate; leave it. Surgical, two string edits.

**Step 3 - Atomic supersede endpoint (closes bug 6).** This is the only real
code addition. Minimal, idiomatic to the codebase.

- **db method.** `crates/origin-core/src/db.rs`. Add
  `async fn supersede_memory(&self, old_source_id: &str, new_content: &str,
  agent: &str) -> Result<SupersedeResponse, OriginError>`. Implement as a single
  libsql transaction (`BEGIN` ... `COMMIT`, per the "Batch SQL" convention):
  insert the new row with `supersedes = old_source_id`, then mark the old row
  archived (`supersede_mode = 'archive'`) in the same transaction. Reuse the
  existing supersede column conventions already present in `list_filtered`
  (it filters `supersede_mode != 'archive'`, [VERIFIED db.rs:9406]), so the
  archive flag is the established mechanism. No new column needed.
- **wire type.** `crates/origin-types/src/responses.rs`. Add
  `SupersedeResponse { old_source_id: String, new_source_id: String,
  wrote: bool }`, mirroring `RevisionAcceptResponse` at responses.rs:813.
- **route + handler.** `crates/origin-server/src/router.rs`:
  `POST /api/memory/{id}/supersede` -> `handle_supersede_memory`.
  `crates/origin-server/src/memory_routes.rs`: handler snapshots
  `Arc<MemoryDB>` out of the guard (the established pattern at
  memory_routes.rs:1636-1639), reads `{ content }` from the JSON body, calls
  `origin_core::post_write::supersede_memory(&db, &id, &content, &agent)`, and
  returns `Json<SupersedeResponse>`. Keep the wrapper logic in `post_write`
  (a `supersede_memory` fn that logs an activity then calls the db method),
  matching how `accept_pending_revision` is structured at
  post_write.rs:699-723. Do NOT put logic in the server handler (crate-boundary
  rule).
- **MCP wrapper.** `crates/origin-mcp/src/tools.rs`. Add `edit_memory_impl`
  (or extend `capture`) that POSTs `/api/memory/{id}/supersede` and
  **typed-deserializes** `SupersedeResponse` from `origin-types` - never
  `serde_json::Value`, per the convention at AGENTS.md and mirrored by
  `accept_revision_impl` (tools.rs:1503-1514). Block it on HTTP transport like
  the other write wrappers (tools.rs:1496-1502).
- **SKILL.md.** Replace the two-write edit line (SKILL.md:37-38) with a single
  `edit` verb that calls the new tool.

**Step 4 (optional, defer) - gate the supersede behind a confirmation.** The
existing review flow is read-only until the user acts (SKILL.md:62). Keep that
property: the new supersede is a write, so the skill must require explicit user
confirmation before calling it, same as `forget`.

### Risks

- **Bucket re-labeling churn (steps 1-2).** Changing the `list_pending`
  description risks other skills (`/brief`, `/handoff`) that reference the same
  tool. Grep `plugin/skills/` for `list_pending` before editing wording so the
  cross-references stay coherent. Low risk, pure docs.
- **Supersede archival semantics (step 3).** `list_filtered` already excludes
  `supersede_mode = 'archive'` rows ([VERIFIED db.rs:9406]) and excludes rows
  that are the `supersedes` target of a non-pending revision
  ([VERIFIED db.rs:9413]). Verify the new archived old-row does not leak back
  into either `captures` or `revisions` listings. Add a regression test in
  `db.rs` tests mirroring `accept_pending_revision_happy_path_then_not_found_on_recall`
  (db.rs:30642).
- **Transaction + libsql Mutex.** The supersede transaction holds the
  `conn` mutex for two statements. That is fine (it is sub-millisecond and the
  Mutex is the established single-writer guard), but do NOT `.await` anything
  other than the libsql calls inside the lock scope.
- **Path-shape divergence from #92.** #92 proposed
  `/pending-revision/{id}/accept`; shipped code uses
  `/memory/revision/{id}/accept`. Anyone following the issue verbatim will hit
  404s. Worth a one-line comment on the issue closing it out and pointing to the
  real paths.
- **`{id}` ambiguity.** Accept/dismiss take the **target_source_id**, not the
  revision row id. The wrapper handles this (tools.rs:1503) but a hand-rolled
  curl against the route with the wrong id silently 404s
  (`NotFound` from post_write.rs:707). Document which id in the SKILL.md.

---

## DELIVERABLE 2 - review-curation benchmark

### Why

#92 sat for 2+ weeks because retrieval eval has a number and review curation
does not. He optimizes what he can score. This makes the review-curation loop
scorable, mirroring `eval::page_faithfulness` conventions
([VERIFIED crates/origin-core/src/eval/page_faithfulness.rs:1-65]) and the
fixture+floor pattern of `app/eval/page_fixtures/`
([VERIFIED app/eval/page_fixtures/seed_facts.toml]).

### What it measures

The benchmark scores the **decision quality of the curation surface**: given a
labeled set of captures and proposed revisions, does the review flow surface the
right items into the right bucket, and would a correct reviewer's accept/dismiss
verbs produce the labeled-correct end state. Concretely two scores:

1. **Bucket-routing accuracy.** For each fixture item with a known
   ground-truth bucket (`captures` = first-approval `confirmed=false`, or
   `revisions` = `pending_revision=1` supersede), does the daemon route it to
   that bucket? Scored as precision/recall per bucket + a confusion matrix.
2. **Contradiction-surfacing precision/recall.** For revision items that are a
   genuine contradiction of an existing memory (label `is_contradiction =
   true`), does the item land in the `pending_revision` bucket (surfaced) rather
   than silently co-existing? Recall = surfaced contradictions / labeled
   contradictions. Precision = labeled contradictions / items the flow surfaced
   as revisions. This grounds on the real surfacing path
   `check_page_contradiction` -> re-distill flag
   ([VERIFIED crates/origin-core/src/post_ingest.rs:391-456]) and the
   `pending_revision` flag set on topic-match
   ([VERIFIED crates/origin-server/src/memory_routes.rs:407-410]).

Plus two **operator-decision rates** computed from a labeled correct-action
column (no LLM judge; the fixture states the right verb):

3. **False-accept rate.** Fraction of items whose ground-truth verb is "reject"
   (forget / dismiss) but which the flow presents in a way that defaults to or
   recommends accept. In practice: items that should be dropped but are routed
   to the keep-leaning bucket.
4. **False-reject rate.** Fraction of items whose ground-truth verb is "keep"
   (confirm / accept) but which never surface in any review bucket (dropped on
   the floor, invisible to the user).

And one timing proxy:

5. **Time-to-review (round-trip count).** Number of HTTP round trips to render
   the full queue for N items. The fix makes this O(1) per bucket (one list
   call, content inline, [VERIFIED db.rs:9431,12855]); the benchmark asserts it
   stays O(1), i.e. no N+1 regression. This is a structural metric, not a
   wall-clock one, so it is deterministic in CI.

### Fixture format

TOML, mirroring `page_fixtures`. New dir `app/eval/review_fixtures/`. One file
per scenario class (e.g. `seed_supersedes.toml`, `seed_first_captures.toml`,
`seed_contradictions.toml`, plus a negative-control
`seed_should_drop.toml`). Schema:

```toml
description = "Labeled captures + proposed revisions for the review-curation loop."

[[case]]
id = "rev_pref_001"
# An existing, already-confirmed memory the new item relates to (may be empty
# for a pure first-capture case).
existing_memory = "User prefers tabs over spaces."
# The incoming capture/revision content.
incoming = "User now prefers spaces over tabs for Python."
# Ground-truth bucket: "captures" | "revisions" | "drop".
expected_bucket = "revisions"
# Is this a genuine contradiction of existing_memory?
is_contradiction = true
# The correct reviewer verb on this item: "accept" | "dismiss" | "confirm" | "forget".
expected_verb = "accept"
```

`drop` + `expected_verb = "forget"` is the negative control: items the loop
should NOT surface as keep-worthy (mirrors the `seed_hallucinations.toml`
high-floor negative control, [VERIFIED app/eval/page_fixtures/seed_hallucinations.toml
exists]).

### Harness shape

New module `crates/origin-core/src/eval/review_curation.rs`, registered in
`eval/mod.rs` alongside `page_faithfulness`
([VERIFIED crates/origin-core/src/eval/mod.rs:22]). Mirror the
`PageFaithfulnessReport` struct layout:

```rust
pub struct ReviewCaseResult {
    pub fixture_path: String,
    pub case_id: String,
    pub expected_bucket: String,
    pub actual_bucket: String,          // observed from the daemon flow
    pub bucket_correct: bool,
    pub is_contradiction: bool,
    pub surfaced: bool,                 // appeared in any review bucket
    pub expected_verb: String,
}

pub struct ReviewCurationReport {
    pub fixture_count: usize,
    pub case_count: usize,
    pub bucket_routing_accuracy: f64,
    pub contradiction_precision: f64,
    pub contradiction_recall: f64,
    pub false_accept_rate: f64,
    pub false_reject_rate: f64,
    pub max_round_trips_per_bucket: usize,   // time-to-review proxy
    pub per_case: Vec<ReviewCaseResult>,
    pub env: Option<ReportEnv>,
}
```

Run shape: seed each `incoming` through the real ingest path against a temp DB
(reuse the `MemoryDB::new(NoopEmitter)` test harness already used in db.rs
tests, [VERIFIED db.rs test modules around 22657, 30642]), then call
`list_filtered_confirmed(.., Some(false), ..)` and `list_pending_revisions(..)`
to observe which bucket each item landed in. Compare to `expected_bucket`.
Compute the five metrics. Per-case breakdown always emitted (the eval-citation
"per-case visibility" rule).

Test gating: a `#[ignore]`d smoke test named like
`review_curation_meets_floor`, run in **L6 main canary** (post-merge,
non-blocking), exactly like `page_faithfulness` and `kg_faithfulness`
([VERIFIED AGENTS.md eval table]). No GPU, no API key needed for the
string-label core, so it could also run in L4; keep it L6 first to match the
faithfulness benches and avoid PR-time flake until the fixture stabilizes.

### Pass/fail floor

Single floor, honest and low to start (scaffold, single-run; not for external
citation per the Single-run rule):

- `bucket_routing_accuracy >= 0.90`
- `contradiction_recall >= 0.80` (missing a contradiction is the expensive
  error: the user trusts a stale memory)
- `false_reject_rate <= 0.05` (silently dropping a keep-worthy capture is the
  trust-killer for "review before trust")
- `max_round_trips_per_bucket == 1` (asserts no N+1 regression, the bug-7 fix)

`contradiction_precision` and `false_accept_rate` are **reported but not
gated** at first: a noisy surface (over-surfacing) is annoying but safe; the
user dismisses it. Gate them once a baseline exists.

### Scope limits (mirrors page-faithfulness honesty)

- **Label-driven, not semantic.** The fixture states the ground-truth bucket
  and verb. The benchmark checks routing + surfacing against those labels; it
  does NOT judge whether a contradiction is "really" a contradiction. That
  judgment is baked into the human-curated fixture, same as
  `page_fixtures` bake in `expected_min_faithfulness`.
- **No LLM judge.** Matches the page/KG faithfulness benches' string-match
  stance. A real contradiction-detection-quality judge (does the LLM
  contradiction check at post_ingest.rs:391 agree with the label?) is a
  follow-up, gated behind an `ANTHROPIC_API_KEY` L7 lane, not this floor.
- **Time-to-review is structural, not wall-clock.** It counts round trips, not
  milliseconds. It catches N+1 regressions deterministically; it says nothing
  about real latency under load. A wall-clock variant belongs in
  `eval::latency` ([VERIFIED eval/latency.rs exists]) if needed.
- **Does not measure the human.** It scores whether the *surface* presents the
  right items with the right default verbs. It cannot measure whether the real
  user makes the right call. "Review before trust" working end-to-end means the
  surface is correct AND cheap to act on; this benchmark covers the first half.
- **Single-run scaffold.** Until run N>=3 with stddev, the numbers are internal
  scaffold only, not citable externally (eval-citation Single-run rule).

---

## Verification index (every claim, file:line read)

- list_pending POSTs with confirmed=false: tools.rs:913,916 [VERIFIED]
- POST /api/memory/list registered: router.rs:90 [VERIFIED]
- handler doc now "POST": memory_routes.rs:1185 [VERIFIED]
- list_filtered delegates: db.rs:9354-9363 [VERIFIED]
- list_filtered_confirmed honors confirmed: db.rs:9366-9408 [VERIFIED]
- handler passes req.confirmed: memory_routes.rs:1199-1205 [VERIFIED]
- pending-revisions route: router.rs:434-435 [VERIFIED]
- handle_list_pending_revisions: memory_routes.rs:3004-3018 [VERIFIED]
- list_pending_revisions SQL: db.rs:12823-12838 [VERIFIED]
- accept/dismiss routes: router.rs:109-114 [VERIFIED]
- accept/dismiss handlers + typed responses: memory_routes.rs:1631-1657 [VERIFIED]
- accept wrapper path uses target_source_id: tools.rs:1503 [VERIFIED]
- list_pending tool desc "all unconfirmed captures": tools.rs:2182 [VERIFIED]
- list_pending_revisions tool desc: tools.rs:2310-2315 [VERIFIED]
- SKILL captures/revisions split + verbs: SKILL.md:36-43 [VERIFIED]
- SKILL two-write edit flow: SKILL.md:37-38 [VERIFIED]
- no atomic route in router: router.rs (grep no match) [VERIFIED]
- store resolves supersedes server-side: memory_routes.rs:411-425 [VERIFIED]
- IndexedFileInfo.content documented: memory.rs:201-205 [VERIFIED]
- list_filtered_confirmed selects content: db.rs:9431 [VERIFIED]
- PendingRevisionItem.revision_content: responses.rs:805; db.rs:12855 [VERIFIED]
- accept_pending_revision post_write fn returns typed resp: post_write.rs:699-723 [VERIFIED]
- check_page_contradiction surfacing path: post_ingest.rs:391-456 [VERIFIED]
- pending_revision flag set on topic match: memory_routes.rs:407-410 [VERIFIED]
- page_faithfulness report/fixture shape: page_faithfulness.rs:1-65 [VERIFIED]
- eval mod registration: eval/mod.rs:22 [VERIFIED]
- page_fixtures fixture format: seed_facts.toml [VERIFIED]
- db test harness patterns: db.rs:22657,30642 [VERIFIED]
