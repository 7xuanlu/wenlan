# PR-A: domain → space rename + e2e scoping Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename the user-visible scoping concept from `domain` to `space` across the Origin workspace, wire space filter end-to-end on all retrieval paths, and prove correctness with an acceptance e2e test (capture A+B, recall space=A returns only A).

**Architecture:** SQL ALTER (RENAME COLUMN) at migration 50 boundary; old migrations preserved verbatim (they ran against `domain`-named columns and must keep doing so on fresh DBs). Rust field rename across 5 crates by manual walk (not blanket sed). One-release `#[serde(alias = "domain")]` shim on wire types for back-compat with cached MCP clients. Wire-gap fixes piggyback: `db.search` doc-path honors space; dead `classification.space` + `classified_domain=None` branches deleted; `handle_context` filter extended to all shelves.

**Tech Stack:** Rust 1.83, libSQL (SQLite ≥3.25 RENAME COLUMN), Cargo workspace (5 crates), Axum 0.8, rmcp (MCP SDK).

**Worktree:** `/Users/lucian/Repos/origin/.worktrees/feature/space-rename` (branch `feature/space-rename`).

**PR title (controls release-please):** `feat: rename domain → space + complete e2e scoping (BREAKING CHANGE)` — pre-1.0 minor bump 0.6.x → 0.7.0.

---

## Pre-flight audit (must complete before Task 3)

### Task 1: Audit `list_spaces()` + `SpaceStore` read paths

**Files:**
- Read: `crates/origin-core/src/db.rs:4779` (`list_spaces`), `crates/origin-core/src/spaces.rs` (full file), `crates/origin-server/src/memory_routes.rs:2368` (space assignment route)

- [ ] **Step 1: Grep all callers of `list_spaces`**

Run: `rg -n --no-heading 'list_spaces\b' crates/`
Expected: handler in `memory_routes.rs` or `routes.rs`, possibly UI surface (none expected post-tauri-extract).

- [ ] **Step 2: Grep all callers of `SpaceStore` methods**

Run: `rg -n --no-heading 'space_store\.|SpaceStore' crates/ --type rust`
Expected: write-side (`set_document_tags`, `auto_create_space_if_needed`), read-side (per audit suspected dead; verify).

- [ ] **Step 3: Document findings in `docs/superpowers/plans/audits/2026-05-14-spacestore-read-paths.md`**

Write a 1-page markdown audit with three sections: "Read paths confirmed live," "Read paths confirmed dead," "Mixed/unsure." This informs PR-B scope but does NOT block PR-A.

- [ ] **Step 4: Commit the audit doc only (no code yet)**

```bash
git -C /Users/lucian/Repos/origin/.worktrees/feature/space-rename add docs/superpowers/plans/audits/2026-05-14-spacestore-read-paths.md
git -C /Users/lucian/Repos/origin/.worktrees/feature/space-rename commit -m "docs: audit SpaceStore read paths (informs PR-B scope)" --no-verify
```

**Done when:** audit file exists and committed. No Rust changes yet.

---

### Task 2: Decide `memory.space` ↔ `Space.name` join contract

**Files:**
- Read: `crates/origin-core/src/db.rs:4779-4920` (`list_spaces` body + how it joins `memories.domain = spaces.name`), `crates/origin-core/src/db.rs:12177` (`auto_create_space_if_needed`)

- [ ] **Step 1: Trace the join key**

Find the SQL that joins memories to spaces. Currently `memories.domain = spaces.name` per audit. Confirm whether `name` is canonical case-sensitive identifier or display label.

- [ ] **Step 2: Choose contract**

Two options:
- **A. Keep current behavior**: `memories.space` is a free-form string serving as both ID and display label. `auto_create_space_if_needed` creates a `spaces` row with `id = name = <string>` so the join works.
- **B. Normalize**: `memories.space` references `spaces.id` (FK), `spaces.name` becomes display-only.

Decision rule: pick A if no UI rename surface needed in PR-A scope, pick B if you want a clean FK boundary now.

**Default for this PR: A** (no FK migration, smaller blast radius, matches Karpathy surgical-change rule).

- [ ] **Step 3: Document decision inline in plan**

Append to this plan file under `## Decisions made during execution` (create if absent): one bullet recording A or B and rationale.

- [ ] **Step 4: Commit the decision note**

```bash
git -C /Users/lucian/Repos/origin/.worktrees/feature/space-rename add docs/superpowers/plans/2026-05-14-pr-a-domain-space-rename.md
git -C /Users/lucian/Repos/origin/.worktrees/feature/space-rename commit -m "docs: record memory.space/Space.name join contract decision" --no-verify
```

**Done when:** decision recorded and committed.

---

## Migration 50 (schema layer)

### Task 3: Write failing migration replay test

**Files:**
- Create test: `crates/origin-core/src/db.rs` (append to migration test module near line 27946 where migrations 46/49 replay tests live)

- [ ] **Step 1: Write the failing test (red)**

Append to db.rs migration tests module:

```rust
#[tokio::test]
async fn migration_50_renames_domain_to_space() {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = MemoryDB::new(db_path.clone(), Arc::new(NoopEmitter)).await.unwrap();

    // Insert a memory with domain populated, then roll back user_version to 49
    db.insert_memory_for_test_with_domain("test fact", Some("alpha")).await.unwrap();
    {
        let conn = db.conn.lock().await;
        conn.execute("PRAGMA user_version = 49", ()).await.unwrap();
    }
    drop(db);

    // Re-open — migrations 50 should re-fire.
    let db = MemoryDB::new(db_path, Arc::new(NoopEmitter)).await.unwrap();
    let conn = db.conn.lock().await;

    // Column renamed
    let mut rows = conn.query("SELECT space FROM memories WHERE source_text = ?1", libsql::params!["test fact"]).await.unwrap();
    let row = rows.next().await.unwrap().expect("memory row present");
    let space: Option<String> = row.get(0).unwrap();
    assert_eq!(space.as_deref(), Some("alpha"), "data must survive RENAME COLUMN");

    // Index renamed
    let mut idx_rows = conn.query("SELECT name FROM sqlite_master WHERE type='index' AND name='idx_memories_space'", ()).await.unwrap();
    assert!(idx_rows.next().await.unwrap().is_some(), "idx_memories_space must exist after migration 50");

    let mut old_idx = conn.query("SELECT name FROM sqlite_master WHERE type='index' AND name='idx_memories_domain'", ()).await.unwrap();
    assert!(old_idx.next().await.unwrap().is_none(), "idx_memories_domain must be dropped after migration 50");
}
```

