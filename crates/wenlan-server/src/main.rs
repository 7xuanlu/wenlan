// SPDX-License-Identifier: Apache-2.0
//! Wenlan headless daemon — runs the memory server without Tauri.

mod cmd_backfill;

/// Resolve the bind address. Honors the `WENLAN_BIND_ADDR` env var when set
/// (e.g. inside Docker where the daemon must listen on `0.0.0.0`). Falls back
/// to the localhost-only address used by the macOS/native install path.
fn resolve_bind_addr(port: u16) -> String {
    wenlan_core::env_compat::var_compat("WENLAN_BIND_ADDR")
        .and_then(|v| v.into_string().ok())
        .unwrap_or_else(|| format!("127.0.0.1:{}", port))
}

#[cfg(test)]
mod bind_addr_tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    static TEST_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    fn env_lock() -> &'static Mutex<()> {
        TEST_ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn default_when_env_unset() {
        let _guard = env_lock().lock().unwrap();
        std::env::remove_var("WENLAN_BIND_ADDR");
        assert_eq!(resolve_bind_addr(7878), "127.0.0.1:7878");
    }

    #[test]
    fn honors_env_when_set() {
        let _guard = env_lock().lock().unwrap();
        std::env::set_var("WENLAN_BIND_ADDR", "0.0.0.0:9090");
        assert_eq!(resolve_bind_addr(7878), "0.0.0.0:9090");
        std::env::remove_var("WENLAN_BIND_ADDR");
    }
}

// All other modules live in the library target (src/lib.rs) so that
// integration tests in tests/ can reference them as wenlan_server::<mod>.
use wenlan_server::{
    ingest_batcher, router, scheduler,
    state::{ServerState, SharedState},
};

use clap::{Parser, Subcommand};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Wenlan memory daemon — headless HTTP server.
#[derive(Parser)]
#[command(
    name = "wenlan-server",
    bin_name = "wenlan-server",
    version,
    about = "Wenlan headless HTTP daemon."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Override the data directory (for isolated dev/demo runs).
    /// When set, the daemon reads/writes the DB at `<dir>/memorydb/origin_memory.db`
    /// and config at `<dir>/config.json` instead of the default
    /// the platform data directory under `dirs::data_local_dir().join("wenlan/")`.
    /// macOS: `~/Library/Application Support/wenlan/`. Linux: `~/.local/share/wenlan/`. Windows: `%LOCALAPPDATA%\origin\`. Also honored via `WENLAN_DATA_DIR` env.
    #[arg(long, global = true)]
    data_dir: Option<std::path::PathBuf>,

    /// Override the HTTP port (default 7878). Useful when running a scratch
    /// daemon alongside the main one. Also honored via `WENLAN_PORT` env.
    #[arg(long, global = true)]
    port: Option<u16>,
}

#[derive(Subcommand)]
enum Command {
    /// Internal maintenance: delete archived stale pages. Daemon must be stopped first.
    #[command(name = "backfill-stale-pages", hide = true)]
    BackfillStalePages {
        /// Print candidates without modifying the database.
        #[arg(long)]
        dry_run: bool,
    },
}

