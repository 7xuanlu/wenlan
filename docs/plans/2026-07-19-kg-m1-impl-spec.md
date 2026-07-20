# M1 implementation spec — "honest columns"

Companion to `2026-07-19-kg-m1-space-workspace-audit.md` (commit `cd490e43`).
The audit is the evidence; this file is the build order. Every claim here is
already grounded there — re-read the audit before deviating.

Branch `kg-m1-honest-columns`, base `9cb6c6ac`, `SCHEMA_VERSION = 79` → M1 takes
**80**. Re-verify 79 is still current before writing the number; main moved
underneath M0 once already.

## Decisions already made — do not relitigate

| decision | rationale |
|---|---|
| **Scope migrates FROM `workspace` INTO `space`** | GT3. `workspace` is authoritative (m63); `pages.space` is a rename of `domain`. |
| **No `kind` column in M1** | Zero category residue, and `source`/`authored`/`entity`/`overview` already have homes. Deferred to M3. |
| **The fold ledger still ships** | Its job is reversibility of the fold, not the category mapping. It is what makes the rollback contract real. |
| **`workspace` is kept and dual-written** | Dropping it makes the fold irreversible. Out of scope to drop. |
| **Wire types stay `Option<String>`** | The NOT NULL flip is invisible to the app only while they do. See audit §6.7. |
| **`unfiled`, not `uncategorized`** | `uncategorized` collides with the `AmbiguousUncategorized` sentinel (`read_scope.rs:41-53`). |
| **`writable_schema` patch, not a table rebuild** | m67 (`db.rs:7056`) already ruled against rebuilding `pages`. Paired with an in-transaction NULL assertion, since the patch validates nothing. |
| **Overview page scopes to `unfiled`** | It is a global sentinel with no space today (`overview.rs:78-90`). `unfiled` is the least-surprising home; M4 revisits when overviews become per-community. Leave a `ponytail:` comment naming that ceiling. |

## Floor — every increment, no exceptions

