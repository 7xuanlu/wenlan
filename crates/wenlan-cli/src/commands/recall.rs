// SPDX-License-Identifier: Apache-2.0
//! `wenlan recall <query>` — POST /api/memory/search.

use anyhow::Result;
use wenlan_types::responses::SearchMemoryResponse;

use crate::client::WenlanClient;
use crate::output::{print_json, ResolvedFormat};

pub async fn run(
    client: &WenlanClient,
    format: ResolvedFormat,
    quiet: bool,
    query: String,
    limit: usize,
) -> Result<()> {
    let response = client.recall(query, limit).await?;
    if quiet {
        return Ok(());
    }
    match format {
        ResolvedFormat::Json => print_json(&response)?,
        ResolvedFormat::Table => print!("{}", format_table(&response)),
    }
    Ok(())
}

fn format_table(response: &SearchMemoryResponse) -> String {
    if response.results.is_empty() {
        return "(no recalled memories)\n".to_string();
    }
    let mut output = format!(
        "{} recalled memor{} in {:.0}ms\n",
        response.results.len(),
        if response.results.len() == 1 {
            "y"
        } else {
            "ies"
        },
        response.took_ms
    );
    for result in &response.results {
        let title = if result.title.is_empty() {
            result.content.lines().next().unwrap_or("(no title)")
        } else {
            &result.title
        };
        let title = if title.chars().count() > 70 {
            format!("{}...", title.chars().take(67).collect::<String>())
        } else {
            title.to_string()
        };
        output.push_str(&format!(
            "  [{:.3}] {} ({})\n",
            result.score, title, result.source_id
        ));
    }
    output.push_str(&super::format_supplemental_pages(
        response.supplemental_pages.as_deref(),
    ));
    output
}
