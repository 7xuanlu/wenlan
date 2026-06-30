// SPDX-License-Identifier: Apache-2.0
//! `wenlan curate` — walk pending revisions (conflicts / merges) from the daemon.
//!
//! CLI-first replacement for the `/curate` skill's deferred-MCP round-trips: the
//! skill Bashes `wenlan --format json curate` to list, then
//! `wenlan curate accept|dismiss <target_source_id>` to act, so it pays no
//! ToolSearch hop. Scope is revisions only (the attention-gated surface that
//! actually needs a human); pending captures stay on the rare opt-in MCP path.

use anyhow::Result;
use serde::Serialize;
use wenlan_types::responses::PendingRevisionItem;

use crate::client::WenlanClient;
use crate::output::{print_json, OutputFormat};

/// One logical revision: all per-chunk rows sharing a `revision_source_id`, joined.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GroupedRevision {
    /// The memory this revision would replace; the accept/dismiss action key.
    pub target_source_id: String,
    /// The staged revision row id (kept for diagnostics / round-tripping).
    pub revision_source_id: String,
    /// Full revision text, chunks joined in order.
    pub content: String,
    pub source_agent: Option<String>,
}

#[derive(clap::Subcommand, Debug)]
pub enum CurateAction {
    /// List pending revisions (the default when no action is given).
    Revisions {
        /// Max revisions to fetch.
        #[arg(short, long, default_value_t = 20)]
        limit: usize,
    },
    /// Accept a revision: replace the original memory with the revised text.
    Accept {
        /// `target_source_id` of the revision (from the list output).
        target_source_id: String,
    },
    /// Dismiss a revision: drop it, keep the original memory.
    Dismiss {
        /// `target_source_id` of the revision (from the list output).
        target_source_id: String,
    },
}

const DEFAULT_LIMIT: usize = 20;

pub async fn run(
    client: &WenlanClient,
    format: OutputFormat,
    quiet: bool,
    action: Option<CurateAction>,
) -> Result<()> {
    match action.unwrap_or(CurateAction::Revisions {
        limit: DEFAULT_LIMIT,
    }) {
        CurateAction::Revisions { limit } => list_revisions(client, format, quiet, limit).await,
        CurateAction::Accept { target_source_id } => {
            let resp = client.accept_revision(&target_source_id).await?;
            if quiet {
                return Ok(());
            }
            match format {
                OutputFormat::Json => print_json(&resp)?,
                OutputFormat::Table => println!(
                    "{} revision for {}",
                    if resp.wrote { "accepted" } else { "no-op" },
                    resp.target_source_id
                ),
                OutputFormat::Auto => unreachable!("Auto resolved by main before dispatch"),
            }
            Ok(())
        }
        CurateAction::Dismiss { target_source_id } => {
            let resp = client.dismiss_revision(&target_source_id).await?;
            if quiet {
                return Ok(());
            }
            match format {
                OutputFormat::Json => print_json(&resp)?,
                OutputFormat::Table => println!(
                    "{} revision for {}",
                    if resp.wrote { "dismissed" } else { "no-op" },
                    resp.target_source_id
                ),
                OutputFormat::Auto => unreachable!("Auto resolved by main before dispatch"),
            }
            Ok(())
        }
    }
}

async fn list_revisions(
    client: &WenlanClient,
    format: OutputFormat,
    quiet: bool,
    limit: usize,
) -> Result<()> {
    let items = client.list_pending_revisions(limit).await?;
    let groups = group_revisions(items);
    if quiet {
        return Ok(());
    }
    match format {
        OutputFormat::Json => print_json(&groups)?,
        OutputFormat::Table => print_table(&groups),
        OutputFormat::Auto => unreachable!("Auto resolved by main before dispatch"),
    }
    Ok(())
}

fn print_table(groups: &[GroupedRevision]) {
    if groups.is_empty() {
        println!("(no pending revisions — nothing needs you)");
        return;
    }
    println!(
        "{} pending revision{}",
        groups.len(),
        if groups.len() == 1 { "" } else { "s" }
    );
    for g in groups {
        let snippet = if g.content.chars().count() > 100 {
            format!("{}...", g.content.chars().take(97).collect::<String>())
        } else {
            g.content.clone()
        };
        let agent = g.source_agent.as_deref().unwrap_or("daemon");
        println!("  {}  ·  ({})  {}", g.target_source_id, agent, snippet);
    }
    println!("accept/dismiss: wenlan curate accept|dismiss <target_source_id>");
}

/// Group per-chunk `PendingRevisionItem` rows into one logical revision each.
///
/// `list_pending_revisions` returns one row per chunk; a long revision spans
/// several rows sharing the same `revision_source_id`. Join their content in the
/// order received (the daemon sorts by `last_modified DESC`) so each card is ONE
/// logical revision, not a mid-sentence fragment.
fn group_revisions(items: Vec<PendingRevisionItem>) -> Vec<GroupedRevision> {
    use std::collections::HashMap;
    let mut order: Vec<String> = Vec::new();
    let mut by_rev: HashMap<String, GroupedRevision> = HashMap::new();
    for it in items {
        let piece = it.revision_content.trim();
        match by_rev.get_mut(&it.revision_source_id) {
            Some(g) => {
                if !piece.is_empty() {
                    if !g.content.is_empty() {
                        g.content.push(' ');
                    }
                    g.content.push_str(piece);
                }
            }
            None => {
                order.push(it.revision_source_id.clone());
                by_rev.insert(
                    it.revision_source_id.clone(),
                    GroupedRevision {
                        target_source_id: it.target_source_id,
                        revision_source_id: it.revision_source_id,
                        content: piece.to_string(),
                        source_agent: it.source_agent,
                    },
                );
            }
        }
    }
    order
        .into_iter()
        .filter_map(|k| by_rev.remove(&k))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(target: &str, rev: &str, content: &str) -> PendingRevisionItem {
        PendingRevisionItem {
            target_source_id: target.to_string(),
            revision_source_id: rev.to_string(),
            revision_content: content.to_string(),
            source_agent: None,
            last_modified: 0,
        }
    }

    #[test]
    fn empty_input_yields_no_groups() {
        assert!(group_revisions(vec![]).is_empty());
    }

    #[test]
    fn single_chunk_is_one_group_trimmed() {
        let groups = group_revisions(vec![item("mem_a", "rev_1", "  hello world  ")]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].target_source_id, "mem_a");
        assert_eq!(groups[0].content, "hello world");
    }

    #[test]
    fn multi_chunk_same_revision_joins_in_order() {
        let groups = group_revisions(vec![
            item("mem_a", "rev_1", "first part"),
            item("mem_a", "rev_1", "second part"),
        ]);
        assert_eq!(groups.len(), 1, "two chunks of one revision = one group");
        assert_eq!(groups[0].content, "first part second part");
    }

    #[test]
    fn distinct_revisions_stay_separate_in_first_seen_order() {
        let groups = group_revisions(vec![
            item("mem_b", "rev_2", "bee"),
            item("mem_a", "rev_1", "ay"),
        ]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].revision_source_id, "rev_2");
        assert_eq!(groups[1].revision_source_id, "rev_1");
    }
}
