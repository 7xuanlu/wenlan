# Ambient Enrichment Scheduler Implementation Plan

Status: Synced to `origin/main` at `4fd6df7c`; bounded automatic implementation and foreground resource protection are locally verified. The branch is not release-ready while RB-01 and RB-06 remain open. RB-05's workflow contract is corrected locally; its Linux/Windows runner execution remains a normal required-CI gate.

> Scope: The conservative scheduler plus approved fixed memory stages. Store and imports perform durable handoff only; document, classification, structured extraction, entity, title, Page growth, reconcile, and citation share ambient admission. Automatic maintenance uses bounded stage slices. Automatic steep is fail-closed to the one bounded ReDistill Page slice; legacy global phases remain available only through explicit foreground steep. No semantic batching, generalized dependency graph, or OS thermal API.

## Contract

- The scheduler owns document, classification, structured extraction, entity, title, Page growth, reconcile, and citation lanes.
- One ambient turn selects at most one durable work item and performs at most one LLM request.
- One admitted automatic turn selects at most one bounded item; every provider role used by that turn shares one hard one-inference budget.
- Ambient turns never overlap, never detach, and do not start during the quiet window, cooldown, elevated system CPU, or low available memory.
- Heavy automatic work requires two consecutive 30-second system-idle samples. The conservative pre-calibration gate is aggregate CPU at or below 20% and available RAM at or above both 2 GiB and 15% of total memory.
- Every selected item either advances a durable checkpoint or records a terminal/retry outcome.
- The working tree uses the existing ten-minute quiet horizon as a conservative cooldown floor. A long turn extends recovery to at least 19 times its own duration; no runtime calculation can shorten the ten-minute floor. A production release value is frozen only after a local before/after profile.

The automatic store/import enrichment path is inside the bounded controller. Automatic maintenance runs one cursor-backed bounded stage slice. Automatic steep cannot enter Decay, Promote, Recaps, Reweave, Reembed, EntityExtraction, CommunityDetection, Detect, Emergence, SummaryRollup, Overview, RefinementQueue, DecisionLogs, PruneRejections, Evict, or KgRethink; those legacy whole-corpus algorithms are explicit foreground work until they independently earn a bound and convergence proof. ReDistill admits one Page, rejects more than 64 sources before provider invocation, caps each source to 800 characters, caps topic/title-hint text, and advances its durable cursor on an oversized Page.

## Decisions, compressed

Everything below is an engineering correction and does **not** need a product decision: bounded graceful shutdown, panic isolation around provider availability, invalidating stale entity links on memory edits, scheduling maintenance as its own durable turn, citation retry accounting, reconcile backoff, and updating the ingest-parity rule.

The source-policy decision is closed: automatic work uses a hard pin. No pin means paused; a present but unavailable pin also means paused; neither state falls back to Anthropic, external, or on-device. A healthy explicit pin authorizes deferred work but never an inference in the store request path. Cross-surface consent/onboarding remains separate release work (RB-06), not a reason to weaken this runtime boundary.

The numeric thermal/convergence target is a later evidence approval, not a decision to guess now. First collect supported-hardware measurements; then approve the measured duty cycle only if the user cannot perceive fan/heat and backlog convergence remains acceptable.

## Release blocker register

Every row marked `OPEN` blocks a thermal-ready release. A blocker may move to `CLOSED` only when its exit criteria and evidence address are filled; implementation intent is not evidence.

