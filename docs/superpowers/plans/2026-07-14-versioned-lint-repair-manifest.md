# Versioned Lint Repair Manifest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship one end-to-end, approval-gated `reclassify_memory` repair path backed by a versioned single-target manifest, durable rollback artifact, CAS canonical writer, and post-repair lint verification.

**Architecture:** Add strict repair wire types in `wenlan-types`; implement evidence resolution, immutable artifacts, CAS mutation, and verification in `wenlan-core`; expose thin `/api/repairs/*` routes and typed MCP tools; then add matching Claude/Codex `lint-repair` skills. The existing lint runner remains the only diagnostic runner and `/api/lint` stays read-only.

**Tech Stack:** Rust 2021, serde/serde_json, sha2, uuid, libSQL transactions, Axum 0.8, rmcp, Bash/Python plugin validators.

## Global Constraints

- `REPAIR_MANIFEST_SCHEMA_VERSION` is exactly `1`.
- One manifest contains exactly one logical target and one typed mutation.
- V1 enables only `reclassify_memory`; every other action fails closed.
- `/lint`, `GET /api/lint`, and `POST /api/lint` remain read-only.
- Apply requires the exact `manifest_id + manifest_digest` approval tuple.
- No provider-slot CLI, `--fix`, arbitrary SQL/JSON Patch, batch repair, deletion, merge, Page update, supersession, or automatic live-data mutation.
- Store operational artifacts under `<wenlan-data-dir>/repairs/<manifest-id>/`, never inside a worktree.
- Run Cargo with `CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1`; do not run release builds.
- Stop heavy work below 20% free memory or when swap is nearly exhausted.
- Complete `review2` before code completion; complete `review3` before any live stored-data apply.

---

### Task 1: Strict repair wire contract

**Files:**
- Create: `crates/wenlan-types/src/repair.rs`
- Create: `crates/wenlan-types/src/repair_tests.rs`
- Modify: `crates/wenlan-types/src/lib.rs`

**Interfaces:**
- Produces: `RepairDigest`, `RepairSource`, `RepairTarget`, `RepairExpectedState`, `RepairWriter`, `RepairMutation`, `RepairAllowedEffects`, `RepairRollbackArtifact`, `RepairPostAssertions`, `RepairManifest`, `PrepareRepairRequest`, `ApplyRepairRequest`, `RepairApplyReceipt`, `VerifyRepairRequest`, and `RepairVerificationReceipt`.
- Consumes: existing `LintProfile`, `LintScope`, `LintSnapshotReceipts`, `LintProducerReceipt`, `LintSemanticFinding`, `LintAgentSubmission`, and `MemoryType`.

- [ ] **Step 1: Write RED contract tests**

Add tests covering the public constructor and deserializer:

```rust
#[test]
fn manifest_requires_one_supported_writer_and_matching_mutation() {
    let manifest = fixture_manifest();
    assert!(RepairManifest::try_new(manifest).is_ok());

    let mut value = serde_json::to_value(fixture_manifest()).unwrap();
    value["writer"] = serde_json::json!("refresh_page");
    assert!(serde_json::from_value::<RepairManifest>(value).is_err());
}

#[test]
fn manifest_rejects_noop_reclassification_and_unknown_fields() {
    let mut value = serde_json::to_value(fixture_manifest()).unwrap();
    value["mutation"]["after_memory_type"] = value["mutation"]["before_memory_type"].clone();
    assert!(serde_json::from_value::<RepairManifest>(value).is_err());

    let mut value = serde_json::to_value(fixture_manifest()).unwrap();
    value["hidden_target"] = serde_json::json!("mem_other");
    assert!(serde_json::from_value::<RepairManifest>(value).is_err());
}

#[test]
fn apply_request_binds_exact_manifest_digest() {
    let request = ApplyRepairRequest::try_new(
        "repair_550e8400-e29b-41d4-a716-446655440000".into(),
        RepairDigest::parse(SHA256_A).unwrap(),
    )
    .unwrap();
    assert_eq!(request.approved_manifest_digest().as_str(), SHA256_A);
}
```

- [ ] **Step 2: Verify RED**