async fn run_daemon() -> anyhow::Result<()> {
    // Logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wenlan_core=info,wenlan_server=info".into()),
        )
        .init();

    tracing::info!("wenlan-server v{}", wenlan_core::version());

    // Port (clap `--port`/`WENLAN_PORT` → env var set by main(); read here)
    let port: u16 = wenlan_core::env_compat::var_compat("WENLAN_PORT")
        .and_then(|v| v.into_string().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(7878);

    // Bind BEFORE touching the data dir. Losing the port race must be free:
    // under launchd KeepAlive, a retry loop that first runs full MemoryDB init
    // (schema/FTS writes + embedder load) hammers the live daemon's SQLite
    // file every ~10s — enough lock/CPU pressure to wedge the daemon that
    // actually owns the port.
    let addr = resolve_bind_addr(port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            // Check if existing daemon is healthy
            tracing::warn!("Failed to bind {}: {}", addr, e);
            let url = format!("http://127.0.0.1:{}/api/health", port);
            // Bounded probe: a mute port-holder (accepts, never responds)
            // must not hang this process forever — under launchd KeepAlive
            // a hung loser also blocks the retry that would recover things.
            let probe = reqwest::Client::new()
                .get(&url)
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await;
            match probe {
                Ok(resp) if resp.status().is_success() => {
                    // Port already taken by a healthy daemon. If launchd is the
                    // parent (XPC_SERVICE_NAME set), exit non-zero so launchd
                    // retries after ThrottleInterval — otherwise launchd marks
                    // this attempt as a clean exit and refuses to respawn even
                    // after the winning daemon dies (KeepAlive.SuccessfulExit
                    // = false treats exit-0 as success). For sidecar invocation
                    // by the app, exit 0 is the right answer.
                    if std::env::var_os("XPC_SERVICE_NAME").is_some() {
                        tracing::info!(
                            "Existing healthy daemon on port {} — exiting 75 (launchd retry)",
                            port
                        );
                        std::process::exit(75);
                    }
                    tracing::info!("Existing healthy daemon on port {} — exiting cleanly", port);
                    return Ok(());
                }
                _ => {
                    return Err(anyhow::anyhow!(
                        "Port {} in use and no healthy daemon",
                        port
                    ));
                }
            }
        }
    };

    // One-time origin -> wenlan data migration (default locations only). Runs here,
    // before the DB opens, so the daemon is the sole writer. No-op once migrated.
    if wenlan_core::env_compat::var_compat("WENLAN_DATA_DIR").is_none() {
        if let Some(dl) = dirs::data_local_dir() {
            wenlan_core::migrate_rename::migrate_and_log(&dl.join("origin"), &dl.join("wenlan"));
        }
    }
    if let Some(home) = dirs::home_dir() {
        wenlan_core::migrate_rename::migrate_and_log(&home.join(".origin"), &home.join(".wenlan"));
    }

    // Data directory. `WENLAN_DATA_DIR` (set by `--data-dir` flag) overrides the
    // default, enabling isolated dev/demo runs (e.g. `--data-dir /tmp/wenlan-demo`).
    let wenlan_root = wenlan_core::env_compat::var_compat("WENLAN_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            dirs::data_local_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("wenlan")
        });
    let data_dir = wenlan_root.join("memorydb");
    tracing::info!("Wenlan data root: {}", wenlan_root.display());

    // Build state
    let mut server_state = ServerState::new();

    // Init MemoryDB
    let emitter: Arc<dyn wenlan_core::events::EventEmitter> = Arc::new(wenlan_core::NoopEmitter);
    tracing::info!("Initializing MemoryDB at {}", data_dir.display());
    let db = wenlan_core::db::MemoryDB::new(&data_dir, emitter).await?;
    let db_arc = Arc::new(db);
    server_state.db = Some(db_arc.clone());

    // Run migration-55 backfill (event_date regex Pass A + memory_entities Pass B)
    // before the HTTP listener binds so no ingest races the backfill. Idempotent.
    tracing::info!(
        "Running first-boot data backfill (event dates + knowledge-graph links); \
         this can take a moment on large databases…"
    );
    let m55 = db_arc.run_migration_55().await.map_err(|e| {
        anyhow::anyhow!("running migration 55 (event_date + memory_entities backfill): {e}")
    })?;
    tracing::info!(
        "First-boot backfill complete: scanned {} memories for dates, inserted {} entity links",
        m55.event_dates_scanned,
        m55.entity_links_inserted
    );

    // Requeue any document-enrichment rows left `in_progress` by a previous run
    // (a crash / restart mid-enrichment). Their per-chunk checkpoint is
    // preserved, so the scheduler resumes them from where they stopped rather
    // than re-analyzing from scratch — restart-from-checkpoint, no manual step.
    match db_arc.reset_in_progress_documents().await {
        Ok(0) => {}
        Ok(n) => tracing::info!("[doc-enrich] requeued {n} in-progress document(s) for resume"),
        Err(e) => tracing::warn!("[doc-enrich] reset_in_progress_documents failed: {e}"),
    }

    // Consolidate user-facing assets under ~/.wenlan/.
    // - Ensure ~/.wenlan/{pages, sessions, sessions/_status} exist
    // - Symlink ~/.wenlan/db -> <data_dir> (cosmetic alias; DB stays at
    //   the platform data directory (resolved via `dirs::data_local_dir()` per OS)
    //   under `wenlan/memorydb/`, to avoid moving live SQLite/WAL files mid-flight).
    // - Migrate legacy ~/Origin/knowledge/ md files into ~/.wenlan/pages/ if
    //   the new dir is empty. Never deletes the old dir; user can clean up
    //   manually after verifying.
    if let Some(home) = dirs::home_dir() {
        let wenlan_dot = home.join(".wenlan");
        for sub in ["pages", "sessions", "sessions/_status"] {
            if let Err(e) = std::fs::create_dir_all(wenlan_dot.join(sub)) {
                tracing::warn!("[wenlan-dir] create {} failed: {}", sub, e);
            }
        }

        let db_link = wenlan_dot.join("db");
        let link_target_already_correct = std::fs::read_link(&db_link)
            .map(|t| t == data_dir)
            .unwrap_or(false);
        if !link_target_already_correct && !db_link.exists() {
            #[cfg(unix)]
            if let Err(e) = std::os::unix::fs::symlink(&data_dir, &db_link) {
                tracing::warn!(
                    "[wenlan-dir] symlink {} -> {} failed: {}",
                    db_link.display(),
                    data_dir.display(),
                    e
                );
            }
            #[cfg(windows)]
            {
                tracing::info!(
                    "Database at {} (no shortcut created; Windows symlinks require admin).",
                    data_dir.display()
                );
            }
        }

        let legacy_pages = home.join("Origin/knowledge");
        let new_pages = wenlan_dot.join("pages");
        let legacy_has_md = std::fs::read_dir(&legacy_pages)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .any(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
            })
            .unwrap_or(false);
        let new_is_empty = std::fs::read_dir(&new_pages)
            .map(|entries| {
                !entries
                    .filter_map(|e| e.ok())
                    .any(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
            })
            .unwrap_or(true);
        if legacy_has_md && new_is_empty {
            tracing::info!(
                "[migrate] copying md files from {} to {}",
                legacy_pages.display(),
                new_pages.display()
            );
            if let Ok(entries) = std::fs::read_dir(&legacy_pages) {
                let mut copied = 0usize;
                for entry in entries.filter_map(|e| e.ok()) {
                    let src = entry.path();
                    if src.extension().and_then(|s| s.to_str()) != Some("md") {
                        continue;
                    }
                    if let Some(name) = src.file_name() {
                        let dst = new_pages.join(name);
                        if dst.exists() {
                            continue;
                        }
                        match std::fs::copy(&src, &dst) {
                            Ok(_) => copied += 1,
                            Err(e) => tracing::warn!(
                                "[migrate] copy {} -> {} failed: {}",
                                src.display(),
                                dst.display(),
                                e
                            ),
                        }
                    }
                }
                tracing::info!("[migrate] copied {} md files from legacy path", copied);
            }
        }

        // Initialize ~/.wenlan/ as a git repo so users get version history
        // of pages + sessions for free. Defensive — silent skip if git is
        // missing or any step fails. Skills (/handoff, /distill, /forget)
        // commit per logical batch; daemon only does the initial bring-up
        // here.
        let dot_git = wenlan_dot.join(".git");
        let git_available = std::process::Command::new("git")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !dot_git.exists() && git_available {
            let gitignore = wenlan_dot.join(".gitignore");
            if !gitignore.exists() {
                // No trailing slash on `db` / `bin` — those entries are
                // symlinks in the consolidated layout, and pattern `db/`
                // would only match real directories.
                let _ = std::fs::write(
                    &gitignore,
                    "db\nbin\nlogs/\nsessions/_status/handoff-*.json\n",
                );
            }
            let run = |args: &[&str]| {
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(&wenlan_dot)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .ok()
                    .filter(|s| s.success())
            };
            if run(&["init", "--quiet"]).is_some() {
                let _ = run(&[
                    "-c",
                    "user.name=Wenlan",
                    "-c",
                    "user.email=daemon@origin.local",
                    "commit",
                    "--allow-empty",
                    "--quiet",
                    "-m",
                    "Wenlan initialized",
                ]);
                let _ = run(&["add", "-A"]);
                let _ = run(&[
                    "-c",
                    "user.name=Wenlan",
                    "-c",
                    "user.email=daemon@origin.local",
                    "commit",
                    "--quiet",
                    "-m",
                    "backfill: initial pages from DB",
                ]);
                tracing::info!("[wenlan-dir] git init complete at {}", wenlan_dot.display());
            }
        }
    }

    // One-time backfill: if the knowledge directory is empty but the DB has
    // active pages, write them all to disk. Handles the case where pages were
    // created before KnowledgeWriter was wired up, or via a code path that
    // bypasses the writer.
    //
    // We gate on a `.origin/.backfill-attempted` marker file (created on
    // first attempt regardless of outcome) so this block only runs once per
    // daemon install. Without the marker, a persistent write_page
    // failure — e.g. permission error on the destination directory — would
    // re-trigger a full DB scan + write attempt on every single startup.
    {
        let knowledge_path = wenlan_core::config::load_config().knowledge_path_or_default();
        let marker_path = knowledge_path.join(".wenlan").join(".backfill-attempted");

        let already_attempted = marker_path.exists();
        let has_md_files = !already_attempted
            && knowledge_path.exists()
            && std::fs::read_dir(&knowledge_path)
                .map(|entries| {
                    entries.filter_map(|e| e.ok()).any(|e| {
                        e.path()
                            .extension()
                            .and_then(|s| s.to_str())
                            .map(|ext| ext.eq_ignore_ascii_case("md"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);

        if !already_attempted && !has_md_files {
            match db_arc.list_pages("active", 10_000, 0).await {
                Ok(pages) if !pages.is_empty() => {
                    tracing::info!(
                        "[backfill] knowledge dir empty; writing {} pages to {}",
                        pages.len(),
                        knowledge_path.display()
                    );
                    let projection = wenlan_core::export::knowledge::KnowledgeProjectionWrite::new(
                        knowledge_path.clone(),
                        &db_arc,
                    );
                    let mut written = 0usize;
                    let mut failed = 0usize;
                    for page in &pages {
                        match projection.write_page(page) {
                            Ok(_) => written += 1,
                            Err(e) => {
                                tracing::warn!(
                                    "[backfill] write_page failed for {}: {}",
                                    page.id,
                                    e
                                );
                                failed += 1;
                            }
                        }
                    }
                    tracing::info!("[backfill] wrote {} pages, {} failed", written, failed);

                    // Create the marker file so we don't re-run the
                    // backfill on every subsequent startup — even if every
                    // write_page above failed. The user can delete
                    // `.origin/.backfill-attempted` to force a retry.
                    if let Some(parent) = marker_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    if let Err(e) = std::fs::write(&marker_path, "") {
                        tracing::warn!(
                            "[backfill] failed to write marker {}: {}",
                            marker_path.display(),
                            e
                        );
                    }
                }
                Ok(_) => {
                    // DB has no pages yet — nothing to backfill. Don't create
                    // the marker; the next startup after pages exist should retry.
                }
                Err(e) => {
                    tracing::warn!("[backfill] list_pages failed: {}", e);
                }
            }
        }
    }

    // Startup reconcile: repair the markdown projection from the DB.
    //
    // `write_page` renames a temp file over the target without an fsync — that
    // buys readers atomicity, not crash durability. So a crash can leave a
    // page's file missing, holding the previous version's bytes, or
    // zero-length, plus `.tmp` orphans from a write that died mid-rename.
    // This is the pass that makes "the md is a repairable projection" true.
    //
    // Runs synchronously, before `axum::serve`, for the same reason the
    // backfill above does: no HTTP write and no scheduler tick can race the
    // repair, so the pass needs no locking. The listener is already bound
    // (see the bind-first block up top), so a slow pass on a large corpus
    // delays serving, never the port handoff.
    //
    // ponytail: same 10k page ceiling as the backfill, and one pass reads
    // every projected file. If a corpus ever outgrows that, page the scan or
    // move it behind the listener — do NOT background it naively, since a
    // concurrent page write would race the repair.
    {
        let knowledge_path = wenlan_core::config::load_config().knowledge_path_or_default();
        if knowledge_path.exists() {
            match db_arc.list_pages("active", 10_000, 0).await {
                Ok(pages) => {
                    let projection = wenlan_core::export::knowledge::KnowledgeProjectionWrite::new(
                        knowledge_path.clone(),
                        &db_arc,
                    );
                    match projection.reconcile(&pages) {
                        Ok(stats)
                            if stats.rewritten > 0
                                || stats.temp_files_removed > 0
                                || stats.errors > 0 =>
                        {
                            tracing::info!(
                                "[reconcile] projection repaired: {} checked, {} rewritten, \
                                 {} temp leftover(s) swept, {} failed",
                                stats.checked,
                                stats.rewritten,
                                stats.temp_files_removed,
                                stats.errors
                            );
                        }
                        Ok(stats) => {
                            tracing::debug!(
                                "[reconcile] {} page(s) checked, all clean",
                                stats.checked
                            );
                        }
                        Err(e) => tracing::warn!("[reconcile] pass failed: {e}"),
                    }
                }
                Err(e) => tracing::warn!("[reconcile] list_pages failed: {e}"),
            }
        }
    }

    // Load intelligence config
    server_state.prompts = wenlan_core::prompts::PromptRegistry::load(
        &wenlan_core::prompts::PromptRegistry::override_dir(),
    );
    server_state.tuning =
        wenlan_core::tuning::TuningConfig::load(&wenlan_core::tuning::TuningConfig::config_path());
    server_state.quality_gate =
        wenlan_core::quality_gate::QualityGate::new(server_state.tuning.gate.clone());

    // Load API LLM providers if configured
    let config = wenlan_core::config::load_config();
    if let Some(ref key) = config.anthropic_api_key {
        if !key.is_empty() {
            let routine_model = config
                .routine_model
                .clone()
                .unwrap_or_else(|| wenlan_core::llm_provider::DEFAULT_ROUTINE_MODEL.to_string());
            let provider = wenlan_core::llm_provider::ApiProvider::new(key.clone(), routine_model);
            server_state.api_llm = Some(Arc::new(provider));
            tracing::info!("API LLM provider initialized (routine)");

            let synthesis_model = config
                .synthesis_model
                .clone()
                .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
            let provider =
                wenlan_core::llm_provider::ApiProvider::new(key.clone(), synthesis_model);
            server_state.synthesis_llm = Some(Arc::new(provider));
            tracing::info!("Synthesis LLM provider initialized");
        }
    }

    // Load external LLM provider if configured
    if let (Some(ref endpoint), Some(ref model)) =
        (&config.external_llm_endpoint, &config.external_llm_model)
    {
        if !endpoint.is_empty() && !model.is_empty() {
            let provider = wenlan_core::llm_provider::OpenAICompatibleProvider::new_with_key(
                endpoint.clone(),
                model.clone(),
                config.external_llm_api_key.clone(),
            );
            server_state.external_llm = Some(Arc::new(provider));
            tracing::info!("External LLM provider initialized from config");
        }
    }

    // Cross-encoder reranker wiring. `WENLAN_RERANKER_MODE = off|lite|full` (default
    // off) selects which retrieval paths get a CE and which model; the legacy
    // `WENLAN_RERANKER_ENABLED=1` (with MODE unset) maps to deep-only CE using the
    // configured model — exactly the pre-mode behavior. First construction downloads
    // weights (turbo ~146MB, bge-base ~1.1GB) into the shared FastEmbed cache;
    // failure is non-fatal (the affected path falls back to embedding+FTS ordering).
    let reranker_cache_dir = wenlan_core::db::resolve_fastembed_cache_dir(&data_dir);
    let mut deep_bgebase_pending = false;
    {
        use wenlan_core::reranker::{RerankerMode, RerankerPick};
        use wenlan_types::responses::RerankerStatus;
        let mode = wenlan_core::reranker::reranker_mode_resolved(&config);
        let legacy_enabled = std::env::var("WENLAN_RERANKER_ENABLED").as_deref() == Ok("1");
        let plan = wenlan_core::reranker::resolve_reranker_plan(mode, legacy_enabled);
        server_state.reranker_mode = match mode {
            RerankerMode::Off => "off",
            RerankerMode::Lite => "lite",
            RerankerMode::Full => "full",
        }
        .to_string();
        tracing::info!(
            "[reranker] mode={} (legacy_enabled={legacy_enabled}); light={:?} deep={:?}",
            server_state.reranker_mode,
            plan.light,
            plan.deep
        );

        // Light paths (quick `/api/search` + context `/api/context`): turbo
        // (~146MB), eager-load — small enough not to meaningfully block startup.
        let mut light_reranker: Option<Arc<dyn wenlan_core::reranker::Reranker>> = None;
        if let Some(pick) = plan.light {
            let cache = reranker_cache_dir.clone();
            match tokio::task::spawn_blocking(move || {
                wenlan_core::reranker::init_cross_encoder_reranker_pick(pick, cache)
            })
            .await
            {
                Ok(Ok(r)) => {
                    let model_id = r.model_id().to_string();
                    tracing::info!("[reranker] light paths active (model={model_id})");
                    server_state.reranker_light_status = RerankerStatus::Active { model_id };
                    light_reranker = Some(r.clone());
                    server_state.reranker_light = Some(r);
                }
                Ok(Err(e)) => {
                    tracing::warn!(
                        "[reranker] light init failed; quick + context fall back to plain hybrid: {e}"
                    );
                    server_state.reranker_light_status = RerankerStatus::Failed {
                        reason: e.to_string(),
                    };
                }
                Err(e) => {
                    tracing::warn!("[reranker] light init join failed: {e}");
                    server_state.reranker_light_status = RerankerStatus::Failed {
                        reason: e.to_string(),
                    };
                }
            }
        }

        // Deep path (`/api/memory/search` with rerank=true).
        match plan.deep {
            // Back-compat: ENABLED=1 + mode unset -> eager-load the configured model
            // (+ BYO via WENLAN_RERANKER_ONNX_DIR), blocking startup, exactly as before.
            Some(RerankerPick::Configured) => {
                tracing::info!(
                    "[reranker] deep path (legacy WENLAN_RERANKER_ENABLED); first run downloads \
                     weights (~1.1GB). The daemon finishes starting once the model is ready\u{2026}"
                );
                let cache = reranker_cache_dir.clone();
                match tokio::task::spawn_blocking(move || {
                    wenlan_core::reranker::init_cross_encoder_reranker(cache)
                })
                .await
                {
                    Ok(Ok(r)) => {
                        let model_id = r.model_id().to_string();
                        tracing::info!("[reranker] deep path active (model={model_id})");
                        server_state.reranker_status = RerankerStatus::Active { model_id };
                        server_state.reranker = Some(r);
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(
                            "[reranker] deep init failed; rerank=true falls back to plain hybrid: {e}"
                        );
                        server_state.reranker_status = RerankerStatus::Failed {
                            reason: e.to_string(),
                        };
                    }
                    Err(e) => {
                        tracing::warn!("[reranker] deep init join failed: {e}");
                        server_state.reranker_status = RerankerStatus::Failed {
                            reason: e.to_string(),
                        };
                    }
                }
            }
            // lite: the deep path reuses the already-loaded turbo (no second load).
            // Mirror the light status either way so a FAILED turbo load surfaces as
            // deep=failed (not a misleading deep=disabled) on /api/status; the missing
            // Arc still makes rerank=true fall back to plain hybrid. (review fix)
            Some(RerankerPick::Turbo) => {
                server_state.reranker_status = server_state.reranker_light_status.clone();
                if let Some(r) = light_reranker.clone() {
                    server_state.reranker = Some(r);
                }
            }
            // full: heavy bge-base. Council fix #3 — do NOT block startup; load it in
            // the background after the state is shared (rerank=true falls back to plain
            // until ready). Status stays Disabled until the background load completes.
            Some(RerankerPick::BgeBase) => {
                deep_bgebase_pending = true;
            }
            None => {}
        }
    }

    // Import any legacy tag data from the pre-PR-B2 spaces.db file.
    if let Some(ref db_arc) = server_state.db {
        match wenlan_core::spaces::import_legacy_tags(db_arc).await {
            Ok(n) if n > 0 => {
                tracing::info!("[startup] imported {} legacy tag triples from spaces.db", n)
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("[startup] legacy tags import failed: {e}"),
        }
    }

    // Spawn the ingest coalescer. HTTP `/api/memory/store` handlers submit
    // fully-built RawDocuments + pre-computed chunk counts; the coalescer
    // runs the full ingest pipeline (batched quality gate, partition,
    // upsert survivors) per flush window. This amortizes both the FastEmbed
    // invocation (one batched call per flush for gate's novelty check) AND
    // the libSQL transaction (one per flush for the survivors) across
    // concurrent writes.
    //
    // See `crates/wenlan-server/src/ingest_batcher.rs` for the design and
    // contract tests.
    {
        let db_for_batcher = db_arc.clone();
        let gate_for_batcher = server_state.quality_gate.clone();
        let process: ingest_batcher::BatchProcessFn = Arc::new(
            move |items: Vec<(wenlan_core::sources::RawDocument, usize)>| {
                let db = db_for_batcher.clone();
                let gate = gate_for_batcher.clone();
                Box::pin(async move { ingest_batch_process(db, gate, items).await })
            },
        );
        server_state.ingest_batcher = Some(ingest_batcher::IngestBatcher::spawn(
            process,
            ingest_batcher::BatcherConfig::default(),
        ));
    }

    let shared: SharedState = Arc::new(RwLock::new(server_state));

    // full mode: load the heavy deep bge-base in the background so startup never
    // blocks on the ~1.1GB download (council fix #3). rerank=true uses plain hybrid
    // until this completes; deep status flips to Active/Failed when the load resolves.
    if deep_bgebase_pending {
        let shared_for_deep = shared.clone();
        let cache = reranker_cache_dir.clone();
        tokio::spawn(async move {
            use wenlan_types::responses::RerankerStatus;
            tracing::info!(
                "[reranker] full mode: loading deep bge-base in background (~1.1GB first run); \
                 rerank=true uses plain hybrid until ready\u{2026}"
            );
            let loaded = tokio::task::spawn_blocking(move || {
                wenlan_core::reranker::init_cross_encoder_reranker_pick(
                    wenlan_core::reranker::RerankerPick::BgeBase,
                    cache,
                )
            })
            .await;
            match loaded {
                Ok(Ok(r)) => {
                    let model_id = r.model_id().to_string();
                    let mut st = shared_for_deep.write().await;
                    st.reranker_status = RerankerStatus::Active {
                        model_id: model_id.clone(),
                    };
                    st.reranker = Some(r);
                    tracing::info!("[reranker] deep bge-base loaded and active (model={model_id})");
                }
                Ok(Err(e)) => {
                    let mut st = shared_for_deep.write().await;
                    st.reranker_status = RerankerStatus::Failed {
                        reason: e.to_string(),
                    };
                    tracing::warn!(
                        "[reranker] deep bge-base load failed; rerank=true stays on plain hybrid: {e}"
                    );
                }
                Err(e) => {
                    let mut st = shared_for_deep.write().await;
                    st.reranker_status = RerankerStatus::Failed {
                        reason: e.to_string(),
                    };
                    tracing::warn!("[reranker] deep bge-base load task panicked: {e}");
                }
            }
        });
    }

    // Initialize on-device LLM in the background if a model is already cached.
    // This intentionally does NOT trigger a download — users opt in explicitly
    // via the settings UI (POST /api/on-device-model/download).
    {
        let shared_for_llm = shared.clone();
        let on_device_id = config.on_device_model.clone();
        tokio::spawn(async move {
            let Some(on_device_id) = on_device_id else {
                tracing::info!(
                    "[on-device] no local model selected, skipping init (run `wenlan models install` to enable)"
                );
                return;
            };
            let result = tokio::task::spawn_blocking(move || {
                let model =
                    wenlan_core::on_device_models::resolve_or_default(Some(on_device_id.as_str()));
                if !wenlan_core::on_device_models::is_cached(model) {
                    tracing::info!(
                        "[on-device] model {} not cached, skipping init (use settings to download)",
                        model.id
                    );
                    return Ok::<
                        Option<(Arc<dyn wenlan_core::llm_provider::LlmProvider>, String)>,
                        wenlan_core::error::WenlanError,
                    >(None);
                }
                let provider =
                    wenlan_core::llm_provider::OnDeviceProvider::new_with_model(Some(model.id))?;
                let arc: Arc<dyn wenlan_core::llm_provider::LlmProvider> = Arc::new(provider);
                Ok(Some((arc, model.id.to_string())))
            })
            .await;

            match result {
                Ok(Ok(Some((provider, model_id)))) => {
                    let mut state = shared_for_llm.write().await;
                    state.llm = Some(provider);
                    state.loaded_on_device_model = Some(model_id.clone());
                    tracing::info!("[on-device] model {} loaded and available", model_id);
                }
                Ok(Ok(None)) => {} // Not cached — already logged
                Ok(Err(e)) => tracing::error!("[on-device] init failed: {}", e),
                Err(e) => tracing::error!("[on-device] init task panicked: {}", e),
            }
        });
    }

    // Register the LLM-readiness hook so that the `intelligence-ready`
    // onboarding milestone fires the first time any LLM provider successfully
    // serves traffic. `mark_llm_ready` is a one-shot per process, so this hook
    // runs at most once regardless of which provider fires first.
    //
    // The on-device `llm-provider-worker` (`crates/wenlan-core/src/llm_provider.rs:142`)
    // runs on a `std::thread`, not a Tokio task — GPU inference is blocking
    // and would starve the async runtime. When it calls `mark_llm_ready()`
    // from that thread, our hook fires synchronously on a thread with no
    // Tokio reactor in thread-local context. Bare `tokio::spawn(...)` would
    // then panic: "there is no reactor running, must be called from the
    // context of a Tokio 1.x runtime" — exactly what killed the worker on
    // 2026-04-16. Capture a `Handle` here (we are inside `#[tokio::main]`)
    // and use `handle.spawn(...)` from the closure instead.
    {
        let db_for_ready = db_arc.clone();
        let emitter_for_ready: Arc<dyn wenlan_core::events::EventEmitter> =
            Arc::new(wenlan_core::events::NoopEmitter);
        let handle = tokio::runtime::Handle::current();
        let hook: wenlan_core::llm_provider::ReadinessHook = Arc::new(move || {
            let db = db_for_ready.clone();
            let emitter = emitter_for_ready.clone();
            handle.spawn(async move {
                let ev = wenlan_core::onboarding::MilestoneEvaluator::new(&db, emitter);
                if let Err(e) = ev.check_after_llm_ready().await {
                    tracing::warn!(?e, "onboarding: check_after_llm_ready failed");
                }
            });
        });
        let _ = wenlan_core::llm_provider::LLM_READINESS_HOOK.set(hook);
    }

    // Spawn the event-driven steep scheduler.
    // See docs/superpowers/specs/2026-04-12-event-driven-steep-triggers-design.md
    {
        let write_signal = {
            let s = shared.read().await;
            s.write_signal.clone()
        };
        scheduler::spawn_scheduler(shared.clone(), write_signal);
    }

    if wenlan_core::db::entity_sweep_enabled() {
        tracing::info!(
            "Background entity-enrichment sweep is ON: it backfills knowledge-graph \
             links over existing memories via your configured LLM. Set \
             WENLAN_ENABLE_ENTITY_SWEEP=0 to disable."
        );
    } else {
        tracing::info!("Background entity-enrichment sweep is OFF (WENLAN_ENABLE_ENTITY_SWEEP).");
    }

    // Build router
    let app = router::build_router(shared);

    // Advertise the bound port before accepting requests.
    // `addr` may be `127.0.0.1:0`; `local_addr()` gives the real ephemeral port.
    let local_addr = listener.local_addr()?;
    tracing::info!("Listening on http://{}", local_addr);

    // Eval harness reads this stdout line to discover the bound port even when
    // WENLAN_BIND_ADDR=127.0.0.1:0. Format MUST stay stable — see
    // crates/wenlan-core/src/eval/http_harness.rs in the P2 plan.
    println!("WENLAN_LISTENING_ON={}", local_addr);
    use std::io::Write;
    let _ = std::io::stdout().flush();

    // Alternate signal: write the port to a file if WENLAN_PORT_FILE is set.
    // Eval harness uses this when stdout is captured by tracing-appender.
    if let Ok(port_file) = std::env::var("WENLAN_PORT_FILE") {
        if let Err(e) = std::fs::write(&port_file, local_addr.port().to_string()) {
            tracing::error!("failed to write WENLAN_PORT_FILE={}: {}", port_file, e);
            return Err(anyhow::anyhow!("WENLAN_PORT_FILE write failed: {}", e));
        }
    }

    // Serve
    axum::serve(listener, app.into_make_service()).await?;

    Ok(())
}

/// Batch processor invoked by the ingest coalescer per flush. Runs the
/// full per-request ingest pipeline — quality gate evaluate (batched so
/// one FastEmbed call covers every survivor's novelty check) → partition
/// admitted vs rejected → upsert survivors in a single transaction →
/// emit per-doc outcomes in input order.
///
/// Fail-open policy on gate infrastructure failure: if the batched gate
/// evaluator itself returns an error (DB unreachable, embedding panicked
/// inside FastEmbed, etc.), every doc is admitted rather than rejected —
/// matches `QualityGate::evaluate`'s per-doc behavior, which also fails
/// open rather than wedging stores behind the gate.
async fn ingest_batch_process(
    db: std::sync::Arc<wenlan_core::db::MemoryDB>,
    gate: wenlan_core::quality_gate::QualityGate,
    items: Vec<(wenlan_core::sources::RawDocument, usize)>,
) -> Vec<ingest_batcher::StoreOutcome> {
    use ingest_batcher::StoreOutcome;
    use wenlan_core::quality_gate::{GateResult, GateScores};

    if items.is_empty() {
        return vec![];
    }

    // Batch gate evaluate. One FastEmbed call, N vector queries, one pass.
    let contents: Vec<&str> = items.iter().map(|(d, _)| d.content.as_str()).collect();
    let gate_results = match gate.evaluate_batch(&contents, &db).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("[ingest_batch_process] gate batch evaluate failed (fail closed), rejecting all: {e}");
            contents
                .iter()
                .map(|c| {
                    (
                        GateResult {
                            admitted: false,
                            reason: Some(
                                wenlan_core::quality_gate::RejectionReason::EmbeddingUnavailable(
                                    e.to_string(),
                                ),
                            ),
                            scores: GateScores {
                                content_type_pass: true,
                                novelty_score: None,
                                word_count: c.split_whitespace().count(),
                                pattern_matched: Some("embedding_unavailable".to_string()),
                                latency_ms: 0,
                            },
                        },
                        None,
                    )
                })
                .collect()
        }
    };

    let n = items.len();
    let mut outcomes: Vec<Option<StoreOutcome>> = (0..n).map(|_| None).collect();
    // (original_position, doc, chunks_predicted) for every admitted doc.
    let mut survivors: Vec<(usize, wenlan_core::sources::RawDocument, usize)> = Vec::new();

    for (i, ((doc, chunks), (gate_result, similar_id))) in
        items.into_iter().zip(gate_results).enumerate()
    {
        if gate_result.admitted {
            survivors.push((i, doc, chunks));
        } else {
            let (reason_str, detail_str) = gate_result
                .reason
                .as_ref()
                .map(|r| (r.as_str().to_string(), r.detail()))
                .unwrap_or_else(|| ("unknown".to_string(), "rejected".to_string()));
            outcomes[i] = Some(StoreOutcome::GateRejected {
                reason: reason_str,
                detail: detail_str,
                similar_to: similar_id,
            });
        }
    }

    if !survivors.is_empty() {
        let docs: Vec<wenlan_core::sources::RawDocument> =
            survivors.iter().map(|(_, d, _)| d.clone()).collect();
        match db.upsert_documents(docs).await {
            Ok(_total) => {
                for (pos, _, chunks) in &survivors {
                    outcomes[*pos] = Some(StoreOutcome::Stored {
                        chunks_created: *chunks,
                    });
                }
            }
            Err(e) => {
                let msg = e.to_string();
                for (pos, _, _) in &survivors {
                    outcomes[*pos] = Some(StoreOutcome::UpsertFailed(msg.clone()));
                }
            }
        }
    }

    // Any `None` slot means the item was neither admitted nor rejected —
    // shouldn't happen, but backfill defensively.
    outcomes
        .into_iter()
        .map(|o| o.unwrap_or(StoreOutcome::UpsertFailed("missing outcome slot".into())))
        .collect()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Propagate flags through env vars so both wenlan-server's own path logic
    // and wenlan-core's config loader (`wenlan_core::config::config_path`) see
    // the same values without plumbing a parameter through every call site.
    if let Some(ref dir) = cli.data_dir {
        std::env::set_var("WENLAN_DATA_DIR", dir);
    }
    if let Some(port) = cli.port {
        std::env::set_var("WENLAN_PORT", port.to_string());
    }

    match cli.command {
        Some(Command::BackfillStalePages { dry_run }) => cmd_backfill::run(dry_run).await,
        None => run_daemon().await,
    }
}
