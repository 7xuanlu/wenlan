// SPDX-License-Identifier: Apache-2.0
//! `origin status` — show daemon health + version.

use anyhow::Result;

use crate::client::OriginClient;
use crate::output::{print_json, OutputFormat};

pub async fn run(client: &OriginClient, format: OutputFormat, quiet: bool) -> Result<()> {
    let health = client.health().await?;
    let format = format.resolve();
    if quiet {
        return Ok(());
    }
    match format {
        OutputFormat::Json => {
            print_json(&health)?;
        }
        OutputFormat::Table | OutputFormat::Auto => {
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
    }
    Ok(())
}
