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
    let pages = super::format_supplemental_pages(response.supplemental_pages.as_deref());
    if response.results.is_empty() {
        // Pages can match when no memory does (a new library holds notes
        // before any memory), so they still print under the empty line.
        return format!("(no recalled memories)\n{pages}");
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
    output.push_str(&pages);
    output
}

#[cfg(test)]
mod tests {
    use super::format_table;
    use crate::commands::tests::page;
    use wenlan_types::responses::SearchMemoryResponse;

    #[test]
    fn pages_print_when_no_memory_matches() {
        let response = SearchMemoryResponse {
            results: Vec::new(),
            took_ms: 1.0,
            supplemental_pages: Some(vec![page("Sourdough starter routine", "page_1")]),
        };
        let output = format_table(&response);
        assert!(output.starts_with("(no recalled memories)\n"), "{output}");
        assert!(
            output.contains("Compiled pages:\n  - Sourdough starter routine (page_1)"),
            "{output}"
        );
    }

    #[test]
    fn nothing_matched_prints_only_the_empty_line() {
        let response = SearchMemoryResponse {
            results: Vec::new(),
            took_ms: 1.0,
            supplemental_pages: None,
        };
        assert_eq!(format_table(&response), "(no recalled memories)\n");
    }
}