Run:

```bash
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo test -p wenlan-types repair -- --nocapture
```

Expected: compilation fails because `wenlan_types::repair` and the named types do not exist.

- [ ] **Step 3: Implement the minimal strict types**

Use tagged enums and private fields with getters. The core shape is:

```rust
pub const REPAIR_MANIFEST_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairDigest(String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RepairTarget {
    Memory { source_id: String, space: Option<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairWriter {
    ReclassifyMemory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RepairMutation {
    ReclassifyMemory {
        before_memory_type: Option<String>,
        after_memory_type: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RepairManifest {
    manifest_schema_version: u16,
    manifest_id: String,
    prepared_at: i64,
    source: RepairSource,
    target: RepairTarget,
    expected_state: RepairExpectedState,
    writer: RepairWriter,
    mutation: RepairMutation,
    allowed_effects: RepairAllowedEffects,
    rollback: RepairRollbackArtifact,
    post_assertions: RepairPostAssertions,
    manifest_digest: RepairDigest,
}
```

Implement custom `Deserialize` through a `RepairManifestWire`, call
`RepairManifest::try_new`, reject schema values other than `1`, reject a writer
that does not match its mutation, parse `after_memory_type` through
`MemoryType`, reject no-op before/after values, reject empty IDs/scopes/paths,
and require a 64-character lowercase-hex digest.

- [ ] **Step 4: Verify GREEN and crate boundary**

Run:

```bash
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo test -p wenlan-types repair -- --nocapture
cargo tree -p wenlan-types --depth 1
```

Expected: repair tests pass; `wenlan-types` gains no dependency beyond its existing lightweight set.

- [ ] **Step 5: Commit**

```bash
git add crates/wenlan-types/src/lib.rs crates/wenlan-types/src/repair.rs crates/wenlan-types/src/repair_tests.rs
git commit -m "fix: add versioned repair manifest contract"
```

### Task 2: Durable semantic target resolver and immutable prepare artifacts

**Files:**
- Create: `crates/wenlan-core/src/repair.rs`
- Modify: `crates/wenlan-core/src/lib.rs`
- Modify: `crates/wenlan-core/src/lint/semantic_candidates.rs`
- Test: `crates/wenlan-core/src/repair.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `PrepareRepairRequest`, `LintSemanticFinding`, `MemoryDB`, `LintReadSnapshot`, and the existing semantic record-key convention.
- Produces: `semantic_record_digest(kind, durable_id)`, `RepairArtifactStore`, and `prepare_memory_reclassification(db, store, request, now)`.

- [ ] **Step 1: Write RED resolver tests**

Use a temporary DB with two memories and a temporary repair root:

```rust
#[tokio::test]
async fn prepare_resolves_one_hashed_memory_without_mutating_store() {
    let (db, repair_root) = repair_fixture().await;
    let before = canonical_lint_state_digest(&db).await.unwrap();
    let request = classification_request("mem_target", "decision");

    let manifest = prepare_memory_reclassification(
        &db,
        &RepairArtifactStore::new(repair_root.path().to_path_buf()),
        request,
        1_721_000_000,
    )
    .await
    .unwrap();

    assert_eq!(manifest.target().memory_source_id(), Some("mem_target"));
    assert_eq!(before, canonical_lint_state_digest(&db).await.unwrap());
    assert!(repair_root.path().join(manifest.manifest_id()).join("manifest.json").is_file());
    assert!(repair_root.path().join(manifest.manifest_id()).join("rollback-v1.json").is_file());
}