| ID | Status | Blocker | Why it was not completed in Phase 1 | Exit criteria | Evidence |
|---|---|---|---|---|---|
| RB-01 | OPEN | Target-hardware thermal and convergence envelope | The ten-minute floor, 20% CPU/2 GiB+15% RAM admission gate, and 5% provisional maximum duty-cycle are protective fail-closed defaults, not measured target-hardware values. Guessing a faster envelope could restore fan/heat; keeping these values may stretch a full backlog over days. | Profile representative document/entity/reconcile/citation slices on supported hardware; validate or revise CPU/pressure, request duration, duty-cycle, and maximum convergence targets; retain policy tests for the approved values. | Protective policy tests are local; target-hardware evidence pending |
| RB-02 | CLOSED (automatic path) | Legacy global steep phases were not item-sliced | A one-phase wrapper was insufficient because every non-ReDistill phase still executed its full legacy algorithm. The release seam is therefore an explicit automatic allowlist, not pretending every phase is bounded. | Automatic scheduling can invoke only phases with a source-proven per-turn bound and durable cursor; foreground trigger semantics stay unchanged; unsupported BurstEnd/Daily work cannot create an empty-batch panic or hot-loop. | `crates/wenlan-server/src/scheduler.rs`; scheduler module 56/56; ReDistill cap tests |
| RB-03 | CLOSED | Accepted reconcile revisions did not converge through fixed enrichment dependencies | Acceptance stays SQL-only and invalidates the fixed receipts for the new head version. The existing ambient lanes then classify, extract structured fields, resolve entity/title/Page work over later turns under the shared budget. | Prove accepted revision priority and eventual receipt convergence without nested acceptance inference. | focused accepted-revision convergence test; `crates/wenlan-core/src/post_write.rs`, `db.rs`, `reconcile.rs` |
| RB-04 | CLOSED (live thermal proof remains RB-01) | Graceful shutdown and non-destructive `background off` | Shutdown signals the scheduler, bounds process drain at 1.5 seconds, and preserves launchd/systemd/Task Scheduler registration. If a registered daemon is still starting or respawning and its port refuses connections, CLI stop now stops the supervisor instead of falsely reporting success. Signal handlers are installed before bind or durable startup work, so a SIGTERM arriving during startup cannot take the default abrupt-exit path. | Code-level and child-process tests prove cooperative stop, bounded process exit, startup-signal handling, registered-manager fallback, and preserved registration semantics. | `crates/wenlan-server/src/lifecycle.rs`; `crates/wenlan-server/tests/graceful_shutdown.rs` (4/4); `background_off_` CLI integration group (7/7) |
| RB-05 | CLOSED (runner execution pending) | Required CI asserted destructive `background off` behavior | The workflow contradicted the supported stop-but-keep-registration contract. User approval was required before changing required CI. | Linux/Windows assertions verify stopped-but-registered; a separate test-only cleanup always removes the unit/task. | `.github/workflows/ci.yml`; YAML parse and Linux shell syntax pass locally; Windows runner is the remaining execution gate |
| RB-06 | OPEN — separate session | Cross-surface source consent/onboarding is not yet consistent | Runtime fails closed on every surface, but CLI/plugin/daemon cannot yet give an app-less user one disclosed flow to select and verify the everyday/synthesis pins. | Implement the separate approved onboarding spec so every surface discloses automatic work, requires consent, writes the same pins, and reports paused/unavailable state consistently. | `/Users/lucian/Documents/Codex/2026-07-13/w/cross-surface-model-consent-session-prompt.md` |

Verification risk, tracked but not currently classified as a product release blocker: the unbounded full-workspace verifier itself drove the `wenlan_core` test binary to roughly 535–570% CPU for more than a minute and was interrupted after about 4.5 minutes to avoid recreating the user-facing thermal failure. No assertion failed before interruption. Release evidence uses serialized, bounded module/package gates; required CI still needs its ordinary complete runner execution.

## Review integration

