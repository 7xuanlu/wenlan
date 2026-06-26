// SPDX-License-Identifier: Apache-2.0
//! `wenlan recall <query>` — POST /api/context (knowledge bundle).

use anyhow::Result;
use wenlan_types::responses::KnowledgeContext;

use crate::client::WenlanClient;
use crate::output::{print_json, OutputFormat};

pub async fn run(
    client: &WenlanClient,
    format: OutputFormat,
    quiet: bool,
    query: String,
) -> Result<()> {
    let ctx = client.context(query).await?;
    if quiet {
        return Ok(());
    }
    match format {
        OutputFormat::Json => print_json(&ctx)?,
        OutputFormat::Table => print_table(&ctx),
        OutputFormat::Auto => unreachable!("Auto resolved by main before dispatch"),
    }
    Ok(())
}

fn print_table(ctx: &KnowledgeContext) {
    let pages = ctx.pages.len();
    let decisions = ctx.decisions.len();
    let memories = ctx.relevant_memories.len();
    let graph = ctx.graph_context.len();
    if pages == 0 && decisions == 0 && memories == 0 && graph == 0 {
        println!("(empty knowledge bundle)");
        return;
    }
    println!(
        "Knowledge: {} page(s), {} decision(s), {} memor{}, {} graph fact(s)",
        pages,
        decisions,
        memories,
        if memories == 1 { "y" } else { "ies" },
        graph,
    );
    if !ctx.relevant_memories.is_empty() {
        println!("Top memories:");
        for r in ctx.relevant_memories.iter().take(5) {
            let title: &str = if r.title.is_empty() {
                r.content.lines().next().unwrap_or("(no title)")
            } else {
                &r.title
            };
            let title_disp = if title.chars().count() > 60 {
                format!("{}...", title.chars().take(57).collect::<String>())
            } else {
                title.to_string()
            };
            println!("  [{:.3}] {} ({})", r.score, title_disp, r.source_id);
        }
    }
    if !ctx.pages.is_empty() {
        println!("Pages:");
        for p in ctx.pages.iter().take(5) {
            let line = p.lines().next().unwrap_or("");
            let disp = if line.chars().count() > 70 {
                format!("{}...", line.chars().take(67).collect::<String>())
            } else {
                line.to_string()
            };
            println!("  {}", disp);
        }
    }
}
