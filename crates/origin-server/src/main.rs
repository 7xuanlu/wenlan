// SPDX-License-Identifier: Apache-2.0
//! Origin headless daemon — runs the memory server without Tauri.

mod cmd_backfill;

/// Resolve the bind address. Honors the `ORIGIN_BIND_ADDR` env var when set
/// (e.g. inside Docker where the daemon must listen on `0.0.0.0`). Falls back
/// to the localhost-only address used by the macOS/native install path.
fn resolve_bind_addr(port: u16) -> String {
    std::env::var("ORIGIN_BIND_ADDR")
        .ok()
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
        std::env::remove_var("ORIGIN_BIND_ADDR");
        assert_eq!(resolve_bind_addr(7878), "127.0.0.1:7878");
    }

    #[test]
    fn honors_env_when_set() {
        let _guard = env_lock().lock().unwrap();
        std::env::set_var("ORIGIN_BIND_ADDR", "0.0.0.0:9090");
        assert_eq!(resolve_bind_addr(7878), "0.0.0.0:9090");
        std::env::remove_var("ORIGIN_BIND_ADDR");
    }
}

// All other modules live in the library target (src/lib.rs) so that
// integration tests in tests/ can reference them as origin_server::<mod>.
use origin_server::{
    ingest_batcher, router, scheduler,
    state::{ServerState, SharedState},
};