- Claude first proposed a per-request semaphore, then its implementation review correctly identified serialization as redundant once every ambient job was awaited inline by the single poll loop. The final provider facade has a different purpose: it hard-caps each ambient slice at one forwarded inference and fails closed on any hidden nested call; the same counter feeds telemetry and cooldown decisions.
- Fable rejected semaphore-only serialization because a large document could immediately reacquire it and sustain load. The implementation therefore returns to the scheduler after every request.
- Fable correctly identified fresh-document preparation as a separate CPU risk. Source verification narrowed the claim: embedding is one batch call, not one call per chunk, and directory ingestion already caps text files at 1 MB and PDFs at 10 MB. It is still variable-cost work, so a selected fresh document consumes the same cooldown even if it made no LLM request.
- Store-time and chat-import enrichment no longer spawn the canonical multi-stage pipeline. They persist origin/backlog state and return; the ambient lanes own subsequent inference.
- The ten-minute cooldown in the implementation is an explicitly conservative hotfix borrowed from the existing quiet horizon, not a measured release constant.
- Independent correctness review found and the implementation now covers same-poll activity races, document queue starvation, reconcile frontier starvation, historical no-provider entity rows, and non-atomic document checkpoints.
- The final Opus follow-up found two additional silent-stall paths: an unavailable configured API prevented fallback to an available local provider, and a panic inside an ambient slice terminated the scheduler task. Both were fixed RED-first; provider selection now chooses the first available backend, and ambient panics are isolated while still charging a conservative thermal cooldown. Focused recovery/fallback tests cover both paths.
- A later live audit found the larger automatic path: the old installed daemon repeatedly ran whole Idle steeps for roughly 10–19 minutes, including 1,138,342 ms and 771,001 ms runs with 682 pending. Source audit then proved that a one-phase wrapper was still unsafe: every phase except ReDistill fell through to a whole legacy algorithm. The corrective design is an automatic safe-phase allowlist, not more timeout/semaphore layers. Only ReDistill is eligible; maintenance uses its separate bounded stage round.
- BurstEnd and Daily currently have no allowlisted steep phase. Mature BurstEnd timestamps are drained as unsupported bookkeeping so they cannot grow forever or repeatedly win selection; this path does not charge a thermal cooldown. Idle and Backstop may admit one ReDistill Page. A maintenance panic remains isolated at the scheduler boundary, and an actually attempted bounded turn starts cooldown.
- The final full working-tree Opus review rejected release: provider `is_available()` calls still escape the panic boundary, edited memories can retain stale entity links, steep-first shared budgeting can starve maintenance, and citation provider errors do not advance retry state. Its reconcile "every poll" wording was overstated because round-robin advances the cursor, but reconcile backpressure still lacks the intended backoff.
- Mainline synchronization now rests at `4fd6df7c`. The ambient lanes and store response use the same hard-pin resolver. Chat import keeps durable handoff and contains no detached enrichment task; generic import now records automatic-owned origin atomically with its memory rows.
- Claude's first follow-up incorrectly claimed revision acceptance already runs canonical enrichment; source verification showed acceptance is SQL-only. The final fixed-stage design keeps acceptance fast, invalidates versioned receipts, gives accepted revisions priority, and lets later ambient classification/extraction/entity/title/Page lanes converge without nested acceptance inference.
- Fable's final adversarial review rated the branch `needs-attention`, primarily because the unmeasured ten-minute fallback can make full first-boot backlogs converge over days. That remains a release-calibration blocker, not a reason to guess a faster constant. Its valid code-level finding — a spent reconcile slice still reading the second frontier — was fixed with a RED metadata-side-effect test and an early budget return. Claims that citation repeated the same lane every 30 seconds and that synthesis/external providers bypassed `run_ambient_job` were rejected after source verification: the cursor advances, and those provider handles are not passed into the ambient function.
- The final standard Opus review found five release-blocking gaps after the earlier green suite. CI still encoded destructive stop semantics; a Document provider panic leaked the claimed row as `in_progress`; Maintenance `StalePage` bypassed the 64-source automatic cap; numeric stale-page OFFSET cursors skipped rows when a successful refresh shrank the set; and zero-call reconcile watermark progress was mislabeled as an empty lane. Each finding was reproduced or source-proven, then fixed. Stale-page scans now use a v2 composite keyset plus an additive supporting index; reconcile progress consumes the global thermal turn without receiving the 30-minute empty-lane delay.
- The same review's round-robin cleanup is deferred because only ReDistill is allowlisted and the current metadata write is not a thermal defect. Its citation-counter duplication claim was rejected: citation attempts are Page-version state, while `enrichment_steps` is Memory-version state. Follow-up shutdown audits found and fixed both the registered-but-not-yet-reachable `background off` false-success path and the startup SIGTERM registration race. Medium cleanup remains for simultaneous timer/shutdown readiness and detached/admission task accounting; none escapes the 1.5-second process-exit bound, so they remain follow-up rather than thermal release blockers.
- A post-fix lock audit found five `handle_store_memory` stages that could retain a `ServerState` read guard across a DB await: dedup, agent gating, entity resolution, activity attribution, and activity logging. A five-stage behavioral regression first named each still-blocked stage, then all five paths were changed to clone owned DB/provider Arcs inside a short read scope. The final exhaustive Opus re-audit enumerated every state read in the handler and returned `SHIP` with no blocking findings. The synchronous per-store config read remains deliberate until RB-06 defines cross-process config ownership; caching it now could make a CLI/plugin pin or consent change stale inside the daemon.
- Latest main independently occupied migrations 70-72 for vocabulary healing while this feature had used 70-73 for enrichment receipts/provenance. The integrated lineage preserves main's 70-72, moves ambient migrations to 73-76, and adds an idempotent migration 77 reconciliation. RED/GREEN tests start from both main-72 and feature-73 layouts so neither existing lineage silently skips the other schema.
- Wenlan inactivity alone was insufficient foreground protection because unrelated compilation or model work is invisible to the write signal. The scheduler now samples whole-system CPU and available RAM through the pinned `sysinfo` dependency, requires two quiet samples, and fails closed for every heavy automatic/ambient admission while the machine is busy or memory-constrained.
- Post-merge review found relation vocabulary healing committed destructive collision folding before its recovery ledger. A forced-ledger-failure regression reproduced the provenance loss; relation mutation and pre-image ledger insertion now share one transaction, matching entity folding's atomic boundary.

