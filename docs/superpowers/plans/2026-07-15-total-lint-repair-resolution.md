# Total Lint Repair Resolution Implementation Plan

> **For Codex:** Execute inline with `superpowers:executing-plans`; keep Cargo single-job and complete each RED/GREEN checkpoint before the next task.

**Goal:** Make `/lint repair` resolve every observed deterministic and semantic lint finding into `ready`, `review`, `system_action`, or `blocked`, while preserving the existing single-manifest approval/CAS/apply/verify boundary.

**Architecture:** Add a standalone versioned repair-plan wire contract and a core registry that consumes authenticated General/Deep reports. Registry adapters re-resolve durable targets from canonical storage, prepare existing immutable manifests only for exact mutations, enqueue ambiguous semantic or data decisions into the existing refinement queue, and emit typed operational actions for non-data findings. The server and MCP expose plan preparation; the unified lint skill renders the complete plan and never implies approval.

**Tech Stack:** Rust, serde, libSQL, Axum, rmcp, existing Wenlan lint/repair/refinement contracts.

**Safety floor:** No live-data apply in this plan. `prepare` may write repair-control-plane artifacts and durable review items only. All Cargo commands use `CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1`; run one command at a time after checking memory pressure.

---

### Task 1: Define the versioned total-resolution wire contract

**Files:**
- Create: `crates/wenlan-types/src/repair_plan.rs`
- Create: `crates/wenlan-types/src/repair_plan_tests.rs`
- Modify: `crates/wenlan-types/src/lib.rs`
- Modify: `crates/wenlan-types/src/responses.rs`

- [ ] Write RED serde/validation tests for `RepairPlanRequest`, `RepairPlan`, `RepairPlanEntry`, and `RepairResolution`.
- [ ] Prove one and only one disposition per entry: `ready`, `review`, `system_action`, or `blocked`.
- [ ] Reject blank check IDs, empty affected-record sets where a target is required, duplicate occurrence digests, invalid completeness flags, and mismatched plan digests.
- [ ] Add a typed `LintRepairReview` refinement action/payload carrying check ID, occurrence digest, evidence, choices, and suggested research queries.
- [ ] Export the new module without changing the existing `repair.rs` frozen-v1 manifest schema.
- [ ] Run: `CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo test -p wenlan-types repair_plan --lib`.

### Task 2: Build the totality-first core planner

**Files:**
- Create: `crates/wenlan-core/src/repair_plan.rs`
- Create: `crates/wenlan-core/src/repair_plan_tests.rs`
- Modify: `crates/wenlan-core/src/lib.rs`
- Modify: `crates/wenlan-core/src/db.rs`
- Modify: `crates/wenlan-core/src/lint/catalog.rs`

- [ ] Write RED tests showing every actionable General/Deep check result contributes a plan entry and unsupported checks remain visible as `blocked`.
- [ ] Write RED tests showing incomplete semantic Deep work does not suppress complete deterministic entries.
- [ ] Authenticate matching scope, report schema/catalog, snapshot receipts, and producer receipts before planning.
- [ ] Enumerate findings in canonical check/occurrence order and compute stable occurrence digests.
- [ ] Add a registry fallback of `unsupported_deterministic_writer` rather than filtering findings.
- [ ] Add a catalog coverage test proving all 55 General check IDs and all semantic actions have a resolution route.
- [ ] Add an idempotent `INSERT OR IGNORE` refinement-queue seam for stable lint-review IDs; never resurrect a dismissed/terminal item.
- [ ] Run the focused core tests single-job.

### Task 3: Resolve the seven current deterministic families

**Files:**
- Create: `crates/wenlan-core/src/repair_plan/deterministic.rs`
- Create: `crates/wenlan-core/src/repair_plan/deterministic_tests.rs`
- Modify: `crates/wenlan-core/src/lint/identity/query.rs`
- Modify: `crates/wenlan-core/src/lint/memories/assessment.rs`
- Modify: `crates/wenlan-core/src/lint/pages/link_checks/orphans.rs`
- Modify: `crates/wenlan-core/src/lint/pages/state_checks/version.rs`
- Modify: `crates/wenlan-core/src/lint/runtime/schema.rs`
- Modify: `crates/wenlan-core/src/lint/serving/routes.rs`

- [ ] Extract shared `pub(crate)` typed diagnostic-row enumerators so lint and repair do not duplicate predicates.
- [ ] RED/GREEN mapping tests for `identity.memory_state_integrity`: blank `source_agent` and self-edge are exact; ambiguous state conflicts become Review Items.
- [ ] RED/GREEN mapping tests for `identity.tag_integrity`: blank, unsupported, and dangling rows resolve to exact row deletion.
- [ ] RED/GREEN mapping tests for `memories.supersession_integrity`: self-edge is exact; dangling/missing predecessor becomes Review.
- [ ] RED/GREEN mapping tests for `pages.links.orphan_labels`: one active same-scope target is exact; zero/multiple becomes Review; explicit cross-scope targets remain untouched.
- [ ] RED/GREEN mapping tests for `pages.projection.version_alignment`: DB Page is canonical and the entry names only that projection closure.
- [ ] RED/GREEN mapping tests for `runtime.schema_contract` and `serving.route_scope_contracts`: emit typed system actions, never content manifests.
- [ ] Use LSP references plus ast-grep structural searches to prove shared predicates have no missed consumers or duplicate repair SQL.

### Task 4: Extend manifests and canonical writers only for exact mutations

