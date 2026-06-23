// SPDX-License-Identifier: Apache-2.0
//! `wenlan search <query>` — POST /api/search.

use anyhow::Result;
use wenlan_types::responses::SearchResponse;

use crate::client::WenlanClient;
use crate::output::{print_json, OutputFormat};

pub async fn run(
    client: &WenlanClient,
    format: OutputFormat,
    quiet: bool,
    query: String,
    limit: usize,
) -> Result<()> {
    let resp = client.search(query, limit).await?;
    if quiet {
        return Ok(());
    }
    match format {
        OutputFormat::Json => print_json(&resp)?,
        OutputFormat::Table => print_table(&resp),
        OutputFormat::Auto => unreachable!("Auto resolved by main before dispatch"),
    }
    Ok(())
}

fn print_table(resp: &SearchResponse) {
    if resp.results.is_empty() {
        println!("(no results)");
        return;
    }
    println!("{} result(s) in {:.0}ms", resp.results.len(), resp.took_ms);
    for r in &resp.results {
        // Use title if non-empty, otherwise fall back to first content line.
        let title: &str = if r.title.is_empty() {
            r.content.lines().next().unwrap_or("(no title)")
        } else {
            &r.title
        };
        // Truncate to 60 chars.
        let title_disp = if title.chars().count() > 60 {
            format!("{}...", title.chars().take(57).collect::<String>())
        } else {
            title.to_string()
        };
        println!("  [{:.3}] {} ({})", r.score, title_disp, r.source_id);
    }
}
