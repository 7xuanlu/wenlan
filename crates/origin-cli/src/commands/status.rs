// SPDX-License-Identifier: Apache-2.0
//! `origin status` — show daemon health + version.

use anyhow::Result;

use crate::client::OriginClient;
use crate::output::{print_json, OutputFormat};

/// `format` is the resolved output format (Auto already collapsed in main).
/// `quiet` suppresses success output; errors still propagate via `?` to stderr.
/// We still hit the daemon under `quiet` to surface connection failures via exit code.
pub async fn run(client: &OriginClient, format: OutputFormat, quiet: bool) -> Result<()> {
    let health = client.health().await?;
    if quiet {
        return Ok(());
    }
    match format {
        OutputFormat::Json => {
            print_json(&health)?;
        }
        OutputFormat::Table => {
            println!("Origin daemon");
            println!("  Status:        {}", health.status);
            println!(
                "  DB:            {}",
                if health.db_initialized {
                    "initialized"
                } else {
                    "uninitialized"
                }
            );
            println!("  Version:       {}", health.version);
            println!("  Host:          {}", client.base_url());
        }
        OutputFormat::Auto => unreachable!("Auto resolved by main before dispatch"),
    }
    Ok(())
}