use clap::{Parser, Subcommand};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Origin memory daemon — headless HTTP server.
#[derive(Parser)]
#[command(
    name = "origin-server",
    bin_name = "origin-server",
    version,
    about = "Origin headless HTTP daemon."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Override the data directory (for isolated dev/demo runs).
    /// When set, the daemon reads/writes the DB at `<dir>/memorydb/origin_memory.db`
    /// and config at `<dir>/config.json` instead of the default
    /// the platform data directory under `dirs::data_local_dir().join("origin/")`.
    /// macOS: `~/Library/Application Support/origin/`. Linux: `~/.local/share/origin/`. Windows: `%LOCALAPPDATA%\origin\`. Also honored via `ORIGIN_DATA_DIR` env.
    #[arg(long, global = true)]
    data_dir: Option<std::path::PathBuf>,

    /// Override the HTTP port (default 7878). Useful when running a scratch
    /// daemon alongside the main one. Also honored via `ORIGIN_PORT` env.
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
                .unwrap_or_else(|_| "info,origin_core=info,origin_server=info".into()),
        )
        .init();

    tracing::info!("origin-server v{}", origin_core::version());

    // Port (clap `--port`/`ORIGIN_PORT` → env var set by main(); read here)
    let port: u16 = std::env::var("ORIGIN_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(7878);

    // Data directory. `ORIGIN_DATA_DIR` (set by `--data-dir` flag) overrides the
    // default, enabling isolated dev/demo runs (e.g. `--data-dir /tmp/origin-demo`).
    let origin_root = std::env::var_os("ORIGIN_DATA_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            dirs::data_local_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("origin")
        });
    let data_dir = origin_root.join("memorydb");
    tracing::info!("Origin data root: {}", origin_root.display());

    // Build state
    let mut server_state = ServerState::new();

    // Init MemoryDB
    let emitter: Arc<dyn origin_core::events::EventEmitter> = Arc::new(origin_core::NoopEmitter);
    tracing::info!("Initializing MemoryDB at {}", data_dir.display());
    let db = origin_core::db::MemoryDB::new(&data_dir, emitter).await?;
    let db_arc = Arc::new(db);
    server_state.db = Some(db_arc.clone());

    // Run migration-55 backfill (event_date regex Pass A + memory_entities Pass B)
    // before the HTTP listener binds so no ingest races the backfill. Idempotent.
    db_arc.run_migration_55().await.map_err(|e| {
        anyhow::anyhow!("running migration 55 (event_date + memory_entities backfill): {e}")
    })?;

    // Consolidate user-facing assets under ~/.origin/.
    // - Ensure ~/.origin/{pages, sessions, sessions/_status} exist
    // - Symlink ~/.origin/db -> <data_dir> (cosmetic alias; DB stays at
    //   the platform data directory (resolved via `dirs::data_local_dir()` per OS)
    //   under `origin/memorydb/`, to avoid moving live SQLite/WAL files mid-flight).
    // - Migrate legacy ~/Origin/knowledge/ md files into ~/.origin/pages/ if
    //   the new dir is empty. Never deletes the old dir; user can clean up
    //   manually after verifying.
    if let Some(home) = dirs::home_dir() {
        let origin_dot = home.join(".origin");
        for sub in ["pages", "sessions", "sessions/_status"] {
            if let Err(e) = std::fs::create_dir_all(origin_dot.join(sub)) {
                tracing::warn!("[origin-dir] create {} failed: {}", sub, e);
            }
        }

        let db_link = origin_dot.join("db");
        let link_target_already_correct = std::fs::read_link(&db_link)
            .map(|t| t == data_dir)
            .unwrap_or(false);
        if !link_target_already_correct && !db_link.exists() {
            #[cfg(unix)]
            if let Err(e) = std::os::unix::fs::symlink(&data_dir, &db_link) {
                tracing::warn!(
                    "[origin-dir] symlink {} -> {} failed: {}",
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
        let new_pages = origin_dot.join("pages");
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

        // Initialize ~/.origin/ as a git repo so users get version history
        // of pages + sessions for free. Defensive — silent skip if git is
        // missing or any step fails. Skills (/handoff, /distill, /forget)
        // commit per logical batch; daemon only does the initial bring-up
        // here.
        let dot_git = origin_dot.join(".git");
        let git_available = std::process::Command::new("git")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !dot_git.exists() && git_available {
            let gitignore = origin_dot.join(".gitignore");
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
                    .current_dir(&origin_dot)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .ok()
                    .filter(|s| s.success())
            };
            if run(&["init", "--quiet"]).is_some() {
                let _ = run(&[
                    "-c",
                    "user.name=Origin",
                    "-c",
                    "user.email=daemon@origin.local",
                    "commit",
                    "--allow-empty",
                    "--quiet",
                    "-m",
                    "Origin initialized",
                ]);
                let _ = run(&["add", "-A"]);
                let _ = run(&[
                    "-c",
                    "user.name=Origin",
                    "-c",
                    "user.email=daemon@origin.local",
                    "commit",
                    "--quiet",
                    "-m",
                    "backfill: initial pages from DB",
                ]);
                tracing::info!("[origin-dir] git init complete at {}", origin_dot.display());
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
        let knowledge_path = origin_core::config::load_config().knowledge_path_or_default();
        let marker_path = knowledge_path.join(".origin").join(".backfill-attempted");

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
                    let writer = origin_core::export::knowledge::KnowledgeWriter::new(
                        knowledge_path.clone(),
                    );
                    let mut written = 0usize;
                    let mut failed = 0usize;
                    for page in &pages {
                        match writer.write_page(page) {
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

    // Load intelligence config
    server_state.prompts = origin_core::prompts::PromptRegistry::load(
        &origin_core::prompts::PromptRegistry::override_dir(),
    );
    server_state.tuning =
        origin_core::tuning::TuningConfig::load(&origin_core::tuning::TuningConfig::config_path());
    server_state.quality_gate =
        origin_core::quality_gate::QualityGate::new(server_state.tuning.gate.clone());

    // Load API LLM providers if configured
    let config = origin_core::config::load_config();
    if let Some(ref key) = config.anthropic_api_key {
        if !key.is_empty() {
            let routine_model = config
                .routine_model
                .clone()
                .unwrap_or_else(|| origin_core::llm_provider::DEFAULT_ROUTINE_MODEL.to_string());
            let provider = origin_core::llm_provider::ApiProvider::new(key.clone(), routine_model);
            server_state.api_llm = Some(Arc::new(provider));
            tracing::info!("API LLM provider initialized (routine)");

            let synthesis_model = config
                .synthesis_model
                .clone()
                .unwrap_or_else(|| "claude-sonnet-4-6".to_string());
            let provider =
                origin_core::llm_provider::ApiProvider::new(key.clone(), synthesis_model);
            server_state.synthesis_llm = Some(Arc::new(provider));
            tracing::info!("Synthesis LLM provider initialized");
        }
    }

    // Load external LLM provider if configured
    if let (Some(ref endpoint), Some(ref model)) =
        (&config.external_llm_endpoint, &config.external_llm_model)
    {
        if !endpoint.is_empty() && !model.is_empty() {
            let provider = origin_core::llm_provider::OpenAICompatibleProvider::new(
                endpoint.clone(),
                model.clone(),
            );
            server_state.external_llm = Some(Arc::new(provider));
            tracing::info!("External LLM provider initialized from config");
        }
    }

    // Initialize cross-encoder reranker. Opt-in via `ORIGIN_RERANKER_ENABLED=1`
    // because first construction downloads ~600MB of model weights. Reuses the
    // embedder's FastEmbed cache directory so the download is shared with the
    // BGE embedder. Failure is non-fatal: search falls back to embedding+FTS
    // ordering with no rerank pass.
    if std::env::var("ORIGIN_RERANKER_ENABLED").as_deref() == Ok("1") {
        let cache_dir = origin_core::db::resolve_fastembed_cache_dir(&data_dir);
        let init_result = tokio::task::spawn_blocking(move || {
            origin_core::reranker::init_cross_encoder_reranker(cache_dir)
        })
        .await;
        match init_result {
            Ok(Ok(reranker)) => {
                tracing::info!(
                    "[reranker] cross-encoder initialized (model={})",
                    reranker.model_id()
                );
                server_state.reranker = Some(reranker);
            }
            Ok(Err(e)) => {
                tracing::warn!("[reranker] init failed, falling back to embedding+FTS only: {e}");
            }
            Err(e) => {
                tracing::warn!(
                    "[reranker] init join failed, falling back to embedding+FTS only: {e}"
                );
            }
        }
    } else {
        tracing::info!(
            "[reranker] ORIGIN_RERANKER_ENABLED!=1, skipping cross-encoder init (set =1 to enable)"
        );
    }

    // Import any legacy tag data from the pre-PR-B2 spaces.db file.
    if let Some(ref db_arc) = server_state.db {
        match origin_core::spaces::import_legacy_tags(db_arc).await {
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
    // See `crates/origin-server/src/ingest_batcher.rs` for the design and
    // contract tests.
    {
        let db_for_batcher = db_arc.clone();
        let gate_for_batcher = server_state.quality_gate.clone();
        let process: ingest_batcher::BatchProcessFn = Arc::new(
            move |items: Vec<(origin_core::sources::RawDocument, usize)>| {
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

    // Initialize on-device LLM in the background if a model is already cached.
    // This intentionally does NOT trigger a download — users opt in explicitly
    // via the settings UI (POST /api/on-device-model/download).
    {
        let shared_for_llm = shared.clone();
        let on_device_id = config.on_device_model.clone();
        tokio::spawn(async move {
            let Some(on_device_id) = on_device_id else {
                tracing::info!(
                    "[on-device] no local model selected, skipping init (run `origin model install` to enable)"
                );
                return;
            };
            let result = tokio::task::spawn_blocking(move || {
                let model =
                    origin_core::on_device_models::resolve_or_default(Some(on_device_id.as_str()));
                if !origin_core::on_device_models::is_cached(model) {
                    tracing::info!(
                        "[on-device] model {} not cached, skipping init (use settings to download)",
                        model.id
                    );
                    return Ok::<
                        Option<(Arc<dyn origin_core::llm_provider::LlmProvider>, String)>,
                        origin_core::error::OriginError,
                    >(None);
                }
                let provider =
                    origin_core::llm_provider::OnDeviceProvider::new_with_model(Some(model.id))?;
                let arc: Arc<dyn origin_core::llm_provider::LlmProvider> = Arc::new(provider);
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
    // The on-device `llm-provider-worker` (`crates/origin-core/src/llm_provider.rs:142`)
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
        let emitter_for_ready: Arc<dyn origin_core::events::EventEmitter> =
            Arc::new(origin_core::events::NoopEmitter);
        let handle = tokio::runtime::Handle::current();
        let hook: origin_core::llm_provider::ReadinessHook = Arc::new(move || {
            let db = db_for_ready.clone();
            let emitter = emitter_for_ready.clone();
            handle.spawn(async move {
                let ev = origin_core::onboarding::MilestoneEvaluator::new(&db, emitter);
                if let Err(e) = ev.check_after_llm_ready().await {
                    tracing::warn!(?e, "onboarding: check_after_llm_ready failed");
                }
            });
        });
        let _ = origin_core::llm_provider::LLM_READINESS_HOOK.set(hook);
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

    // Build router
    let app = router::build_router(shared);

    // Bind
    let addr = resolve_bind_addr(port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            // Check if existing daemon is healthy
            tracing::warn!("Failed to bind {}: {}", addr, e);
            let url = format!("http://127.0.0.1:{}/api/health", port);
            match reqwest::get(&url).await {
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

    // Advertise the bound port before accepting requests.
    // `addr` may be `127.0.0.1:0`; `local_addr()` gives the real ephemeral port.
    let local_addr = listener.local_addr()?;
    tracing::info!("Listening on http://{}", local_addr);

    // Eval harness reads this stdout line to discover the bound port even when
    // ORIGIN_BIND_ADDR=127.0.0.1:0. Format MUST stay stable — see
    // crates/origin-core/src/eval/http_harness.rs in the P2 plan.
    println!("ORIGIN_LISTENING_ON={}", local_addr);
    use std::io::Write;
    let _ = std::io::stdout().flush();

    // Alternate signal: write the port to a file if ORIGIN_PORT_FILE is set.
    // Eval harness uses this when stdout is captured by tracing-appender.
    if let Ok(port_file) = std::env::var("ORIGIN_PORT_FILE") {
        if let Err(e) = std::fs::write(&port_file, local_addr.port().to_string()) {
            tracing::error!("failed to write ORIGIN_PORT_FILE={}: {}", port_file, e);
            return Err(anyhow::anyhow!("ORIGIN_PORT_FILE write failed: {}", e));
        }
    }

    // Serve
    axum::serve(listener, app).await?;

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
    db: std::sync::Arc<origin_core::db::MemoryDB>,
    gate: origin_core::quality_gate::QualityGate,
    items: Vec<(origin_core::sources::RawDocument, usize)>,
) -> Vec<ingest_batcher::StoreOutcome> {
    use ingest_batcher::StoreOutcome;
    use origin_core::quality_gate::{GateResult, GateScores};

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
                                origin_core::quality_gate::RejectionReason::EmbeddingUnavailable(
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
    let mut survivors: Vec<(usize, origin_core::sources::RawDocument, usize)> = Vec::new();

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
        let docs: Vec<origin_core::sources::RawDocument> =
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

    // Propagate flags through env vars so both origin-server's own path logic
    // and origin-core's config loader (`origin_core::config::config_path`) see
    // the same values without plumbing a parameter through every call site.
    if let Some(ref dir) = cli.data_dir {
        std::env::set_var("ORIGIN_DATA_DIR", dir);
    }
    if let Some(port) = cli.port {
        std::env::set_var("ORIGIN_PORT", port.to_string());
    }

    match cli.command {
        Some(Command::BackfillStalePages { dry_run }) => cmd_backfill::run(dry_run).await,
        None => run_daemon().await,
    }
}