**Files:**
- Modify: `crates/wenlan-types/src/repair.rs`
- Modify: `crates/wenlan-types/src/repair_tests.rs`
- Modify: `crates/wenlan-core/src/repair.rs`
- Create: `crates/wenlan-core/src/repair_writers.rs`
- Create: `crates/wenlan-core/src/repair_writers_tests.rs`
- Modify: `crates/wenlan-core/src/post_write.rs`
- Modify: `crates/wenlan-core/src/export/knowledge.rs`

- [ ] Write RED frozen-wire and fail-closed tests before adding target/writer/mutation variants for memory normalization, exact tag-row removal, supersession clear, orphan binding, and Page projection regeneration.
- [ ] Preserve the existing `ReclassifyMemory` wire and frozen-v1 fixture byte-for-byte.
- [ ] Each writer takes a complete expected state, performs CAS, records row/file rollback, and declares a target-only allowed-effect closure.
- [ ] Route Page projection through `KnowledgeProjectionWrite::write_page`; do not add direct Markdown repair writes.
- [ ] Test stale expected receipts, no-op mutations, scope mismatch, and out-of-target effects fail closed.
- [ ] Run focused types/core repair tests single-job.

### Task 5: Resolve semantic findings without guessing

**Files:**
- Create: `crates/wenlan-core/src/repair_plan/semantic.rs`
- Create: `crates/wenlan-core/src/repair_plan/semantic_tests.rs`
- Modify: `crates/wenlan-core/src/repair_plan.rs`

- [ ] Cover all twelve `LintSemanticAction` variants in one exhaustive structural match.
- [ ] Reuse the existing reclassification manifest path when the semantic mutation is exact and disagreement-free.
- [ ] Route contradiction, staleness, supersession, KG relation/link, Page claim/evidence, and retrieval judgments to a stable Review Item unless canonical before/after state is uniquely derivable.
- [ ] Include evidence, lawful choices, and bounded suggested research queries; research remains advisory and grants no write authority.
- [ ] Deduplicate identical target/mutation proposals and route conflicting proposals to Review.
- [ ] Test rerunning the same reports refreshes/open-reuses the same review item without duplicating or resurrecting terminal items.

### Task 6: Persist and expose repair plans

**Files:**
- Modify: `crates/wenlan-core/src/repair.rs`
- Modify: `crates/wenlan-server/src/repair_routes.rs`
- Modify: `crates/wenlan-server/src/repair_endpoint_test.rs`
- Modify: `crates/wenlan-server/src/route_registry.rs`
- Modify: `crates/wenlan-mcp/src/tools.rs`
- Modify: `crates/wenlan-mcp/src/tools_test.rs`

- [ ] Add a no-clobber, digest-verified JSONL plan artifact under the existing repair artifact root, writing one entry at a time then fsync/rename.
- [ ] Add typed `POST /api/repairs/plan`; handler delegates all logic to core.
- [ ] Add typed MCP `prepare_lint_repair_plan`; wrappers deserialize a concrete response, never `serde_json::Value`.
- [ ] Keep `/api/lint`, `/api/repairs/prepare`, `/apply`, and `/verify` behavior compatible.
- [ ] Add route and typed-envelope tests, including source-stale, incomplete-family, duplicate occurrence, and no-clobber errors.
- [ ] Add a non-mutation test proving plan preparation changes only repair artifacts and refinement items, not canonical memory/Page/KG/tag data.

### Task 7: Make `/lint repair` the simple user surface

**Files:**
- Modify: `plugin/skills/lint/SKILL.md`
- Modify: `plugin-codex/skills/lint/SKILL.md`
- Modify: `plugin/skills/lint/agents/openai.yaml`
- Modify: `plugin-codex/skills/lint/agents/openai.yaml`
- Modify: `plugin/skills/README.md`
- Modify: `plugin-contract.json`
- Modify: `scripts/validate-plugin-contract.py`
- Modify: `scripts/validate-codex-plugin-slice.py`
- Modify: `docs/product/lint-repair-v1-goal.md`

- [ ] Replace the classification-only proposal instructions with one call that renders all four dispositions and every family/count.
- [ ] Preserve only `/lint`, `/lint deep`, and `/lint repair`; do not restore a separate lint-repair skill.
- [ ] State plainly that showing/preparing is not approval and exact `manifest_id + manifest_digest` approval applies one manifest only.
- [ ] Render every small-plan target inline and link the complete JSONL artifact for larger plans; never call an incomplete plan complete.
- [ ] Update validators and product tracker from “one proposal” to total resolution.
- [ ] Run plugin distribution/contract tests.

### Task 8: Verification and independent review

**Files:**
- Modify only defects found by the gates below.

- [ ] Check memory pressure; run `cargo fmt --all -- --check` and focused crate tests single-job, then `cargo test --workspace --lib` only if the memory floor is safe.
- [ ] Run `cargo clippy --workspace --all-targets` single-job only if memory remains safe.
- [ ] Run LSP diagnostics on every changed Rust module and ast-grep checks for all repair writer/action variants.
- [ ] Run plugin validators and confirm `git diff --check`.
- [ ] Run `review2`: one independent contract/correctness reviewer and one adversarial safety/concurrency reviewer, blind before synthesis.
- [ ] Fix findings and rerun the affected gates.
- [ ] Do not run live apply. Before any later existing-data mutation, run `review3`, show the exact manifests/mutations, and obtain explicit approval.