- [ ] **Step 2: Add the test helper if missing**

If `insert_memory_for_test_with_domain` doesn't exist, add to db.rs test module:

```rust
#[cfg(test)]
impl MemoryDB {
    pub async fn insert_memory_for_test_with_domain(&self, content: &str, domain: Option<&str>) -> Result<String, OriginError> {
        let id = format!("mem_test_{}", uuid::Uuid::new_v4().simple());
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO memories (source_id, source, content, source_text, memory_type, domain, created_at, last_modified) \
             VALUES (?1, 'memory', ?2, ?2, 'fact', ?3, unixepoch('now'), unixepoch('now'))",
            libsql::params![id.clone(), content.to_string(), domain.map(String::from)],
        ).await?;
        Ok(id)
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run from worktree: `cargo test -p origin-core --lib migration_50_renames_domain_to_space -- --nocapture`
Expected: FAIL — migration 50 doesn't exist yet, column is still `domain`.

- [ ] **Step 4: Commit the failing test**

```bash
git add crates/origin-core/src/db.rs
git commit -m "test: add failing migration 50 replay test (red)" --no-verify
```

**Done when:** test fails predictably; commit lands.

---

### Task 4: Implement migration 50

**Files:**
- Modify: `crates/origin-core/src/db.rs` (locate the migration block matching the pattern of migrations 46/49 — after the latest existing migration block)

- [ ] **Step 1: Find current latest migration**

Run: `rg -n --no-heading 'user_version.*49|Migration 49' crates/origin-core/src/db.rs | head -5`
Note the line number of the block.

- [ ] **Step 2: Add migration 50 block immediately after migration 49**

Insert into db.rs (after migration 49's block):

```rust
// ==================== Migration 50: rename domain → space ====================
if user_version < 50 {
    conn.execute("BEGIN", ()).await?;
    let result: Result<(), _> = async {
        conn.execute("PRAGMA foreign_keys = OFF", ()).await?;
        conn.execute("ALTER TABLE memories RENAME COLUMN domain TO space", ()).await?;
        conn.execute("ALTER TABLE entities RENAME COLUMN domain TO space", ()).await?;
        // pages table exists post-migration-46; rename its domain column too
        conn.execute("ALTER TABLE pages RENAME COLUMN domain TO space", ()).await?;
        // Drop old indexes, recreate against renamed column.
        conn.execute("DROP INDEX IF EXISTS idx_memories_domain", ()).await?;
        conn.execute("DROP INDEX IF EXISTS idx_entities_domain", ()).await?;
        conn.execute("DROP INDEX IF EXISTS idx_pages_domain", ()).await?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_space ON memories(space) WHERE space IS NOT NULL",
            (),
        ).await?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_entities_space ON entities(space) WHERE space IS NOT NULL",
            (),
        ).await?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_pages_space ON pages(space) WHERE space IS NOT NULL",
            (),
        ).await?;
        conn.execute("PRAGMA foreign_keys = ON", ()).await?;
        conn.execute("PRAGMA user_version = 50", ()).await?;
        Ok::<(), OriginError>(())
    }.await;
    match result {
        Ok(()) => {
            conn.execute("COMMIT", ()).await?;
            log::info!("[memory_db] migration 50: renamed domain → space + reindexed");
        }
        Err(e) => {
            conn.execute("ROLLBACK", ()).await?;
            return Err(e);
        }
    }
}
```

**Critical rules:**
- DO NOT edit migration 12, 13, 24, 26, 31, 41 bodies. They reference `domain` because they ran against pre-50 schema. Fresh DBs run them sequentially → column is `domain` at that point → migration 50 renames it.
- The CREATE TABLE statements for `memories`/`entities`/`pages` at db.rs lines 696, 749, 4351 must KEEP `domain TEXT` for the same reason: they run before migration 50 on fresh DBs.

- [ ] **Step 3: Run migration test to verify it passes**

Run: `cargo test -p origin-core --lib migration_50_renames_domain_to_space -- --nocapture`
Expected: PASS.

- [ ] **Step 4: Run full migration test suite (sanity check that older migrations still pass)**

Run: `cargo test -p origin-core --lib migration_ -- --nocapture`
Expected: all PASS. If any fail, the rename collided with old migration text.

- [ ] **Step 5: Commit**

```bash
git add crates/origin-core/src/db.rs
git commit -m "feat(db): migration 50 — rename memories.domain → space + reindex" --no-verify
```

**Done when:** migration tests green.

---

## Wire-type rename (origin-types — bottom of dep graph)

### Task 5: Rename `domain` → `space` in origin-types + add serde alias

**Files:**
- Modify: `crates/origin-types/src/memory.rs:27,77,168,188,233,330,448,470,489` (all `pub domain: Option<String>` and related)
- Modify: `crates/origin-types/src/requests.rs:15,44,54,89,122,132,177,244,412,448,542,554`
- Modify: `crates/origin-types/src/entities.rs:12`
- Modify: `crates/origin-types/src/pages.rs:14`
- Modify: `crates/origin-types/src/sources.rs:151,217`
- Modify: `crates/origin-types/src/lib.rs:131`

- [ ] **Step 1: Per-file rename — manual walk**

For each file above, replace `pub domain: Option<String>` with:

```rust
#[serde(alias = "domain")]
pub space: Option<String>,
```

The `alias` allows JSON inputs sending `"domain": ...` to still deserialize (back-compat for cached MCP clients). Outputs serialize as `space`.

For struct-literal initializations like `domain: None,` rename field name to `space: None`.

For doc-strings that say "domain", rewrite to "space" — except where the doc is referencing DNS domain or problem domain (none expected in origin-types).

For test JSON literals like `"domain":null`, keep `"domain"` (back-compat tests) OR update to `"space"` if the test is asserting current output shape. Default: update to `"space"` since `serde(rename(serialize = ...))` is not used, so output is now `"space"`.

- [ ] **Step 2: Compile-check the crate**

Run: `cargo check -p origin-types`
Expected: clean. If errors, internal field reference somewhere not renamed; fix.

- [ ] **Step 3: Run origin-types tests**

Run: `cargo test -p origin-types`
Expected: all pass. If serde-alias test fails, the alias attribute is on the wrong field or struct.

- [ ] **Step 4: Commit**

```bash
git add crates/origin-types/
git commit -m "refactor(types): rename domain → space (serde alias preserved for back-compat)" --no-verify
```

**Done when:** origin-types compiles + tests pass.

---

## Core library rename (origin-core — biggest crate)

### Task 6: Rename `domain` → `space` in origin-core db.rs (manual walk, preserve historic SQL)

**Files:**
- Modify: `crates/origin-core/src/db.rs` (~340 occurrences)

- [ ] **Step 1: Identify untouchable historic-migration regions**

Run: `rg -n --no-heading 'Migration (\d+)' crates/origin-core/src/db.rs | head -30`
List every `// ==================== Migration N ====================` boundary. The body between boundary N and boundary N+1 (for N < 50) is HISTORIC — must not be renamed. Migration 50's body and all post-migration code IS renamed.