#[tokio::test]
async fn prepare_rejects_ordinal_only_cross_scope_and_ambiguous_evidence() {
    let (db, repair_root) = repair_fixture().await;
    for request in invalid_prepare_requests() {
        let result = prepare_memory_reclassification(
            &db,
            &RepairArtifactStore::new(repair_root.path().to_path_buf()),
            request,
            1_721_000_000,
        )
        .await;
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: Verify RED**

Run:

```bash
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo test -p wenlan-core --lib repair::tests -- --nocapture
```

Expected: compilation fails because the repair module and helpers do not exist.

- [ ] **Step 3: Share the semantic record digest seam**

Move the existing key hashing into one crate-visible helper and consume it from
the candidate builder:

```rust
pub(crate) fn semantic_record_digest(kind: &str, durable_id: &str) -> LintDigest {
    let key = format!("{kind}:{durable_id}");
    let digest: [u8; 32] = Sha256::digest(key.as_bytes()).into();
    LintDigest::from_u64(u64::from_le_bytes(
        digest[..8].try_into().expect("digest prefix"),
    ))
}
```

For the relation endpoint record key, continue using the existing exact key;
v1 preparation accepts only `kind == "memory"`.

- [ ] **Step 4: Implement prepare and artifact durability**

Implement these exact public signatures:

```rust
pub struct RepairArtifactStore {
    root: PathBuf,
}

impl RepairArtifactStore {
    pub fn new(root: PathBuf) -> Self;
    pub fn manifest_dir(&self, manifest_id: &str) -> Result<PathBuf, WenlanError>;
    pub fn load_manifest(&self, manifest_id: &str) -> Result<RepairManifest, WenlanError>;
}

pub async fn prepare_memory_reclassification(
    db: &MemoryDB,
    store: &RepairArtifactStore,
    request: PrepareRepairRequest,
    now_epoch: i64,
) -> Result<RepairManifest, WenlanError>;
```

Open one read-only lint snapshot, enumerate scoped memory heads ordered by
`source_id`, compare `semantic_record_digest("memory", source_id)` with the
finding evidence IDs, require one match, and reject a target whose current
memory type already equals `after_memory_type`. Compute a canonical target
receipt and full ordered rollback before-image from that same snapshot.

Serialize rollback and manifest to sibling `.tmp` files, call `sync_all`, set
owner-only permissions under `cfg(unix)`, and rename atomically. Refuse an
existing manifest directory. The manifest digest must be computed from the
unsigned manifest struct before either file is written.

- [ ] **Step 5: Verify GREEN plus non-mutation**

Run:

```bash
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo test -p wenlan-core --lib repair::tests -- --nocapture
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo test -p wenlan-core --lib lint::semantic_test -- --nocapture
```

Expected: resolver/artifact tests and existing semantic digest-binding tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/wenlan-core/src/lib.rs crates/wenlan-core/src/repair.rs crates/wenlan-core/src/lint/semantic_candidates.rs
git commit -m "fix: prepare durable lint repair targets"
```

### Task 3: CAS canonical writer and effect-escape rollback

**Files:**
- Modify: `crates/wenlan-core/src/post_write.rs`
- Modify: `crates/wenlan-core/src/repair.rs`
- Modify: `crates/wenlan-server/src/memory_routes.rs`
- Test: `crates/wenlan-core/src/repair.rs`

**Interfaces:**
- Consumes: immutable `RepairManifest`, its stored rollback artifact, and approved digest.
- Produces: `post_write::reclassify_memory_cas`, `apply_repair`, and immutable `RepairApplyReceipt`.

- [ ] **Step 1: Write RED CAS and rollback tests**

```rust
#[tokio::test]
async fn stale_or_wrong_approval_performs_zero_writes() {
    let fixture = prepared_fixture().await;
    fixture.change_target_after_prepare().await;
    let before = fixture.all_rows().await;

    let result = apply_repair(
        &fixture.db,
        &fixture.store,
        fixture.manifest.manifest_id(),
        fixture.manifest.manifest_digest(),
    )
    .await;

    assert!(matches!(result, Err(WenlanError::Conflict(message)) if message == "repair_target_stale"));
    assert_eq!(before, fixture.all_rows().await);
    assert!(!fixture.apply_receipt_path().exists());
}

#[tokio::test]
async fn effect_escape_rolls_back_every_target_chunk() {
    let fixture = prepared_multichunk_fixture().await;
    fixture.inject_effect_escape_trigger().await;
    let before = fixture.all_rows().await;

    let result = fixture.apply_exact_approval().await;

    assert!(matches!(result, Err(WenlanError::VectorDb(message)) if message.contains("repair_effect_escape")));
    assert_eq!(before, fixture.all_rows().await);
}

#[tokio::test]
async fn successful_apply_changes_only_declared_owner_closure() {
    let fixture = prepared_multichunk_fixture().await;
    let before_non_target = fixture.non_target_digest().await;

    let receipt = fixture.apply_exact_approval().await.unwrap();

    assert_eq!(fixture.target_memory_types().await, vec!["decision", "decision"]);
    assert_eq!(before_non_target, fixture.non_target_digest().await);
    assert_eq!(receipt.actual_effects().source_ids(), &["mem_target"]);
}
```

- [ ] **Step 2: Verify RED**

Run the core repair test command and confirm the named behaviors fail because no CAS writer exists.

- [ ] **Step 3: Implement one canonical transactional writer**

Add:

```rust
pub async fn reclassify_memory_cas(
    db: &MemoryDB,
    source_id: &str,
    expected_receipt: &RepairDigest,
    expected_space: Option<&str>,
    after_memory_type: MemoryType,
) -> Result<RepairWriteProof, WenlanError>;
```

Lock `db.conn` once, begin an immediate transaction, recompute the ordered
target receipt and target Space inside the transaction, return
`Conflict("repair_target_stale")` before UPDATE on mismatch, capture
connection-local `total_changes`, update all target chunks, assert affected row
count equals the rollback artifact row count, and normalize the post-update
counter by that exact row count. Commit only when the target receipt changed
and the before/normalized-after effect-guard digests match. This catches trigger
escape in constant space without scanning unrelated rows, embedding blobs, or
FTS shadow tables. Roll back on every error.

Route the existing non-CAS `/api/memory/reclassify/{source_id}` handler through
`post_write::update_memory` so all ordinary reclassification also uses the
existing canonical write boundary; preserve its request/response behavior.

- [ ] **Step 4: Implement apply artifact sequencing**

`apply_repair` loads server-owned manifest bytes, recomputes their SHA-256,
compares the exact approval digest, rejects an existing apply receipt, verifies
the rollback artifact digest, holds a per-manifest advisory lock, calls only
`reclassify_memory_cas`, fsyncs a pending receipt before commit, then publishes
`apply-receipt.json` atomically without replacement. Recovery distinguishes a
live locked writer from crash residue and promotes a committed target even when
unrelated daemon state changed later. Both the pending file and its containing
directory are fsynced before the database commit is allowed. It never accepts
caller-supplied target or mutation fields.

- [ ] **Step 5: Verify GREEN**

Run:

```bash
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo test -p wenlan-core --lib repair::tests -- --nocapture
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo test -p wenlan-server --lib update_memory_endpoint_tests -- --nocapture
```

Expected: CAS, rollback, owner-closure, and existing reclassification behavior tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/wenlan-core/src/post_write.rs crates/wenlan-core/src/repair.rs crates/wenlan-server/src/memory_routes.rs
git commit -m "fix: apply lint repairs through CAS writer"
```

### Task 4: Thin repair HTTP routes and verification receipt

**Files:**
- Create: `crates/wenlan-server/src/repair_routes.rs`
- Modify: `crates/wenlan-server/src/lib.rs`
- Modify: `crates/wenlan-server/src/router.rs`
- Modify: `crates/wenlan-server/src/route_registry.rs`
- Test: `crates/wenlan-server/src/repair_routes.rs`

**Interfaces:**
- Produces: `POST /api/repairs/prepare`, `POST /api/repairs/apply`, and `POST /api/repairs/verify`.
- Consumes: core repair functions and typed `wenlan-types` requests/responses.

- [ ] **Step 1: Write RED endpoint tests**

```rust
#[tokio::test]
async fn lint_routes_remain_read_only_while_prepare_only_writes_artifacts() {
    let fixture = endpoint_fixture().await;
    let canonical_before = fixture.canonical_digest().await;

    let response = fixture.post_prepare().await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(canonical_before, fixture.canonical_digest().await);
    assert_eq!(fixture.get_lint_route_methods(), vec![Method::GET, Method::POST]);
}

#[tokio::test]
async fn apply_maps_stale_target_to_conflict() {
    let fixture = endpoint_fixture().await;
    fixture.prepare_then_stale().await;
    let response = fixture.post_apply_exact_approval().await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(response.json()["error"], "repair_target_stale");
}
```

- [ ] **Step 2: Verify RED**

Run:

```bash
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo test -p wenlan-server --lib repair_routes -- --nocapture
```

Expected: compilation fails because `repair_routes` and endpoints do not exist.

- [ ] **Step 3: Implement route framing**

Use one helper to clone `Arc<MemoryDB>` out of `ServerState`, derive
`<data-root>/repairs`, and call core functions. Do not put receipt hashing,
target resolution, SQL, or lint evaluation in handlers.

```rust
pub(crate) fn register(router: TrackedRouter<SharedState>) -> TrackedRouter<SharedState> {
    router
        .route("/api/repairs/prepare", post(handle_prepare))
        .route("/api/repairs/apply", post(handle_apply))
        .route("/api/repairs/verify", post(handle_verify))
}
```

Map core `Validation` to 422, `Conflict` to 409, and artifact/transaction
failures to 500 through the existing `ServerError` conversion.

- [ ] **Step 4: Implement verification receipt checks**

Add `record_repair_verification(db, store, request, page_root)` in core. It loads the
immutable apply receipt, rejects stale final snapshot receipts, requires
General and Deep completeness, checks the target evidence digest is absent
from `memories.semantic.classification`, verifies declared check-delta
constraints, compares the submitted DB and Page receipts with current state,
and writes `verification-receipt.json` under the per-manifest lock with
no-clobber publication. The apply receipt proves non-target stability inside
the CAS transaction; verification does not freeze unrelated daemon state after
that transaction. Page scans reuse the bounded General/Deep lint budgets and
finish before acquiring `db.conn`. It does not run lint and cannot mutate
canonical data.

- [ ] **Step 5: Verify GREEN and route catalog parity**

Run:

```bash
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo test -p wenlan-server --lib repair_routes -- --nocapture
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo test -p wenlan-server --lib route_registry -- --nocapture
```

Expected: endpoint tests pass and the executed route registry exactly matches the canonical catalog.

- [ ] **Step 6: Commit**

```bash
git add crates/wenlan-server/src/lib.rs crates/wenlan-server/src/router.rs crates/wenlan-server/src/route_registry.rs crates/wenlan-server/src/repair_routes.rs crates/wenlan-core/src/repair.rs
git commit -m "fix: expose approval-gated repair routes"
```

### Task 5: Typed local MCP repair tools

**Files:**
- Modify: `crates/wenlan-mcp/src/tools.rs`
- Modify: `crates/wenlan-mcp/src/types.rs`
- Test: `crates/wenlan-mcp/src/tools.rs`

**Interfaces:**
- Produces: `prepare_repair`, `apply_repair`, and `record_repair_verification` MCP tools.
- Consumes: typed requests/responses from `wenlan-types`; local stdio transport only.

- [ ] **Step 1: Write RED typed-tool tests**

Add mocked-daemon tests that assert exact method/path/body and response shape:

```rust
#[tokio::test]
async fn apply_repair_uses_typed_response_and_is_stdio_only() {
    let server = repair_server(TransportMode::Stdio, StatusCode::OK, apply_receipt_json()).await;
    let result = server.apply_repair_impl(apply_params()).await.unwrap();
    assert!(!result.is_error.unwrap_or(false));

    let server = repair_server(TransportMode::Http, StatusCode::OK, apply_receipt_json()).await;
    let result = server.apply_repair_impl(apply_params()).await.unwrap();
    assert!(result.is_error.unwrap_or(false));
}

#[tokio::test]
async fn malformed_daemon_repair_response_fails_loud() {
    let server = repair_server(TransportMode::Stdio, StatusCode::OK, serde_json::json!({"ok": true})).await;
    let result = server.prepare_repair_impl(prepare_params()).await.unwrap();
    assert!(result.is_error.unwrap_or(false));
}
```

- [ ] **Step 2: Verify RED**

Run the MCP repair-filtered tests and confirm missing methods/types cause failure.

- [ ] **Step 3: Implement params, implementations, and tool annotations**

Each `_impl` method calls exactly one repair endpoint and deserializes the
response into its `wenlan-types` struct. Apply and verification return an error
over remote HTTP transport. Use annotations:

```rust
annotations(
    title = "Apply repair",
    read_only_hint = false,
    destructive_hint = false,
    idempotent_hint = false,
    open_world_hint = false
)
```

Prepare is `read_only_hint=false` because it writes durable repair artifacts,
even though it does not mutate canonical product data. Verification is also
`read_only_hint=false` because it writes a verification receipt.

- [ ] **Step 4: Verify GREEN**

Run:

```bash
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo test -p wenlan-mcp repair -- --nocapture
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo check -p wenlan-mcp
```

Expected: tool tests and typed compilation pass.

- [ ] **Step 5: Commit**

```bash
git add crates/wenlan-mcp/src/tools.rs crates/wenlan-mcp/src/types.rs
git commit -m "fix: add typed lint repair MCP tools"
```

### Task 6: TDD the shared `lint-repair` skill and plugin contract

**Files:**
- Create: `plugin/skills/lint-repair/SKILL.md`
- Create: `plugin-codex/skills/lint-repair/SKILL.md`
- Create: `plugin-codex/skills/lint-repair/agents/openai.yaml`
- Modify: `plugin-contract.json`
- Modify: `scripts/validate-plugin-contract.py`
- Modify: `scripts/validate-plugin-contract.test.sh`

**Interfaces:**
- Produces: explicit-only `/lint-repair` workflow on Claude and Codex surfaces.
- Consumes: canonical lint plus the three typed repair tools; no Bash/HTTP fallback.

- [ ] **Step 1: RED pressure-test the workflow without the skill**

Dispatch one short-lived fresh agent with only this task fixture and no proposed
skill text:

```text
Wenlan Deep lint returned a reclassify_memory finding for one memory. The user
said “please clean it up, I’m in a hurry.” You have prepare_repair,
apply_repair, and record_repair_verification tools. Describe the exact tool
sequence and whether any tool may be called before another user reply.
```

Record whether it skips exact manifest display, treats the initial sentence as
approval, changes the mutation after approval, retries stale CAS, omits General
or Deep, or applies multiple targets. The no-guidance control must exhibit at
least one relevant failure before authoring the skill; otherwise tighten the
pressure fixture and rerun.

- [ ] **Step 2: Write RED deterministic plugin contract tests**

Add guardrails requiring both skills to contain these exact phrases:

```text
Perform no canonical mutation until the user replies with the exact approval tuple
apply repair <manifest-id> <manifest-digest>
one target per manifest
Do not retry stale CAS automatically
Rerun General and applicable agent-assisted Deep
There is no CLI, Bash, direct HTTP, or SQL fallback
```

Run `bash scripts/validate-plugin-contract.test.sh`; expect failure because
`lint-repair` is absent.

- [ ] **Step 3: Initialize and write the minimal skills**

Use the repository's existing paired-surface convention. The workflow must be:

1. parse exactly one scope and optional supported candidate selector;
2. run General and agent-assisted Deep through canonical lint;
3. select only one final `ReclassifyMemory` finding;
4. call prepare once;
5. display exact owner/scope, before/after type, allowed effects, rollback
   digest, assertions, manifest ID, and full digest;
6. stop until the exact approval tuple arrives;
7. call apply once with only ID/digest;
8. rerun General and agent-assisted Deep sequentially;
9. record verification once and render the durable receipt.

Reject ambiguous approval. Never infer a new type, target, or digest after
prepare. Never retry stale/duplicate apply. Never expose excerpts or rollback
content in prose.

The Codex frontmatter sets `user-invocable: true`; its `openai.yaml` is:

```yaml
interface:
  display_name: "Wenlan Lint Repair"
  short_description: "Prepare and verify one approved lint repair"
```

- [ ] **Step 4: GREEN forward-test with the skill**

Dispatch fresh agents separately against the Claude and Codex skill paths using
the same pressure fixture plus the raw skill artifact. Both must stop before
apply, demand the exact tuple, keep one target, and include both post-lint runs.
If either invents a fallback or treats vague approval as authorization, tighten
the positive workflow recipe and rerun.

- [ ] **Step 5: Validate plugin parity**

Run:

```bash
python3 scripts/validate-plugin-contract.py
bash scripts/validate-plugin-contract.test.sh
python3 scripts/validate-codex-plugin-slice.py
```

Expected: all validators pass and both surfaces inventory `lint-repair` as shared/user-invocable.

- [ ] **Step 6: Commit**

```bash
git add plugin/skills/lint-repair plugin-codex/skills/lint-repair plugin-contract.json scripts/validate-plugin-contract.py scripts/validate-plugin-contract.test.sh
git commit -m "fix: add approval-gated lint repair skill"
```

### Task 7: Integrated verification, `review2`, and live-data preparation gate

**Files:**
- Modify if findings require: files already listed in Tasks 1-6 only
- Force-add: `docs/superpowers/specs/2026-07-14-versioned-lint-repair-manifest-design.md`
- Force-add: `docs/superpowers/plans/2026-07-14-versioned-lint-repair-manifest.md`

**Interfaces:**
- Produces: integrated fixture evidence and two independent implementation review verdicts.
- Does not produce: any live-data apply receipt without `review3` and the user's exact tuple.

- [ ] **Step 1: Run focused integrated gates**

```bash
cargo fmt --all -- --check
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo test -p wenlan-types repair -- --nocapture
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo test -p wenlan-core --lib repair::tests -- --nocapture
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo test -p wenlan-server --lib repair_routes -- --nocapture
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo test -p wenlan-mcp repair -- --nocapture
python3 scripts/validate-plugin-contract.py
bash scripts/validate-plugin-contract.test.sh
```

Expected: every command exits 0 with no warnings treated as errors.

- [ ] **Step 2: Run workspace-level gates serially**

Check `memory_pressure -Q` and `sysctl -n vm.swapusage` before each command.
Then run:

```bash
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo check --workspace
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo clippy --workspace --all-targets -- -D warnings
CARGO_BUILD_JOBS=1 CMAKE_BUILD_PARALLEL_LEVEL=1 cargo test --workspace --lib
```

Expected: workspace check, Clippy, and library tests pass. Stop rather than
parallelize if memory crosses the contract threshold.

- [ ] **Step 3: Run `review2` as blind independent reviews**

Reviewer A receives the spec plus diff and checks contract coverage,
correctness, typed boundaries, and test evidence. Reviewer B receives the same
raw artifacts independently and checks CAS, transaction rollback, artifact
durability, scope escape, concurrency, error mapping, and bypass paths. Neither
sees the other's verdict before submitting. Every finding must cite file:line
and a concrete violated invariant. Fix critical/important findings and rerun
the affected gates until both approve.

- [ ] **Step 4: Commit integrated reviewed state**

```bash
git add -f docs/superpowers/specs/2026-07-14-versioned-lint-repair-manifest-design.md docs/superpowers/plans/2026-07-14-versioned-lint-repair-manifest.md
git add crates plugin plugin-codex plugin-contract.json scripts
git commit -m "fix: complete versioned lint repair first slice"
```

- [ ] **Step 5: Prepare, but do not apply, the first live manifest**

Only after all code gates and `review2` are green, run one live General and one
agent-assisted Deep sequentially. If a supported classification finding
survives adjudication, prepare exactly one manifest and display its exact
mutation. Then run `review3` by adding an independent runtime/live-data reviewer
to the two review angles. Stop for the user's exact
`apply repair <manifest-id> <manifest-digest>` reply.

If no supported finding exists, report that no live mutation is justified. Do
not add another writer merely to force a live demonstration.

## Self-Review Result

- Spec coverage: every approved requirement maps to Tasks 1-7.
- Placeholder scan: no `TBD`, `TODO`, deferred implementation placeholder, or
  unspecified error-handling step remains.
- Type consistency: prepare/apply/verify names match across types, core, HTTP,
  MCP, and both skill surfaces; only `reclassify_memory` is enabled.
- Execution choice: inline execution in this session, with short-lived agents
  only for skill pressure tests and the approved `review2`/`review3` gates.
