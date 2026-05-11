// SPDX-License-Identifier: Apache-2.0
//! Wikilink extraction + resolution.
//!
//! Pages are written with `[[Label]]` link syntax. The distill prompt emits
//! them, users write them by hand. The daemon's job is to:
//!   1. Pull `[[Label]]` occurrences out of a page body.
//!   2. Resolve each label against current page titles (case-insensitive,
//!      trimmed). No fuzzy matching in v1 — the LLM's job is to use exact
//!      titles, and the orphan-by-label feed surfaces drift.
//!   3. Persist resolved/unresolved pairs into the `page_links` table so the
//!      refinery can re-resolve later (when a target page is created) and
//!      emergence can mine repeated orphan labels as topic candidates.
//!
//! The extraction regex deliberately rejects pipe-aliased links (`[[Target|alias]]`)
//! by capturing only the label segment; if Obsidian-style aliases become
//! load-bearing later we'll plumb the alias separately, but the link's
//! identity is the target label, not the visible text.

use crate::db::MemoryDB;
use crate::error::OriginError;
use std::collections::HashSet;

/// A `[[Label]]` reference parsed out of a page body. `target_page_id` is
/// `None` when the resolver couldn't find a matching page title — those rows
/// land in `page_links` with NULL target so the refinery's orphan resolve
/// phase can pick them up later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wikilink {
    pub label: String,
    pub target_page_id: Option<String>,
}

/// Pull every `[[...]]` occurrence out of `content`. Returns labels in the
/// order they appear, deduplicated case-insensitively (a label is the link's
/// identity, not its location — a page that mentions `[[Rust]]` four times
/// produces one outbound edge, not four). Whitespace inside the braces is
/// trimmed; empty labels (`[[]]`) are dropped.
///
/// The label is whatever sits between `[[` and the first `|` or `]]` — so
/// `[[Target|alias]]` extracts as `"Target"`.
pub fn extract_wikilinks(content: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let bytes = content.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'[' && bytes[i + 1] == b'[' {
            // Find the matching `]]`. UTF-8 safe because we only compare
            // bytes against ASCII `]` / `|`, never slice on a non-ASCII
            // boundary — the slice endpoints are always at ASCII positions.
            let start = i + 2;
            let mut j = start;
            let mut end = None;
            while j + 1 < bytes.len() {
                if bytes[j] == b']' && bytes[j + 1] == b']' {
                    end = Some(j);
                    break;
                }
                j += 1;
            }
            match end {
                Some(close) => {
                    // Stop at the first `|` so aliased links extract the
                    // target, not the display text.
                    let pipe = content[start..close].find('|');
                    let raw_label = match pipe {
                        Some(p) => &content[start..start + p],
                        None => &content[start..close],
                    };
                    let label = raw_label.trim().to_string();
                    if !label.is_empty() {
                        let key = label.to_lowercase();
                        if seen.insert(key) {
                            out.push(label);
                        }
                    }
                    i = close + 2;
                    continue;
                }
                None => break, // unmatched `[[` at end of input — bail
            }
        }
        i += 1;
    }
    out
}

/// Resolve each extracted label against the current `pages` table.
/// Case-insensitive exact match on title, restricted to `status='active'`.
/// Unresolved labels keep `target_page_id = None` so the caller can persist
/// them as orphan rows.
pub async fn resolve_against_pages(
    db: &MemoryDB,
    labels: &[String],
) -> Result<Vec<Wikilink>, OriginError> {
    let mut out = Vec::with_capacity(labels.len());
    for label in labels {
        let target = db.find_active_page_id_by_title(label).await?;
        out.push(Wikilink {
            label: label.clone(),
            target_page_id: target,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_single_link() {
        let v = extract_wikilinks("hello [[Rust Ownership]] world");
        assert_eq!(v, vec!["Rust Ownership"]);
    }

    #[test]
    fn extract_multiple_links_preserve_order() {
        let v = extract_wikilinks("[[Alpha]] then [[Beta]] then [[Gamma]]");
        assert_eq!(v, vec!["Alpha", "Beta", "Gamma"]);
    }

    #[test]
    fn extract_dedups_case_insensitively() {
        let v = extract_wikilinks("[[Origin]] and [[origin]] and [[ORIGIN]]");
        // Keep first-seen casing as the canonical label.
        assert_eq!(v, vec!["Origin"]);
    }

    #[test]
    fn extract_handles_aliased_links() {
        let v = extract_wikilinks("see [[Rust Ownership|borrowing rules]] for more");
        assert_eq!(v, vec!["Rust Ownership"]);
    }

    #[test]
    fn extract_drops_empty_labels() {
        let v = extract_wikilinks("[[]] and [[   ]] are noise");
        assert!(v.is_empty());
    }

    #[test]
    fn extract_ignores_unmatched_brackets() {
        // Unmatched `[[` at end of input must not panic or slice past the
        // buffer.
        let v = extract_wikilinks("legit [[Foo]] then trailing [[");
        assert_eq!(v, vec!["Foo"]);
    }

    #[test]
    fn extract_handles_utf8_payload() {
        let v = extract_wikilinks("café [[Naïve Bayes]] résumé");
        assert_eq!(v, vec!["Naïve Bayes"]);
    }

    #[test]
    fn extract_strips_inner_whitespace() {
        let v = extract_wikilinks("[[  Spaces  ]]");
        assert_eq!(v, vec!["Spaces"]);
    }

    #[test]
    fn extract_returns_empty_for_no_links() {
        let v = extract_wikilinks("plain prose with [no brackets] only");
        assert!(v.is_empty());
    }
}
