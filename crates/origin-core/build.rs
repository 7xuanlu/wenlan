// SPDX-License-Identifier: Apache-2.0
//! Build script for origin-core.
//!
//! Emits two compile-time env vars used by `ReportEnv` in the eval harness:
//!
//! - `ORIGIN_MIGRATIONS_HASH`: 16-char hex prefix of SHA-256 over the
//!   migration-bearing source files (`src/db.rs` + `src/migrations/**`).
//!   Changes whenever a new migration lands, which invalidates eval caches
//!   that were built against the old schema.
//!
//! - `ORIGIN_GIT_SHA`: 12-char short git SHA of HEAD.  Unset in tarball
//!   builds where `.git/` is absent (the `option_env!` call-sites handle
//!   the None case gracefully).

use sha2::{Digest, Sha256};
use std::path::Path;

fn main() {
    // --- migrations hash ---------------------------------------------------
    // Migrations live as inline Rust inside src/db.rs and src/migrations/.
    // Hash those files so the hash changes when a migration is added or edited.
    let sources: &[&str] = &["src/db.rs", "src/migrations"];
    let mut hasher = Sha256::new();
    let mut found_any = false;

    for src in sources {
        let p = Path::new(src);
        if p.is_file() {
            println!("cargo:rerun-if-changed={}", src);
            let bytes = std::fs::read(p).unwrap_or_default();
            hasher.update(src.as_bytes());
            hasher.update(b":");
            hasher.update(&bytes);
            hasher.update(b"\n");
            found_any = true;
        } else if p.is_dir() {
            // Walk the directory, sort for determinism.
            let mut entries: Vec<_> = walkdir(p);
            entries.sort();
            for path_str in &entries {
                println!("cargo:rerun-if-changed={}", path_str);
                let bytes = std::fs::read(path_str).unwrap_or_default();
                hasher.update(path_str.as_bytes());
                hasher.update(b":");
                hasher.update(&bytes);
                hasher.update(b"\n");
                found_any = true;
            }
        }
    }

    if found_any {
        let hex = format!("{:x}", hasher.finalize());
        let short: String = hex.chars().take(16).collect();
        println!("cargo:rustc-env=ORIGIN_MIGRATIONS_HASH={}", short);
    } else {
        println!("cargo:rustc-env=ORIGIN_MIGRATIONS_HASH=missing");
    }

    // --- git SHA -----------------------------------------------------------
    // Best-effort: silently skipped in tarball / CI checkouts without .git.
    println!("cargo:rerun-if-changed=.git/HEAD");
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
    {
        if output.status.success() {
            let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !sha.is_empty() {
                println!("cargo:rustc-env=ORIGIN_GIT_SHA={}", sha);
            }
        }
    }
}

/// Recursively collect all file paths under `dir` as strings.
fn walkdir(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_file() {
            if let Some(s) = p.to_str() {
                out.push(s.to_owned());
            }
        } else if p.is_dir() {
            out.extend(walkdir(&p));
        }
    }
    out
}