- **TDD**: failing test first. **Mutation-prove** each load-bearing test — break
  the product code, watch that exact test fail, restore. Note the mutation you
  used in the commit body.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo fmt --check --all` — green before every commit.
- **Builds must run outside the agent sandbox.** The repo's `.cargo/config.toml`
  wires `sccache` as a shared cross-worktree cache and the sandbox blocks its
  socket (`Operation not permitted`). Do not "fix" this by unsetting
  `RUSTC_WRAPPER` — that triggers a full cold rebuild of llama-cpp.
- Never weaken an existing check to get green. Never touch `main`.
- `SELECT COUNT(*) FROM pages` **returns 0** — the libSQL vector-index bug. Use
  `EXISTS(SELECT 1 …)` or enumerate rows. This applies inside migration
  assertions too, where a false 0 would read as "no NULLs remain".

## Increment 1 — migration 80

One migration, one explicit transaction (the m50 / 73-77 precedent for
destructive multi-statement work: `PRAGMA foreign_keys=OFF` + `BEGIN` +
`COMMIT`/`ROLLBACK`). Replay-safe throughout: `IF NOT EXISTS`,
`INSERT OR IGNORE`, and idempotent UPDATEs.

**Order matters — steps 1 and 2 must precede any backfill.**

1. **Mint the `unfiled` space row.** `INSERT OR IGNORE INTO spaces(...)` with a
   **hard-coded UUID literal** so a replay converges on one row. This is
   load-bearing: `registered_space_or_none` (`db.rs:7969-7982`) silently NULLs
   any space with no registry row, so backfilling `space='unfiled'` without this
   reintroduces the exact NULLs M1 removes. Precedent: m12 at `db.rs:3576-3585`.

2. **Create `page_space_fold_ledger`** (`IF NOT EXISTS`):

   ```
   page_id TEXT PRIMARY KEY
   prior_space      TEXT            -- nullable: what it was
   prior_workspace  TEXT            -- nullable: what it was
   assigned_space   TEXT NOT NULL
   rule             TEXT NOT NULL   -- 'workspace' | 'space_residue' | 'unfiled'
   migrated_at      TEXT NOT NULL
   ```

   Populate with `INSERT OR IGNORE` **before** the UPDATE, so a replay after a
   partial run never overwrites a prior_* with an already-folded value. This is
   the rollback artifact: `workspace` alone cannot reverse the fold because the
   backfill overwrites `space`.

3. **Backfill `space`**, precedence exactly:

   ```sql
   space = COALESCE(
     workspace,                                            -- 1. authoritative (GT3)
     CASE WHEN space IN (SELECT name FROM spaces) THEN space END,  -- 2. origin residue
     'unfiled'                                             -- 3.
   )
   ```

   Expected on the reference DB: 132 / 28 / 37 = 197. This resolves all 5
   collisions in `workspace`'s favour, which is also what makes the cascade
   semantics in increment 3 safe.

4. **Backfill `workspace`**: `UPDATE pages SET workspace = space WHERE workspace IS NULL`.
   After step 3 `space` is non-NULL everywhere, so this closes the shadow column.

5. **Batch 3 and 4** by rowid range against a **measured** lock-duration budget
   (§6.3). Measure it — do not guess a batch size — and record the measurement in
   the PR. Each batch commits with its cursor so a kill mid-run resumes rather
   than restarts.

6. **Assert, in-transaction, before the schema patch**:
   `EXISTS(SELECT 1 FROM pages WHERE space IS NULL OR workspace IS NULL)` must be
   false. If true, **ROLLBACK and fail the migration**. The `writable_schema`
   patch validates nothing, so this assertion is the only thing standing between
   a surviving NULL and a column that lies about itself.

7. **Rebuild both indexes** — both are partial today and both predicates go
   vacuous under NOT NULL:
   - `idx_pages_workspace` (`db.rs:6907`)
   - `idx_pages_space` (`db.rs:6347`) — *not* in the original brief

   `DROP INDEX IF EXISTS` then `CREATE INDEX IF NOT EXISTS` without the `WHERE`.

8. **Stamp NOT NULL** on `pages.space` and `pages.workspace` via the m67
   `writable_schema` pattern (`db.rs:7094-7107`): `PRAGMA writable_schema=ON`,
   rewrite the `pages` entry in `sqlite_master.sql`, `PRAGMA writable_schema=RESET`.
   Do **not** rebuild the table — `pages_fts` is contentless with 3 rowid-coupled
   triggers (`db.rs:6070-6086`) and `search_pages` joins `vector_top_k` on
   `c.rowid` (`27928-27929`).

9. `PRAGMA user_version = 80`.

**Tests for increment 1:**
- fresh-DB schema and upgraded-DB schema agree (byte-compare the normalized
  `pages` DDL, both columns NOT NULL)
- backfill precedence: one case per rule — workspace wins over a *different*
  non-NULL space (the collision case), space residue used when workspace NULL,
  `unfiled` when both NULL
- an unregistered space value in `pages.space` routes to `unfiled`, and the
  ledger records `rule='unfiled'` with the original value in `prior_space`
- `registered_space_or_none("unfiled")` returns `Some` after the migration
- NOT NULL is real: inserting a NULL space fails
- **replay-safety**: run the migration, interrupt mid-batch, rerun, converge —
  no duplicate ledger rows, no half-folded column, ledger `prior_*` unchanged
- the assertion fires: seed a row the backfill cannot resolve, confirm the
  migration rolls back rather than stamping NOT NULL over a NULL

## Increment 2 — NULL producers

Three sites; the first two are production.

| site | fix |
|---|---|
| `wenlan-mcp/src/tools.rs:2074` | stop hardcoding `workspace: None`. The server mirror (`memory_routes.rs:2084-2096`) currently rescues this; do not rely on it. |
| `synthesis/overview.rs:78-90` | `ensure_overview_page` hardcodes both `space: None` and `workspace: None`. Assign `unfiled` per the decision table, with a `ponytail:` comment naming M4 as the upgrade path. |
| `db.rs:25676` | `insert_page` wrapper — test-only reachable, fixed for hygiene so it cannot become a production NULL source later. |

Do **not** touch the already-honest sites: `distill.rs:756,759`,
`refinement_queue.rs:299,302`, `eval/shared.rs:2529,2532`. The M0 gate
(`post_write.rs:2585,2603,2628,2633`) passes both columns through verbatim and
invents nothing — leave it alone.

**Test:** each production producer, driven end-to-end, lands a non-NULL scope.
Mutation-prove by reverting the hardcode and watching the test fail.

## Increment 3 — cascades

The double-apply hypothesis in the original brief **does not hold** — each site
is a single UPDATE, so no row is touched twice (audit §6.6). The real work:

1. **Collapse the duplicate SET target** at all three sites — `update_space`
   (`db.rs:8071-8085`), `delete_space` move-branch (`8252-8265`),
   `reassign_memories_space` (`8376-8389`). Post-fold, `space = CASE…` and
   `workspace = CASE…` name the same logical column.
2. **Fix the tautological WHERE** — `space=?2 OR workspace=?2` becomes
   `space=?2 OR space=?2`.
3. **`delete_space` never rescopes pages** (`db.rs:8141-8207`). The
   `keep`/`unassign`/`delete` branches touch `memories` and `entities` only; only
   `move:` reaches pages. A supported user action currently manufactures pages
   scoped to a space with no registry row — illegal under M1's model. Rescope
   them (`unfiled` for the non-move branches).

**Tests:** one per site. Rename applies exactly once and to both logical
positions; merge does not double-bump `version`; `delete_space` leaves **no**
page pointing at a deleted space, in every branch. Include a regression test
seeded with a row shaped like the 5 collisions.

## Increment 4 — backup, integrity receipt, restore drill

Nothing exists to reuse. `operation_receipts` (m79, `db.rs:7433-7440`) is API
idempotency keyed `(caller_id, operation_id)` — a false friend, not migration
integrity. Write this fresh:

- **Pre-migration online backup.** A raw file copy is unsound under WAL. Prefer
  `VACUUM INTO` (verify libSQL supports it on the pinned version before
  committing to it; if not, use the backup API through the libsql crate).
- **Integrity receipt**: `PRAGMA integrity_check` result, schema version before
  and after, row counts (via `EXISTS`/enumeration, not `COUNT(*)`), backup path
  and digest.
- **Restore drill that actually restores** — migrate, restore the backup,
  confirm the restored DB is at the *old* schema version with the *old* column
  nullability and the pre-fold `space` values. A drill that only checks the file
  exists is not a drill.

The newer-schema refusal the spec calls missing **already exists** at
`db.rs:2955-2958`. Cover it with a test if none exists; do not reimplement it.

## Out of scope — stop conditions if they appear necessary

- Dropping `workspace` or the ledger.
- Any `kind` column (M3).
- Removing the dead category reader (`db.rs:27916`, MCP `tools.rs:3127`) — it
  changes the wire contract. Recorded as a follow-up.
- Retyping `Page.space` / `Page.workspace` to `String`.
- Anything in M2–M6.

## Done

The draft PR description **is** the acceptance checklist, every item checked with
evidence: the audit table, test names, gate output, migration logs, the measured
batch budget, and restore-drill output. A pushed branch without the checklist is
not done.
