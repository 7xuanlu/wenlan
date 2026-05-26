// SPDX-License-Identifier: Apache-2.0
//! L2 eval orchestration — spawn `origin-server`, exercise the public HTTP
//! surface, return baseline reports stamped with [`EvalLayer::L2Http`].
//!
//! # Status
//!
//! What ships in P2:
//!   - [`L2Config`] — caller-facing knobs (fixture path, variant, warmup count).
//!   - [`smoke_preflight`] — health probe + N warmup `/api/health` GETs to
//!     pay one-time cold costs (route-table init, libsql page cache touch)
//!     before the real timed run.
//!   - [`stamp_l2_env`] — builds a [`ReportEnv`] with `layer=L2Http` so the
//!     P0b layered baseline path can consume L2 reports without ambiguity.
//!   - [`run_locomo_l2`] / [`run_longmemeval_l2`] — orchestration shells that
//!     spawn the daemon and run smoke preflight, then **return `Err(_)` with
//!     `not-yet-wired`** until per-scenario HTTP ingest + scoring is wired
//!     in a follow-up PR.
//!
//! Why the deferral: the existing L1 runners (`run_locomo_eval` etc.) drive
//! `MemoryDB` directly and score in-process. Reusing their per-question
//! NDCG/MRR/Recall logic against an HTTP backend requires either (a) factoring
//! the scoring loop out of `locomo.rs::run_locomo_eval` into a
//! transport-agnostic scorer, or (b) duplicating ~250 LOC of scoring in
//! `l2_runner`. Both are larger than P2's budget. Shipping the harness now
//! lets the wiring PR focus on the scoring extraction alone, and keeps the
//! `DaemonHandle` + `OrphanReaper` infra usable by future P3 work in the
//! meantime.

use anyhow::Result;

use crate::eval::http_harness::DaemonHandle;
use crate::eval::report::ReportEnv;
use crate::eval::EvalLayer;

/// Caller-facing config for an L2 baseline run.
#[derive(Debug, Clone)]
pub struct L2Config {
    /// Filesystem path to the source fixture (e.g. `locomo10.json` or
    /// `longmemeval_s.json`). Treated as the same payload the L1 runners
    /// consume.
    pub fixture_path: std::path::PathBuf,
    /// Root for layered baseline output (default
    /// `~/.cache/origin-eval/baselines`).
    pub baselines_root: std::path::PathBuf,
    /// `"base"` or `"reranked"`. Wired into [`ReportEnv::variant`] +
    /// `retrieval_method`.
    pub variant: &'static str,
    /// Sequential calls only — libsql `Mutex<Connection>` serialises writes,
    /// so multi-thread harness concurrency was theatre. Default 1.
    pub max_concurrency: usize,
    /// Pre-timed warmup calls excluded from `LatencySummary`. Recorded on
    /// `ReportEnv::warmup_iterations` for downstream reproducibility checks.
    pub warmup_iterations: u32,
}

impl Default for L2Config {
    fn default() -> Self {
        Self {
            fixture_path: std::path::PathBuf::new(),
            baselines_root: std::path::PathBuf::new(),
            variant: "base",
            max_concurrency: 1,
            warmup_iterations: 5,
        }
    }
}

/// Smoke preflight — health probe + `warmup_n` extra `/api/health` GETs.
///
/// **Why GETs, not stores:** the daemon's first request pays cold costs
/// (route-table init, libsql page cache touch, fastembed model load).
/// `/api/health` exercises the same axum router + tokio runtime path as
/// `/api/memory/search` without dirtying scenario state. Wiring scenario
/// ingest into smoke would pollute `space_id` with smoke-test seeds; the
/// real per-scenario ingest happens in the (not-yet-wired) main loop.
///
/// `warmup_n` is informational only — the call count happens to equal
/// the `ReportEnv::warmup_iterations` stamp produced by [`stamp_l2_env`].
pub async fn smoke_preflight(daemon: &DaemonHandle, warmup_n: u32) -> Result<()> {
    let client = reqwest::Client::new();
    client
        .get(daemon.url("/api/health"))
        .send()
        .await?
        .error_for_status()?;
    for _ in 0..warmup_n {
        client
            .get(daemon.url("/api/health"))
            .send()
            .await?
            .error_for_status()?;
    }
    Ok(())
}

