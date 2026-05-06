// SPDX-License-Identifier: Apache-2.0
//! fixture-gen: Generate eval fixtures.
//!
//! Lives in origin-core (not the Tauri app crate) so Tauri's bundler
//! doesn't copy it into Origin.app/Contents/MacOS. Binaries under
//! `origin-core/src/bin/` are discoverable by cargo but invisible to
//! `tauri build`, which only scans the app crate's `[[bin]]` entries.
//!
//! Usage:
//!   cargo run -p origin-core --bin fixture_gen -- --mode regression --count 6 --out eval/fixtures/gen/regression
//!   cargo run -p origin-core --bin fixture_gen -- --mode blind --count 10 --out eval/fixtures/gen/blind
//!   cargo run -p origin-core --bin fixture_gen -- --help

use origin_core::eval::gen;
use origin_core::llm_provider::{LlmProvider, OnDeviceProvider};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug)]
struct Args {
    mode: String,
    count: usize,
    out_dir: PathBuf,
}

fn parse_args() -> Result<Args, String> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!("fixture-gen: Generate eval fixtures\n");
        eprintln!("Usage:");
        eprintln!(
            "  cargo run -p origin-core --bin fixture_gen -- --mode <regression|blind> [OPTIONS]\n"
        );
        eprintln!("Modes:");
        eprintln!("  regression     Pipeline-aware adversarial fixtures (requires on-device LLM)");
        eprintln!("  blind          Pipeline-ignorant fixtures (requires on-device LLM)\n");
        eprintln!("Options:");
        eprintln!("  --count N      Number of fixtures to generate (default: 6 for regression, 10 for blind)");
        eprintln!("  --out DIR      Output directory (default: eval/fixtures/gen/<mode>)");
        std::process::exit(0);
    }

    let mode = get_flag(&args, "--mode").ok_or("--mode is required (regression or blind)")?;

    if mode != "regression" && mode != "blind" {
        return Err(format!(
            "unknown mode '{mode}' — expected 'regression' or 'blind'"
        ));
    }

    let default_count = if mode == "regression" { 6 } else { 10 };
    let count = get_flag(&args, "--count")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(default_count);

    let default_out = format!("eval/fixtures/gen/{}", mode);
    let out_dir = PathBuf::from(get_flag(&args, "--out").unwrap_or(default_out));

    Ok(Args {
        mode,
        count,
        out_dir,
    })
}

fn get_flag(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Error: {e}");
            eprintln!("Run with --help for usage.");
            std::process::exit(1);
        }
    };

    eprintln!("fixture-gen: mode={}", args.mode);

    match args.mode.as_str() {
        "regression" | "blind" => {
            eprintln!("count={}, out={}", args.count, args.out_dir.display());
            eprintln!("Loading on-device LLM (this may download the model on first run)...");
            let provider = match OnDeviceProvider::new() {
                Ok(p) => Arc::new(p) as Arc<dyn LlmProvider>,
                Err(e) => {
                    eprintln!("Failed to initialize LLM: {e}");
                    eprintln!("The on-device model (Qwen3-4B) is required for fixture generation.");
                    std::process::exit(1);
                }
            };

            let result = match args.mode.as_str() {
                "regression" => {
                    gen::generate_regression(&provider, args.count, &args.out_dir).await
                }
                "blind" => gen::generate_blind(&provider, args.count, &args.out_dir).await,
                _ => unreachable!(),
            };

            match result {
                Ok(n) => eprintln!("Generated {n} fixtures in {}", args.out_dir.display()),
                Err(e) => {
                    eprintln!("Generation failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        _ => unreachable!(),
    }
}