- [ ] **Step 2: Identify untouchable CREATE TABLE statements**

The initial CREATE TABLE statements for `memories` (line 696), `entities` (line 749), and `pages` (line 4351) define schemas as they exist BEFORE migration 50 runs on fresh DBs. Keep `domain TEXT` in those CREATE TABLE bodies. Migration 50 will rename at runtime.

- [ ] **Step 3: Rename outside historic regions**

Categories to rename:
- Function parameter names: `domain: Option<&str>` → `space: Option<&str>`
- Function names: `domain_has_memories` → `space_has_memories`, `get_memory_domain` → `get_memory_space`, `list_decision_domains` → `list_decision_spaces`, `update_domain` → `update_space`, `list_pages_by_domain` → `list_pages_by_space`.
- Variable names: `let domain = ...` → `let space = ...`, `final_domain` → `final_space`, `domain_filter` → `space_filter`.
- Struct field accesses: `memories[i].domain` → `memories[i].space`.
- New SQL strings (post-migration-50 logic): `WHERE c.domain = ?` → `WHERE c.space = ?`, `SELECT domain FROM memories` (in new helpers) → `SELECT space FROM memories`.
- Doc comments outside historic blocks.

Critical preservation:
- Embed prefix at line 2667-2668 — `format!("[{}] {}", domain, t)` is preserved logic; just rename local variable `domain` → `space`. The embedded string still uses the value, so semantics unchanged.
- Lines 1934-1938 (migration 12 INSERT-SELECT), 1962 (migration 13 UPDATE), 2441/2475/2485/2503/2519 (migration 24 schema recreation): UNTOUCHED. They reference column `domain` because at their run-time, that's the column name.

- [ ] **Step 4: Compile-check**

Run: `cargo check -p origin-core`
Expected: clean. Watch for: dependent crates not yet renamed → reference `memory.domain` field that no longer exists. Origin-core consumes origin-types which is renamed → struct field is `space` now. If a usage missed the rename, error here.

- [ ] **Step 5: Run library tests**

Run: `cargo test -p origin-core --lib`
Expected: all pass. Migration tests + classify + post_write etc. If a test fails, the fixture or assertion still references `domain` and needs update.

- [ ] **Step 6: Commit**

```bash
git add crates/origin-core/src/db.rs
git commit -m "refactor(core): rename domain → space in db.rs (historic migrations preserved)" --no-verify
```

**Done when:** origin-core compiles + lib tests pass.

---

### Task 7: Rename in origin-core non-db modules

**Files:**
- Modify (per grep earlier): `classify.rs:15`, `post_write.rs:15`, `refinery/mod.rs:11`, `synthesis/distill.rs:15`, `synthesis/recaps.rs:7`, `synthesis/decision_logs.rs:5`, `synthesis/emergence.rs:3`, `synthesis/refinement_queue.rs:1`, `narrative.rs` (if has refs), `briefing.rs:2`, `topic_match.rs:8`, `contradiction.rs:4`, `quality_gate.rs:7`, `tuning.rs:4`, `prompts/defaults.rs:8`, `kg/entity_extraction.rs:1`, `pages.rs:1`, `post_ingest.rs:1`, `schema.rs:1`, `memory_schema.rs:1`, `importer.rs:3`, `engine.rs:1`, `sources/obsidian.rs:7`, `sources/page_watcher.rs:3`, `sources/local_files.rs:1`, `export/knowledge.rs:6`, `export/obsidian.rs:3`, `bin/model_benchmark.rs:3`, `bin/hard_distill.rs:2`, `llm_provider.rs:7`
- Modify (eval modules — careful, see fixture exception): `eval/gen.rs:21`, `eval/retrieval.rs:18`, `eval/fixtures.rs:18`, `eval/locomo.rs:16`, `eval/lifecycle.rs:15`, `eval/answer_quality.rs:10`, `eval/pipeline.rs:9`, `eval/longmemeval.rs:7`, `eval/shared.rs:4`, `eval/runner.rs:4`, `eval/context_path.rs:2`

- [ ] **Step 1: Rename Rust identifiers (variables, struct fields, function params, doc comments)**

For each file, replace identifier-level `domain` → `space` outside string literals. Use `cargo check` after each crate sub-group to catch typos early.

- [ ] **Step 2: PRESERVE eval prompt fixtures**

In `eval/gen.rs` lines 513, 519, 676-678: `"domain": "backend"|"cooking"|"devops"|"fitness"` are LLM prompt input strings — model was never trained on `"space": "cooking"`. Leave these as `"domain": ...` literals (they will round-trip through serde via `#[serde(alias = "domain")]` correctly).

Same rule for `llm_provider.rs:1400,1410` test JSON, `classify.rs:495-532` test JSON. These are LLM-input fixtures, not internal serde.

- [ ] **Step 3: Update Rust struct field accesses + variable names in eval modules**

Outside the prompt-string literals, `let domain = ...` → `let space = ...`, `cluster.domain` → `cluster.space`, etc.

- [ ] **Step 4: Compile-check whole crate**

Run: `cargo check -p origin-core --all-targets`
Expected: clean.

- [ ] **Step 5: Run all origin-core tests including ignored?**

