// SPDX-License-Identifier: Apache-2.0
//! `wenlan search <query>` — POST /api/search.

use anyhow::Result;
use wenlan_types::responses::SearchResponse;

use crate::client::WenlanClient;
use crate::output::{print_json, ResolvedFormat};

pub async fn run(
    client: &WenlanClient,
    format: ResolvedFormat,
    quiet: bool,
    query: String,
    limit: usize,
) -> Result<()> {
    let resp = client.search(query, limit).await?;
    if quiet {
        return Ok(());
    }
    match format {
        ResolvedFormat::Json => print_json(&resp)?,
        ResolvedFormat::Table => print_table(&resp),
    }
    Ok(())
}

fn print_table(resp: &SearchResponse) {
    print!("{}", format_table(resp));
}

fn format_table(resp: &SearchResponse) -> String {
    if resp.results.is_empty() {
        return "(no results)\n".to_string();
    }
    let mut output = format!(
        "{} result(s) in {:.0}ms\n",
        resp.results.len(),
        resp.took_ms
    );
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
        output.push_str(&format!(
            "  [{:.3}] {} ({})\n",
            r.score, title_disp, r.source_id
        ));
    }
    output.push_str(&super::format_supplemental_pages(
        resp.supplemental_pages.as_deref(),
    ));
    output
}