## Why a semaphore alone is insufficient

`Semaphore(1)` prevents concurrent requests but still permits a large document to release and immediately reacquire the permit for every chunk. That can keep CPU/GPU utilization continuous and preserve the fan/heat failure. The unit of scheduling must therefore be one request followed by a return to the scheduler, not one end-to-end job guarded by a semaphore.

## Task 1: Make entity backfill a durable one-item slice

**Files:**

- Modify: `crates/wenlan-core/src/db.rs`

**RED tests:**

1. A database with five eligible rows invokes the extractor once and returns after one selected row.
2. An empty entity result records terminal `entity_extract = ok` and is not selected again; `skipped` remains a compatibility input for rows created when no provider was available.
3. A provider/parser error records `needs_retry`; the configured attempt cap records `abandoned` and prevents another selection.
4. A valid extraction records `ok`, links the entity, and is not selected again.

**Implementation:**

- Replace the drain-until-partial-batch behavior with `run_entity_enrichment_slice`.
- Select one candidate by joining `enrichment_steps`; allow fresh rows and retryable rows below the attempt cap.
- Stop recording a terminal entity-extract step when ingestion has no provider; retain compatibility for historical `skipped` rows without charging that old skip against the actual retry cap.
- Reuse `record_enrichment_step`; add no table or migration.
- Return a small report containing whether an item was selected and whether one LLM call was attempted.

## Task 2: Add an ambient one-request document slice without breaking foreground callers

**Files:**

- Modify: `crates/wenlan-core/src/db.rs`
- Modify: `crates/wenlan-core/src/document_enrichment.rs`

**RED tests:**

1. A multi-chunk document processes exactly one chunk, checkpoints it, returns, and leaves the queue claimable.
2. Repeated slices resume at the next chunk without repeating prior calls.
3. After the final chunk, the next slice performs only entity extraction and completes the document.
4. A failed request preserves the checkpoint and follows the existing pause/backoff path.
5. Summary persistence and the queue resume point commit atomically.
6. A yielded long document moves behind other pending documents.

**Implementation:**

- Add a queue transition that yields `in_progress` back to `pending` without incrementing the retry counter or discarding `last_completed_chunk`.
- Factor the existing pipeline behind a request budget.
- Keep `run_document_enrichment` as the end-to-end compatibility wrapper.
- Add `run_document_enrichment_slice` with a budget of one request for the ambient scheduler.
- After consuming the request budget, persist/yield and return before another LLM call. The existing last-chunk checkpoint is sufficient to make entity extraction a separate next slice.
- Persist each per-chunk summary and its resume index in one transaction; persistence failure pauses instead of yielding.
- Move a yielded row to the durable queue tail so one large file cannot monopolize every thermal turn.

## Task 3: Expose one-request reconcile and citation slices

**Files:**

- Modify: `crates/wenlan-core/src/reconcile.rs`
- Modify: `crates/wenlan-core/src/citations.rs`

**RED tests:**