Run: `cargo test -p origin-core --lib` (skip `--ignored` since those need GPU per AGENTS.md).
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/origin-core/src/
git commit -m "refactor(core): rename domain → space in non-db modules (eval LLM fixtures preserved)" --no-verify
```

**Done when:** origin-core fully renamed + tests pass.

---

## Server rename + wire-gap fixes (origin-server)

### Task 8: Rename in origin-server + extend handle_context filter

**Files:**
- Modify: `crates/origin-server/src/routes.rs` (lines 27, 107, 240, 668, 720, 730, 849 — and the `handle_context` body lines ~237-260)
- Modify: `crates/origin-server/src/memory_routes.rs` (~28 occurrences, especially lines 349, 429, 445, 639-641, 801, 869-873, 1043, 1121, 1404, 1419, 1791, 1810, 1840, 1853, 2368, 2597, 2693, 2699, 3235)
- Modify: `crates/origin-server/src/ingest_routes.rs:76,106,112,126,171`
- Modify: `crates/origin-server/src/import_routes.rs:285,308,335,356`
- Modify: `crates/origin-server/src/websocket.rs:166`
- Modify: `crates/origin-server/src/ingest_batcher.rs:251`
- Modify: `crates/origin-server/src/cmd_backfill.rs:5` (doc comment only)

- [ ] **Step 1: Rename `SearchRequest.domain` → `space`** (routes.rs:27)

```rust
#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub source_filter: Option<String>,
    #[serde(default, alias = "domain")]
    pub space: Option<String>,
}
```

- [ ] **Step 2: Extend `handle_context` filter to all `load_memories_by_type` calls** (routes.rs lines 240-260)

Find every `load_memories_by_type(memory_type, limit, _)` call in `handle_context`. Currently only identity/preference/decision pass `domain_filter`. Pass `space_filter` to ALL calls (recap, fact, gotcha, lesson, every shelf).

- [ ] **Step 3: Kill dead `classification.space` fallback**

Remove `let domain_filter = req.domain.as_deref().or(classification.space.as_deref());` and replace with:

```rust
let space_filter = req.space.as_deref();
```

(Classifier never populates `classification.space` — verified via grep.)

- [ ] **Step 4: Kill dead `classified_domain = None` branch in `memory_routes.rs`** (~line 264)

Find the `let classified_domain: Option<String> = None;` line and the subsequent `final_domain = req.domain.or(classified_domain)` fallback. Replace with:

```rust
let final_space = req.space.clone();
```

Eliminating the fake fallback.

- [ ] **Step 5: Wire `db.search` doc-path to honor space**

`crates/origin-core/src/db.rs` — find `pub async fn search(`. Add `space: Option<&str>` parameter, mirror the filter pattern from `search_memory` (line 6204):

```rust
if let Some(s) = space {
    if s == "uncategorized" {
        // NOTE: documents don't have space — skip filter entirely
        // OR if documents are stored with space, add WHERE doc.space IS NULL
    } else {
        // documents currently lack a space column — this is a wire-gap that requires
        // a schema decision. If documents need space scoping, add a `space` column
        // in a follow-up. For PR-A, document the limitation:
    }
}
```

If documents don't have `space` column (verify via schema), this becomes a no-op + doc comment explaining. The acceptance gate test for the doc-path can then assert "doc-path with space filter returns all docs" (no change) and a future PR adds the column.

**Alternative if you want to ship space scoping for docs in PR-A**: add `documents.space TEXT` column in migration 50 + populate from ingest path. That extends scope. Default: doc-path filter is no-op + documented; revisit in PR-C.

Update `handle_search` (routes.rs:115) else-branch to pass `req.space.as_deref()`:

```rust
} else {
    db.search(&req.query, req.limit, req.source_filter.as_deref(), req.space.as_deref())
        .await
        .map_err(|e| ServerError::SearchFailed(e.to_string()))?
}
```

- [ ] **Step 6: Rename remaining call-sites + types throughout origin-server**

Per-file walk. Same rules as Task 6 step 3.

- [ ] **Step 7: Compile + tests**

```bash
cargo check -p origin-server
cargo test -p origin-server
```
Expected: clean and green.

- [ ] **Step 8: Commit**

```bash
git add crates/origin-server/
git commit -m "refactor(server): rename domain → space + extend handle_context filter + wire db.search space param" --no-verify
```

**Done when:** origin-server compiles + tests pass.

---

## MCP rename + back-compat alias (origin-mcp)

### Task 9: Rename in origin-mcp tools.rs + add serde alias on every input param

**Files:**
- Modify: `crates/origin-mcp/src/tools.rs` (~81 occurrences including all `Params` structs at lines 94, 131, 146, 148, 162, 230, 309, 391, 392, 412, 439, 441 + every test fixture)

- [ ] **Step 1: Rename Params structs**

For every `Params` struct with a `pub domain: Option<String>` field:

```rust
#[schemars(description = "Topic scope (e.g. 'work', 'personal'). Auto-detected from cwd if omitted.")]
#[serde(alias = "domain")]
pub space: Option<String>,
```

Applies to: `CaptureParams`, `RecallParams`, `ContextParams`, `DistillParams`, `ListMemoriesParams`, `SearchPagesParams`, `ListNurtureCardsParams`, `ListMemoriesByDomainParams` (rename to `ListMemoriesBySpaceParams`).

- [ ] **Step 2: Rewrite tool descriptions**

Replace `"Scope context to a domain/space"` with `"Scope context to a space (e.g. 'work', 'personal')"`. Drop the slash. Update distill's target description: `"a domain value (e.g. 'work', 'personal') to scope to that domain"` → `"a space value (e.g. 'work', 'personal') to scope to that space"`.

- [ ] **Step 3: Update HTTP query construction**

Find `q.push(format!("domain={}", url_encode_simple(d)));` at line 1335. Replace with:

```rust
q.push(format!("space={}", url_encode_simple(s)));
```

The daemon's HTTP handler (renamed in Task 8) accepts `space` query param. Variable rename: `params.domain` → `params.space`.

- [ ] **Step 4: Update internal forwarding (origin-types request construction)**

Every `.domain: params.domain` line becomes `.space: params.space`. Origin-types fields already renamed in Task 5.

- [ ] **Step 5: Update tool tests**

Test assertions at lines 2362, 2381, 2410, 2416, 2441, 2446, 2450, 2465, 2669, 2690, 2693, 2781, 2804, 2897, 2942, 2971, 3006, 3059, 3068, 3078, 3087, 3090, 3098, 3107, 3120, 3134, 3148, 3171, 3178, 3187, 3204, 3223 etc.

For most: `params.domain` → `params.space`, `json["domain"]` → `json["space"]`, `domain: Some("work".into())` → `space: Some("work".into())`.

KEEP ONE backward-compat test: add a new test asserting that JSON input `{"domain": "work"}` still deserializes (the `serde(alias = "domain")` shim):

```rust
#[test]
fn legacy_domain_alias_still_deserializes() {
    let json = r#"{"topic": "test", "domain": "work"}"#;
    let params: ContextParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.space.as_deref(), Some("work"), "legacy 'domain' JSON key must deserialize to space via alias");
}
```

- [ ] **Step 6: Compile + tests**

```bash
cargo check -p origin-mcp
cargo test -p origin-mcp
```
Expected: clean and green. Legacy-alias test must pass.

- [ ] **Step 7: Commit**

```bash
git add crates/origin-mcp/
git commit -m "refactor(mcp): rename domain → space (serde alias preserves legacy clients for one release)" --no-verify
```

**Done when:** origin-mcp compiles + tests pass including back-compat alias test.

---

## CLI rename (origin)

### Task 10: Rename in origin-cli

**Files:**
- Modify: `crates/origin-cli/src/client.rs:66,121,155`

- [ ] **Step 1: Rename field references**

```rust
// before:
domain: None,
// after:
space: None,
```

(origin-types fields are already renamed.)

- [ ] **Step 2: Compile + tests**

```bash
cargo check -p origin
cargo test -p origin
```
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add crates/origin-cli/
git commit -m "refactor(cli): rename domain → space" --no-verify
```

