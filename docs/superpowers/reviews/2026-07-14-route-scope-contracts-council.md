# Sensitive Read Scope Contract Council

**Date:** 2026-07-14
**Artifact reviewed:**
`docs/superpowers/specs/2026-07-14-route-scope-contracts-design.md`
**Baseline:** `origin/main` at `8cfae406`

## Reviewers

- Claude Opus, xhigh adversarial review: `needs-attention`
- Codex GPT-5.5, xhigh data-isolation review: `REQUEST_CHANGES`
- Codex GPT-5.4, xhigh feasibility review: `REQUEST_CHANGES`
- Codex GPT-5.6 Sol, xhigh verification review: `REQUEST_CHANGES`

The design was not approved as originally written. All blocking findings were
reconciled into the design before implementation planning.

## Accepted Corrections

1. Correct the scoped inventory from the 36 rows already labeled scoped to 40
   actual scoped routes. `home-stats`, `retrievals/recent`, `activities`, and
   `tags` expose Memory-derived rows and move out of Global.
2. Freeze the exact 55-route catalog, 40 scoped keys, and 15 justified Global
   keys. Catalog count is a progress ledger, not proof.
3. Add an executed HTTP behavior-case registry whose key set equals the 40
   scoped catalog keys. Router construction and mutable metadata are
   insufficient evidence.
4. Require positive and negative canaries for every enabled retrieval channel,
   filtering before ranking/limits, and scope propagation into internal Page,
   Entity, and graph candidate helpers during Wave 1.
5. Distinguish single-ID, batch, and parent-collection selection gates. Preserve
   input order for batch reads and use identical `404` responses for missing
   and mismatched direct IDs.
6. Resolve the exact `uncategorized` selector fail-closed when a registered
   Space has the same name.
7. Keep Page category (`pages.space`) independent from product scope
   (`pages.workspace`), including fixtures where the values differ.
8. Gate Entity relations through both endpoints and derive Entity-suggestion
   visibility conservatively through all referenced Memories.
9. Define derived ownership for activity events, tags, briefing, home stats, and
   snapshot captures instead of treating them as unscoped projections.
10. Run a read-only orphan-binding preflight and update defect-preserving tests
    plus `scripts/lint-e2e.sh` as part of acceptance.

## Partially Rejected Recommendations

### Chunk rows have no scope source

Rejected as stated. File and Memory chunk rows share the `memories` table and
therefore share `memories.space`, including legacy `source='file'` rows. The
reconciled contract scopes candidates first, then preserves memory-before-file
precedence among in-scope rows. It adds file-only and source-ID collision tests
without inventing a new Chunk scope or changing the desktop API.

### Every optional retrieval failure must fail the request

Rejected as a blanket behavior change. Optional retrieval channels deliberately
degrade today. Scope resolution, gating queries, and required content loads must
fail non-success; optional-channel degradation can remain. Positive canaries
for every enabled channel prevent a silently missing stream from producing a
false scope pass.

### Constant-time direct-ID behavior

Narrowed to an observable non-disclosure contract. Missing and mismatched IDs
must use the same query shape, status, response bytes, and no mismatch-specific
log above DEBUG. No unsupported timing guarantee is claimed.

## Final Council Decision

Proceed to executable planning only against the reconciled design. Before
implementation, run one bounded plan-level conflict check with Claude and one
independent Codex reviewer. Per-task five-way review is not required; RED-first
tests, serial verification, and a final dual-model review remain mandatory.

## Post-Council Baseline Refresh

The worktree was rebased from the reviewed `8cfae406` baseline onto
`origin/main` at `09725cdf` before executable planning. The intervening mainline
changes affect plugin presentation and CI, not the sensitive-read catalog,
daemon handlers, core query ownership, or scope contract. The executable plan
therefore uses design commit `02104092` and current main `09725cdf` without
reopening the approved product contract.