1. Reconcile with multiple eligible candidates performs at most one judge call and preserves its watermark for the next slice.
2. Citation backfill with multiple eligible pages performs at most one annotate call.
3. A citation page with no evidence can become terminal without an LLM call, but the slice still returns after that selected page.
4. Reconcile alternates its persisted starting frontier so a steady document backlog cannot starve captures.
5. A judge-created revision does not trigger hidden canonical-enrichment calls in the same ambient slice.

**Implementation:**

- Keep the existing batch/tick entry points for explicit callers and current tests.
- Add ambient wrappers that pass a request/page budget of one through the same implementation.
- Do not add another queue: existing reconcile watermarks and citation attempt metadata remain authoritative.
- Stage ambient revisions with no nested provider. This preserves the hard thermal bound but leaves full accepted-revision classification as an explicit Phase-2 gap rather than silently breaking the one-call contract.

## Task 4: Split cheap directory sync from ambient document work

**Files:**

- Modify: `crates/wenlan-server/src/scheduler.rs`

**RED tests:**

1. Directory mtime/hash synchronization still runs every poll without claiming document work.
2. The document slice claims at most one queue item when selected by the ambient scheduler.

**Implementation:**

- Split `run_directory_sync_tick` into cheap source synchronization and one ambient document slice.
- Keep filesystem/database freshness outside the LLM gate.
- Remove `MAX_DOC_ENRICH_PER_POLL` and the document drain.

## Task 5: Add the private round-robin ambient controller

**Files:**

- Modify: `crates/wenlan-server/src/scheduler.rs`

**RED tests:**

1. Recent activity prevents every ambient job from starting.
2. Cooldown prevents another turn after a completed request.
3. When all eight jobs are due, eight eligible turns select document, classification, structured extraction, entity, title, Page growth, reconcile, and citation once each.
4. A job with no work does not starve later jobs.
5. Core slice tests prove one request per invocation, and the scheduler source contains no detached ambient job dispatch; the single poll loop awaits each invocation inline.
6. No periodic sweep uses `tokio::spawn` after the change.
7. The ambient provider facade forwards one request and rejects a second, so future nested calls fail closed in production as well as tests.
8. A lane that selected backlog remains due after the global cooldown; only an empty lane receives the 30-minute rescan backoff.

**Implementation:**

- Add a private closed `AmbientJob` enum and state containing the round-robin cursor, due times, and `next_allowed_at`.
- After cheap sync and normal trigger handling, check the quiet horizon and cooldown, select one due job, await one slice inline, record duration/outcome, advance the cursor, and return to the poll loop.
- Remove detached entity/reconcile/citation sweep spawns.
- Emit compact telemetry: job, selected/progressed, LLM calls, elapsed milliseconds, attempt/outcome where available, and next eligible time.
- Re-read activity immediately before Idle and ambient quiet gates so a write that arrives during filesystem sync blocks new background work.
- Keep known backlog governed by the single global cooldown and round-robin cursor; use the 30-minute lane timestamps only to avoid repeatedly scanning an empty lane.
- Admit steep and maintenance through the same global quiet/cooldown policy, with one trigger and one shared inference budget per turn. Do not represent that wrapper as item-level boundedness; keep RB-02 open until the inner loops are sliced or proven.

## Task 6: Profile and freeze the first production envelope

**Files:**

- Modify: `crates/wenlan-server/src/scheduler.rs`
- Update: this plan with the evidence command and measured values

**Verification:**

1. Record a pre-change single-request wall time and system pressure/CPU sample on the current supported Mac.
2. Run the same fixture through the one-request slice.
3. Choose the initial target duty cycle and minimum cooldown from that evidence, favoring slower convergence over perceptible fan/heat.
4. Add unit tests for the duration-to-cooldown calculation with an injected policy and paused Tokio time.

Protective code is complete: the injected policy keeps a ten-minute floor, extends recovery after long turns, and adds a two-sample CPU/RAM admission gate. Those values deliberately reduce risk without claiming calibration.

The controller is not declared thermally ready until the remaining live numeric gate is filled with measured evidence. If reliable thermal sensors require privileges unavailable in the harness, keep the conservative policy and surface the missing profile as a release blocker rather than presenting it as calibration.

## Final verification