**Done when:** CLI compiles.

---

## Workspace check + skill markdown

### Task 11: Full workspace compile + clippy + workspace test

**Files:** none — verification only

- [ ] **Step 1: Full workspace compile**

```bash
cargo check --workspace --all-targets
```
Expected: zero errors. If any error, find the missed `domain` reference, fix in the appropriate Task's crate, amend the relevant commit (or add a "fix: missed-rename" commit).

- [ ] **Step 2: Workspace clippy**

```bash
cargo clippy --workspace --all-targets
```
Expected: zero new warnings.

- [ ] **Step 3: Workspace tests (lib + bin only — skip ignored GPU tests)**

```bash
cargo test --workspace --lib
cargo test --workspace --bins
```
Expected: all green.

- [ ] **Step 4: No commit (verification only)**

Done when: full workspace clean.

---

### Task 12: Skill markdown rename

**Files:**
- Modify: `plugin/skills/brief/SKILL.md` (line 59, 67 — `domain=<inferred from cwd>` → `space=<inferred from cwd>`)
- Modify: `plugin/skills/capture/SKILL.md` (line 26, 54)
- Modify: `plugin/skills/recall/SKILL.md` (line 42, 47)
- Modify: `plugin/skills/distill/SKILL.md` (lines 88, 128, 168 — `cluster.domain` → `cluster.space`)

- [ ] **Step 1: Per-file rewrite**

For each skill:
- Replace `domain=...` → `space=...` in code examples.
- Replace prose mentions of "domain" referring to topic/project scoping with "space".
- Keep prose mentions of "domain" if referring to DNS or problem-domain (none expected in these skills).
- Drop "Auto-detected if omitted" phrasing where it overstates daemon behavior. Replace with: "Always pass when scope is known; if uncertain, run `list_spaces` first (post-PR-C) or omit."

- [ ] **Step 2: Verify skill markdown by grep**

Run: `rg -n --no-heading '\bdomain\b' plugin/skills/`
Expected: zero hits (or only legitimate non-scoping references with comment).

- [ ] **Step 3: Commit**

```bash
git add plugin/skills/
git commit -m "docs(skills): rename domain → space in brief/capture/recall/distill SKILL.md" --no-verify
```

**Done when:** skill markdown grep is clean.

---

## E2E acceptance tests

### Task 13: SQL-filter e2e (DB layer)

**Files:**
- Create: `crates/origin-core/tests/space_scoping_e2e.rs`

- [ ] **Step 1: Add the `insert_memory_for_test_with_space` helper**