/// Build a [`ReportEnv`] for an L2 baseline run.
///
/// Pre-populates the layered-path fields ([`ReportEnv::layer`],
/// [`ReportEnv::task`], [`ReportEnv::variant`], [`ReportEnv::retrieval_method`],
/// [`ReportEnv::warmup_iterations`]) so callers only need to fill the
/// fixture/embedder/provider fields. Defaults match the L2Http convention:
///   - `layer = L2Http`
///   - `retrieval_method = "search_memory"` for variant `"base"`,
///     `"search_memory_reranked"` for variant `"reranked"`
///   - `n_runs = 1`, `is_single_run = true` (L2 baselines are single runs;
///     multi-run aggregation belongs at a future layer).
pub fn stamp_l2_env(task: &str, variant: &'static str, warmup: u32) -> ReportEnv {
    let retrieval_method = match variant {
        "base" => "search_memory",
        "reranked" => "search_memory_reranked",
        other => other,
    }
    .to_string();
    ReportEnv {
        layer: Some(EvalLayer::L2Http),
        task: Some(task.to_string()),
        variant: Some(variant.to_string()),
        retrieval_method,
        warmup_iterations: warmup,
        n_runs: 1,
        is_single_run: true,
        ..ReportEnv::default()
    }
}

/// L2 LoCoMo baseline runner.
///
/// **NOT YET WIRED.** Spawns the daemon, runs [`smoke_preflight`], then
/// returns `Err(_)` with explanatory text. The follow-up PR will fold in
/// per-scenario HTTP ingest + scoring once `run_locomo_eval`'s scorer is
/// factored out of `locomo.rs`. See module doc for rationale.
///
/// `_config` is accepted so the public signature stays stable across the
/// scaffolding → wired transition; callers can write `run_locomo_l2(cfg)`
/// today and have it light up automatically when wiring lands.
#[allow(dead_code)]
pub async fn run_locomo_l2(_config: L2Config) -> Result<crate::eval::locomo::LocomoReport> {
    let daemon = DaemonHandle::spawn().await?;
    smoke_preflight(&daemon, _config.warmup_iterations).await?;
    anyhow::bail!(
        "run_locomo_l2: NOT YET WIRED — scoring loop pending follow-up to factor \
         per-scenario scoring out of locomo::run_locomo_eval. See l2_runner module doc."
    )
}

/// L2 LongMemEval baseline runner.
///
/// **NOT YET WIRED.** See [`run_locomo_l2`] for status + rationale.
#[allow(dead_code)]
pub async fn run_longmemeval_l2(
    _config: L2Config,
) -> Result<crate::eval::longmemeval::LongMemEvalReport> {
    let daemon = DaemonHandle::spawn().await?;
    smoke_preflight(&daemon, _config.warmup_iterations).await?;
    anyhow::bail!(
        "run_longmemeval_l2: NOT YET WIRED — scoring loop pending follow-up to factor \
         per-scenario scoring out of longmemeval::run_longmemeval_eval. See l2_runner module doc."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l2config_default_is_sequential_with_warmup() {
        let c = L2Config::default();
        assert_eq!(c.max_concurrency, 1);
        assert_eq!(c.warmup_iterations, 5);
        assert_eq!(c.variant, "base");
    }

    #[test]
    fn stamp_l2_env_marks_layer_l2http() {
        let env = stamp_l2_env("locomo", "base", 5);
        assert_eq!(env.layer, Some(EvalLayer::L2Http));
        assert_eq!(env.task.as_deref(), Some("locomo"));
        assert_eq!(env.variant.as_deref(), Some("base"));
        assert_eq!(env.warmup_iterations, 5);
        assert!(env.is_single_run);
        assert_eq!(env.n_runs, 1);
    }

    #[test]
    fn stamp_l2_env_maps_variant_to_retrieval_method() {
        assert_eq!(
            stamp_l2_env("locomo", "base", 5).retrieval_method,
            "search_memory"
        );
        assert_eq!(
            stamp_l2_env("locomo", "reranked", 5).retrieval_method,
            "search_memory_reranked"
        );
    }

    #[test]
    fn stamp_l2_env_produces_layered_path_consumable_by_save_full_report() {
        // The comparable_env_hash + encode_baseline_path contracts both
        // require layer/task/variant on the env. Verify they're all present
        // so save_full_report won't bail at the env-presence guard.
        let env = stamp_l2_env("longmemeval", "reranked", 0);
        assert!(env.layer.is_some());
        assert!(env.task.is_some());
        assert!(env.variant.is_some());
        let _ = crate::eval::report::comparable_env_hash(&env);
        let _ =
            crate::eval::report::encode_baseline_path(std::path::Path::new("/tmp/baselines"), &env);
    }
}