- `cargo fmt --check`
- Focused RED/GREEN tests for each core module and scheduler.
- Bounded `wenlan-core` module/focused regressions; do not use the verifier itself to recreate the thermal incident locally.
- `cargo test -p wenlan-server --lib`
- `cargo clippy -p wenlan-core -p wenlan-server --all-targets -- -D warnings`
- Independent correctness/concurrency review.
- Opus architecture/code review; the earlier explicitly requested Fable adversarial UX/thermal review remains supporting historical evidence, not a standing review requirement.
- Live daemon proof: no overlapping ambient request, cooldown visible in logs, backlog advances across turns, and stop/start CLI behavior remains unchanged.

Latest code evidence (2026-07-18):

- Latest remote `origin/main` resolves to `4fd6df7c879bb636750ddc5459c9d76187fa065f` and is integrated locally.
- Dual-lineage migration RED: main-72 lacked `enrichment_steps.input_version`; feature-73 lacked `entity_type_vocabulary`. GREEN: migration 73-77 group 4/4 plus lineage group 2/2.
- Foreground resource-policy RED then GREEN: high CPU/low memory admission and two-sample recovery 2/2; duration-aware cooldown 1/1; full scheduler module 61/61 in 3.86 seconds.
- Relation vocabulary atomicity RED then GREEN: a forced ledger failure first left only the mutated canonical edge, then preserved both original relation rows after the transaction fix; full `kg_quality` regression group 18/18.
- RED then GREEN: generic import writes automatic-owned enrichment origin atomically — 1/1.
- RED then GREEN: automatic batch contains only ReDistill and its cursor never leaves the allowlist; full scheduler module — 56/56 in 5.15 seconds.
- ReDistill regression group — 6/6, including one-Page slicing, 64-source join/legacy-JSON caps, zero provider calls when oversized, retryable cursor reachability, and mutation-safe stale-page keyset traversal.
- Automatic maintenance module — 17/17. The StalePage path now shares the automatic 64-source cap and mutation-safe composite keyset cursor with ReDistill.
- Accepted-revision fixed-dependency convergence — 1/1.
- Everyday and synthesis hard-pin resolver groups — 5/5 each. Healthy external store route reports `pending` but performs zero inline inference; unconfigured/unavailable reports `paused`.
- RED then GREEN: Document provider panic pauses the exact claimed generation instead of leaking `in_progress`; stale-page success cannot skip a shifted row; zero-call reconcile watermark progress consumes the shared thermal turn without receiving empty-lane backoff.
- RED then GREEN: five-stage store-route lock regression identifies each `ServerState` guard retained across a DB await; all stages now release the guard before waiting, and panic/timeout hook cleanup is RAII-safe — 1/1.
- `wenlan-server --lib` — 220/220 after latest-main integration and resource admission. Graceful-shutdown child-process integration — 4/4, including SIGTERM after bind during startup.
- `wenlan` CLI — 83 passed, 1 ignored; `background off` preserves service registration and stops a registered-but-unreachable supervisor.
- `wenlan-types` paused response round-trip — 1/1.
- `cargo clippy -p wenlan -p wenlan-core -p wenlan-server --all-targets -- -D warnings` — no issues.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- CI lifecycle assertions now keep the Linux unit and Windows task registered after `background off`; YAML parsing and extracted Linux shell syntax pass locally. Windows runner execution remains required CI.
- Lean Dev Tools/rust-analyzer at warning severity — no diagnostics in scheduler, refinery, DB, store route, distill, or importer.
- The full-workspace local test was intentionally interrupted when its verifier sustained roughly 535–570% CPU; no assertion failure had occurred. Required CI remains the complete-suite and Linux/Windows lifecycle execution gate.
- A standard Opus working-tree review found the startup SIGTERM registration race; it was reproduced RED and fixed GREEN. One additional `ReflectionDebouncer` observation is baseline dead code outside this feature diff, not a thermal behavior introduced by this branch. The final file-backed Opus gate returned `SHIP`: no blocking findings in the completed store/scheduler/lifecycle seam.

Not yet proven: live daemon telemetry and target-hardware CPU/fan/thermal measurements. The two-sample CPU/RAM gate and duration-aware ten-minute floor stay conservative protective defaults until those measurements exist.