The Task 3 helper writes to column `domain` (correct because Task 3's migration replay test inserts at user_version=49 before migration 50 fires). For post-rename e2e tests (this task onwards), add a sibling helper that writes the `space` column directly. In `crates/origin-core/src/db.rs` test module, append:

```rust
#[cfg(test)]
impl MemoryDB {
    pub async fn insert_memory_for_test_with_space(&self, content: &str, space: Option<&str>) -> Result<String, OriginError> {
        let id = format!("mem_test_{}", uuid::Uuid::new_v4().simple());
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO memories (source_id, source, content, source_text, memory_type, space, created_at, last_modified) \
             VALUES (?1, 'memory', ?2, ?2, 'fact', ?3, unixepoch('now'), unixepoch('now'))",
            libsql::params![id.clone(), content.to_string(), space.map(String::from)],
        ).await?;
        Ok(id)
    }
}
```

Both helpers coexist: `..._with_domain` for migration-replay tests, `..._with_space` for post-rename tests.

- [ ] **Step 2: Write the test file**

```rust
//! Acceptance gate for PR-A: space filter end-to-end at DB layer.

use origin_core::db::MemoryDB;
use origin_core::events::NoopEmitter;
use std::sync::Arc;

#[tokio::test]
async fn space_scoping_excludes_other_space() {
    let tmp = tempfile::tempdir().unwrap();
    let db = MemoryDB::new(tmp.path().join("test.db"), Arc::new(NoopEmitter)).await.unwrap();

    db.insert_memory_for_test_with_space("foo fact alpha-only", Some("alpha")).await.unwrap();
    db.insert_memory_for_test_with_space("bar fact beta-only", Some("beta")).await.unwrap();

    let r = db.search_memory("fact", 10, None, Some("alpha"), None, None, None, None).await.unwrap();
    let texts: Vec<&str> = r.iter().map(|x| x.content.as_str()).collect();

    assert!(texts.iter().any(|t| t.contains("foo")), "alpha hit missing");
    assert!(!texts.iter().any(|t| t.contains("bar")), "beta leaked into alpha results");
}

#[tokio::test]
async fn space_uncategorized_matches_null() {
    let tmp = tempfile::tempdir().unwrap();
    let db = MemoryDB::new(tmp.path().join("test.db"), Arc::new(NoopEmitter)).await.unwrap();

    db.insert_memory_for_test_with_space("orphan fact", None).await.unwrap();
    db.insert_memory_for_test_with_space("alpha fact", Some("alpha")).await.unwrap();

    let r = db.search_memory("fact", 10, None, Some("uncategorized"), None, None, None, None).await.unwrap();
    assert!(r.iter().any(|x| x.content.contains("orphan")), "uncategorized must match NULL");
    assert!(!r.iter().any(|x| x.content.contains("alpha fact") && !x.content.contains("orphan")), "alpha must not match uncategorized");
}
```

- [ ] **Step 3: Run the tests**

```bash
cargo test -p origin-core --test space_scoping_e2e
```
Expected: PASS (SQL filter wired pre-rename via Task 6; column rename via Task 4 migration).

- [ ] **Step 4: Commit**

```bash
git add crates/origin-core/src/db.rs crates/origin-core/tests/space_scoping_e2e.rs
git commit -m "test(core): e2e space scoping at DB layer (acceptance gate)" --no-verify
```

**Done when:** e2e SQL tests pass.

---

### Task 14: handle_context extended-filter e2e

**Files:**
- Create: `crates/origin-server/tests/context_space_filter_e2e.rs`

- [ ] **Step 1: Test setup**

Spin up daemon in-process (use existing test harness pattern from origin-server tests; if none, mimic `crates/origin-server/tests/triggered_revisions.rs:1-100`).

- [ ] **Step 2: Write the test**

```rust
//! Verify handle_context filters EVERY shelf by space, not just identity/preference/decision.

#[tokio::test]
async fn context_filters_all_shelves_by_space() {
    let harness = TestDaemon::spawn().await;

    // Insert one memory per shelf in space=alpha, one per shelf in space=beta
    for shelf in ["identity", "preference", "decision", "fact", "lesson", "gotcha", "recap"] {
        harness.store(shelf, "alpha-content", Some("alpha")).await;
        harness.store(shelf, "beta-content", Some("beta")).await;
    }

    let resp = harness.context(Some("alpha")).await;
    let texts: Vec<&str> = resp.memories.iter().map(|m| m.content.as_str()).collect();

    // Every alpha shelf entry should be present; no beta entries.
    assert!(texts.iter().all(|t| !t.contains("beta-content")), "beta leaked into space=alpha context");
    assert!(texts.iter().any(|t| t.contains("alpha-content")), "alpha shelves missing");
}
```

If test harness doesn't exist, alternative: drive directly through `handle_context` handler function with a constructed `ServerState`.

- [ ] **Step 3: Run**

```bash
cargo test -p origin-server --test context_space_filter_e2e
```
Expected: PASS post-Task 8 step 2.

- [ ] **Step 4: Commit**

```bash
git add crates/origin-server/tests/context_space_filter_e2e.rs
git commit -m "test(server): e2e handle_context filters all shelves by space" --no-verify
```

**Done when:** context filter test passes.

---

### Task 15: MCP round-trip e2e

**Files:**
- Create: `crates/origin-mcp/tests/space_roundtrip_e2e.rs`

- [ ] **Step 1: Write the test**

```rust
//! End-to-end via MCP tool layer: capture(space=alpha) + capture(space=beta), recall(space=alpha) returns only alpha.

#[tokio::test]
async fn mcp_capture_and_recall_respects_space() {
    let harness = TestMcpDaemon::spawn().await;

    harness.call_capture(json!({"content": "alpha fact one", "space": "alpha"})).await;
    harness.call_capture(json!({"content": "beta fact two", "space": "beta"})).await;

    let resp = harness.call_recall(json!({"query": "fact", "space": "alpha"})).await;
    let items = resp.get("memories").and_then(|v| v.as_array()).unwrap();
    let texts: Vec<&str> = items.iter().filter_map(|m| m["content"].as_str()).collect();

    assert!(texts.iter().any(|t| t.contains("alpha fact")), "alpha hit missing");
    assert!(!texts.iter().any(|t| t.contains("beta fact")), "beta leaked into alpha results via MCP");
}

#[tokio::test]
async fn mcp_legacy_domain_key_still_works() {
    let harness = TestMcpDaemon::spawn().await;

    // Cached pre-0.7.0 client sends `domain` not `space`. Serde alias must accept it.
    harness.call_capture(json!({"content": "legacy fact", "domain": "alpha"})).await;
    let resp = harness.call_recall(json!({"query": "legacy", "domain": "alpha"})).await;

    let items = resp.get("memories").and_then(|v| v.as_array()).unwrap();
    assert!(!items.is_empty(), "legacy domain= request must still find the memory");
}
```

If the existing MCP test harness lacks `TestMcpDaemon`, use the rmcp-test pattern at `crates/origin-mcp/src/tools.rs` test module (in-process tool dispatch).

- [ ] **Step 2: Run**

```bash
cargo test -p origin-mcp --test space_roundtrip_e2e
```
Expected: both PASS.

- [ ] **Step 3: Commit**

```bash
git add crates/origin-mcp/tests/space_roundtrip_e2e.rs
git commit -m "test(mcp): e2e capture+recall round-trip honors space; legacy domain alias works" --no-verify
```

**Done when:** MCP round-trip tests pass including legacy alias.

---

### Task 16: /api/search doc-path e2e

**Files:**
- Create: `crates/origin-server/tests/search_docpath_space_e2e.rs`

- [ ] **Step 1: Write the test**

If documents don't have a `space` column (verify with `rg -n 'CREATE TABLE.*documents' crates/origin-core/src/db.rs`), this test should assert that passing `space` to `/api/search` with `source_filter != "memory"` is a no-op (filter ignored, all docs return) and document the wire-gap as Out of Scope.

```rust
#[tokio::test]
async fn doc_search_with_space_no_ops_until_doc_space_column_added() {
    let harness = TestDaemon::spawn().await;
    harness.ingest_doc("doc1 content").await;
    let resp = harness.search(json!({
        "query": "doc1",
        "source_filter": "document",
        "space": "alpha"
    })).await;
    // Doc filter is currently a no-op — see PR-C for documents.space column.
    let results = resp.get("results").and_then(|v| v.as_array()).unwrap();
    assert!(!results.is_empty(), "doc-path space filter currently no-op; remove this test after PR-C adds documents.space column");
}
```

If documents DO have a space column (post-discovery), the test asserts true filter behavior.

- [ ] **Step 2: Run + commit**

```bash
cargo test -p origin-server --test search_docpath_space_e2e
git add crates/origin-server/tests/search_docpath_space_e2e.rs
git commit -m "test(server): document doc-path space behavior (currently no-op, see PR-C)" --no-verify
```

**Done when:** test asserts current truth and documents future work.

---

### Task 17: list_pages_by_space e2e

**Files:**
- Create: `crates/origin-server/tests/list_pages_by_space_e2e.rs`

- [ ] **Step 1: Write the test**

```rust
#[tokio::test]
async fn list_pages_filters_by_space() {
    let harness = TestDaemon::spawn().await;
    harness.create_page("page alpha title", Some("alpha")).await;
    harness.create_page("page beta title", Some("beta")).await;

    let resp = harness.list_pages(json!({"space": "alpha"})).await;
    let titles: Vec<&str> = resp.get("pages").and_then(|v| v.as_array()).unwrap()
        .iter().filter_map(|p| p["title"].as_str()).collect();

    assert!(titles.iter().any(|t| t.contains("alpha")));
    assert!(!titles.iter().any(|t| t.contains("beta")));
}
```

- [ ] **Step 2: Run + commit**

```bash
cargo test -p origin-server --test list_pages_by_space_e2e
git add crates/origin-server/tests/list_pages_by_space_e2e.rs
git commit -m "test(server): e2e list_pages filters by space" --no-verify
```

**Done when:** list_pages filter test passes.

---

## Version bumps + release notes

### Task 18: Bump origin-types + origin-mcp + workspace versions

**Files:**
- Modify: `crates/origin-types/Cargo.toml` (version field — bump per release-please marker)
- Modify: `crates/origin-mcp/Cargo.toml` (version + npm package.json if present at `crates/origin-mcp/package.json`)
- Modify: `crates/origin-server/Cargo.toml`, `crates/origin-cli/Cargo.toml`, `crates/origin-core/Cargo.toml` if they have the `# x-release-please-version` marker
- Modify: `version.txt` (workspace-level marker if present)
- Modify: `.release-please-manifest.json`

- [ ] **Step 1: Identify current versions**

```bash
cat version.txt 2>/dev/null
cat .release-please-manifest.json
rg -n '# x-release-please-version' crates/*/Cargo.toml
```

- [ ] **Step 2: Choose new version**

Pre-1.0 → minor bump (BREAKING change but pre-1.0 capped at minor per AGENTS.md release-please section). Current daemon = 0.6.x → 0.7.0 across all version-marked files.

- [ ] **Step 3: Update all version-marked files**

For each file with `# x-release-please-version` comment, change the version field to `0.7.0`. Update `.release-please-manifest.json`:

```json
{".": "0.7.0"}
```

(or whichever shape that file uses — match exactly.)

- [ ] **Step 4: Verify all in sync**

```bash
rg -n '0\.7\.0|0\.6\.' crates/*/Cargo.toml version.txt .release-please-manifest.json
```
Expected: every version-marked file shows 0.7.0, no leftover 0.6.x.

- [ ] **Step 5: Commit**

```bash
git add crates/*/Cargo.toml version.txt .release-please-manifest.json
git commit -m "chore: bump workspace to 0.7.0 (BREAKING — domain → space rename)" --no-verify
```

**Done when:** all version files agree at 0.7.0.

---

## Final smoke + adversarial review

### Task 19: Smoke test the daemon end-to-end

**Files:** none — runtime verification

- [ ] **Step 1: Build daemon from worktree**

```bash
cd /Users/lucian/Repos/origin/.worktrees/feature/space-rename
cargo build -p origin-server
```
Expected: clean.

- [ ] **Step 2: Run daemon on isolated port + tmp data dir**

```bash
ORIGIN_PORT=7898 ORIGIN_DATA_DIR=/tmp/origin-pra-smoke ./target/debug/origin-server &
DAEMON_PID=$!
sleep 3
```

- [ ] **Step 3: Hit /api/health and verify alive**

```bash
curl -fsS http://127.0.0.1:7898/api/health
```
Expected: `{"status":"ok","db_initialized":true,"version":"0.7.0"}` (or similar).

- [ ] **Step 4: Store + recall with `space` (new field)**

```bash
curl -fsS -X POST http://127.0.0.1:7898/api/memory/store -H 'Content-Type: application/json' \
  -d '{"content":"smoke alpha fact","space":"alpha"}'
curl -fsS -X POST http://127.0.0.1:7898/api/memory/store -H 'Content-Type: application/json' \
  -d '{"content":"smoke beta fact","space":"beta"}'
curl -fsS -X POST http://127.0.0.1:7898/api/search -H 'Content-Type: application/json' \
  -d '{"query":"fact","source_filter":"memory","space":"alpha"}' | jq '.results[].content'
```
Expected: only `"smoke alpha fact"`, not `"smoke beta fact"`.

- [ ] **Step 5: Store + recall with legacy `domain` (back-compat alias)**

```bash
curl -fsS -X POST http://127.0.0.1:7898/api/memory/store -H 'Content-Type: application/json' \
  -d '{"content":"legacy fact gamma","domain":"gamma"}'
curl -fsS -X POST http://127.0.0.1:7898/api/search -H 'Content-Type: application/json' \
  -d '{"query":"legacy","source_filter":"memory","domain":"gamma"}' | jq '.results[].content'
```
Expected: `"legacy fact gamma"`. Confirms `#[serde(alias = "domain")]` works.

- [ ] **Step 6: Migration replay smoke (already isolated tmp dir)**

Daemon should have run migration 50 on startup if `/tmp/origin-pra-smoke/memorydb/origin_memory.db` was fresh. Verify:

```bash
sqlite3 /tmp/origin-pra-smoke/memorydb/origin_memory.db "PRAGMA user_version; .schema memories" | head -20
```
Expected: `user_version` ≥ 50, schema shows `space` column not `domain`.

- [ ] **Step 7: Teardown**

```bash
kill -9 $DAEMON_PID
lsof -ti :7898 || echo "port free"
rm -rf /tmp/origin-pra-smoke
```

- [ ] **Step 8: Document smoke result in plan**

Append to `## Execution log` section at bottom of this plan file: smoke pass/fail + observations.

- [ ] **Step 9: Commit smoke log only (no code)**

```bash
git add docs/superpowers/plans/2026-05-14-pr-a-domain-space-rename.md
git commit -m "docs: PR-A smoke test results" --no-verify
```

**Done when:** smoke passes both new `space` param and legacy `domain` alias path.

---

### Task 20: Adversarial fresh-eye review of the integrated diff

**Files:** none — review only

- [ ] **Step 1: Dispatch fresh subagent**

Per CLAUDE.md "Code review before merge" rule: dispatch a fresh-eye adversarial subagent (Opus) to critique the integrated branch diff. Frame prompt explicitly adversarial.

Prompt template:

> Review the diff on branch `feature/space-rename` against `main`. Goal: rename DB column `domain` → `space` workspace-wide, add `#[serde(alias = "domain")]` back-compat, wire e2e space filter, ship as 0.7.0 BREAKING. Attack the diff. Look for:
> - Missed `domain` references that should have been renamed (especially in test fixtures asserting daemon output shape).
> - Historic migration regions that got accidentally renamed (would break fresh DB replay).
> - LLM prompt fixtures that got swept into the rename (would silently degrade eval quality).
> - serde alias attribute placed on wrong field or struct.
> - Integration tests that pass via stale cached state rather than real space filtering.
> - Version-bump files left out of sync.
> Output: prioritized BLOCKER/MAJOR/MINOR list under 600 words.

- [ ] **Step 2: Triage findings**

For each BLOCKER: fix immediately, commit. For each MAJOR: fix or document deferral. For each MINOR: judgement call.

- [ ] **Step 3: Re-run workspace tests after fixes**

```bash
cargo test --workspace --lib
```
Expected: green.

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "fix: address adversarial-review findings on PR-A" --no-verify
```

**Done when:** adversarial findings triaged + fixes committed.

---

### Task 21: Open PR

**Files:** none — git/gh

- [ ] **Step 1: Verify branch state**

```bash
git -C /Users/lucian/Repos/origin/.worktrees/feature/space-rename status
git -C /Users/lucian/Repos/origin/.worktrees/feature/space-rename log --oneline main..HEAD
```
Expected: clean tree, ~20 commits ahead of main.

- [ ] **Step 2: Push branch (ASK USER FIRST if during work hours per CLAUDE.md)**

```bash
# Confirm time:
TZ=America/Los_Angeles date '+%a %H:%M %Z'
```

If Mon-Fri 09:00-17:00 PT, **stop and ask user** before pushing. Otherwise auto-push allowed per CLAUDE.md off-hours rule.

```bash
git -C /Users/lucian/Repos/origin/.worktrees/feature/space-rename push -u origin feature/space-rename
```

- [ ] **Step 3: Open PR via gh**

```bash
gh pr create \
  --title "feat: rename domain → space + complete e2e scoping (BREAKING CHANGE)" \
  --body "$(cat <<'EOF'
## Summary

Renames the user-facing scoping concept from `domain` to `space` across the workspace. Wires space filter end-to-end on retrieval paths. Ships as 0.7.0 (BREAKING pre-1.0 minor bump). One-release `#[serde(alias = "domain")]` shim preserves cached MCP clients.

## Scope

- DB migration 50: `ALTER TABLE memories RENAME COLUMN domain TO space` (and entities, pages) + reindex.
- Rust rename across 5 crates (~700 sites manual walk; historic migration bodies preserved verbatim).
- Wire-gap fixes: `db.search` doc-path stub + acknowledgement, dead branches removed, `handle_context` filter extended to all shelves.
- E2E tests: SQL filter, MCP round-trip (including legacy domain alias), handle_context, list_pages_by_space, doc-path documentation.
- Version bump 0.6.x → 0.7.0.

## Out of scope (follow-up PRs)

- **PR-B**: drop dead `SpaceStore` + 4 unused tables (informed by Task 1 audit).
- **PR-C**: `/api/spaces` list endpoint + `list_spaces` MCP tool + `documents.space` column.
- **PR-D**: rule-based auto-assign (optional, only if SpaceStore retained).

## Test plan

- [x] Migration 50 replay test (rolls user_version back, re-runs, verifies column rename + data preserved)
- [x] E2E SQL filter test (capture A+B, recall space=A excludes B)
- [x] E2E MCP round-trip + legacy domain alias
- [x] E2E handle_context filters all shelves (not just identity/preference/decision)
- [x] E2E list_pages_by_space
- [x] Workspace cargo check, clippy, lib tests green
- [x] Manual daemon smoke on isolated port — store + recall with both `space` and legacy `domain` keys
- [x] Adversarial fresh-eye review on integrated diff

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 4: Report PR URL**

Print the PR URL for the user.

**Done when:** PR opened on GitHub.

---

## Decisions made during execution

### Task 2: `memory.space` ↔ `Space.name` join contract

**Decision:** Option A — `memories.space` is a free-form string that doubles as both ID and display label. `auto_create_space_if_needed` continues to write a UUID `id` with `name = <space_string>` to the `MemoryDB.spaces` table. No FK constraint added.

**Rationale:**
- The join `memories.domain = spaces.name` is sound: `spaces.name` has a `UNIQUE` constraint (line 821 in db.rs), and `auto_create_space_if_needed` uses `INSERT OR IGNORE` — the UNIQUE constraint is exactly what makes idempotent upserts safe, with no collision risk from free-form strings.
- Task 1 audit confirmed two spaces systems coexist (`MemoryDB.spaces` SQL-backed, `SpaceStore` mostly-dead), but the join is entirely within `MemoryDB.spaces` — `SpaceStore` is not involved. Adding an FK would only complicate the already-working path without fixing any live bug.
- PR-A's risk budget is fully consumed by the column rename across ~20 call sites. A schema normalization (Option B) is the right shape for PR-B (SpaceStore consolidation), when the spaces table will already be restructured.

**Evidence (file:line refs):**
- `crates/origin-core/src/db.rs:819-826` — `spaces` DDL: `id TEXT PRIMARY KEY`, `name TEXT NOT NULL UNIQUE`.
- `crates/origin-core/src/db.rs:5148-5163` — `auto_create_space_if_needed` body: `INSERT OR IGNORE INTO spaces (id, name, ...) VALUES (?1, ?2, ...)` with `id = uuid`, `name = domain`.
- `crates/origin-core/src/db.rs:4783-4788` — `list_spaces` subquery join: `memories.domain = s.name` and `entities.domain = s.name`.
- `crates/origin-server/src/memory_routes.rs:2369,2378` — `handle_set_document_space` calls `db.update_domain(&source_id, &req.space_name)` — free-string write path confirming `space_name` is written directly to the domain column.

**Trade-off recorded:** Option B is the cleaner long-term shape (FK boundary, normalized display name). Deferred because (a) zero current bug pressures it — UNIQUE on `name` + `INSERT OR IGNORE` is already safe, (b) PR-B (drop SpaceStore) is the natural moment to normalize spaces schema, (c) PR-A's risk budget is consumed by the rename itself.

---

## Execution log

<!-- Append smoke results, adversarial findings summary, and any deviation notes here. -->
