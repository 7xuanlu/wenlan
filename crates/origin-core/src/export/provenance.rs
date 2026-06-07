// SPDX-License-Identifier: Apache-2.0
//! Provenance projection helpers: delimiter-owned Sources block, the shared
//! ingress canonicalizer, and `_sources/` stub projection + GC.

/// Opening delimiter for the export-only `## Sources` block. The block is
/// generated from DB truth at projection time and stripped at ingress; it is
/// NEVER part of canonical `Page.content`.
pub const SOURCES_BLOCK_START: &str = "<!-- origin:sources:start -->";
/// Closing delimiter for the export-only Sources block.
pub const SOURCES_BLOCK_END: &str = "<!-- origin:sources:end -->";

/// Strip ONLY the delimiter-owned Sources block from a page body. A user may
/// legitimately type a `## Sources` heading or a `[[mem_123]]` wikilink in
/// prose; neither is touched. Removes the first `START..END` span (inclusive
/// of both delimiters and any trailing newline before START) and returns the
/// remainder trimmed of trailing whitespace. If the delimiters are absent or
/// malformed (END before START, or START with no END), the body is returned
/// trimmed but otherwise untouched.
pub fn canonicalize_page_body(body: &str) -> String {
    let start = match body.find(SOURCES_BLOCK_START) {
        Some(i) => i,
        None => return body.trim_end().to_string(),
    };
    let after_start = start + SOURCES_BLOCK_START.len();
    let end_rel = match body[after_start..].find(SOURCES_BLOCK_END) {
        Some(i) => i,
        None => return body.trim_end().to_string(),
    };
    let end = after_start + end_rel + SOURCES_BLOCK_END.len();
    // Drop whitespace/newlines immediately preceding the block so a fresh
    // projection (body + "\n\n" + block) canonicalizes back to bare body.
    let head = body[..start].trim_end();
    let tail = body[end..].trim_start();
    let mut out = String::from(head);
    if !tail.is_empty() {
        out.push_str("\n\n");
        out.push_str(tail);
    }
    out.trim_end().to_string()
}

/// Encode `s` as a YAML-safe double-quoted scalar. A JSON string literal is
/// a valid YAML flow scalar, so serde_json handles quotes/backslashes/control
/// chars correctly. Falls back to a naive quote only if JSON encoding fails
/// (it never does for a String).
pub(crate) fn yaml_quoted(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| format!("\"{}\"", s.replace('"', "'")))
}

/// Render the export-only `## Sources` block from a page's cited memory ids.
/// Returns the empty string when there are no sources (source-less pages get
/// no block). The block is wrapped in the delimiters so the ingress
/// canonicalizer can strip it exactly.
pub fn render_sources_block(source_memory_ids: &[String]) -> String {
    if source_memory_ids.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str(SOURCES_BLOCK_START);
    out.push_str("\n## Sources\n");
    for id in source_memory_ids {
        out.push_str(&format!("- [[{id}]]\n"));
    }
    out.push_str(SOURCES_BLOCK_END);
    out.push('\n');
    out
}

/// Render the read-only `sources:` frontmatter line (quoted wikilinks, which
/// Obsidian requires for list properties). Empty string when no sources.
/// PROJECTION-OUT ONLY — the watcher never reads this back.
// ids are `mem_*`-shaped (no YAML-specials), but they still route through
// `yaml_quoted` for uniform safety — unlike `related_frontmatter`, which takes
// untrusted free-text titles where escaping is load-bearing.
pub fn sources_frontmatter(source_memory_ids: &[String]) -> String {
    if source_memory_ids.is_empty() {
        return String::new();
    }
    let quoted: Vec<String> = source_memory_ids
        .iter()
        .map(|id| yaml_quoted(&format!("[[{id}]]")))
        .collect();
    format!("sources: [{}]\n", quoted.join(", "))
}

/// Render the read-only `related:` frontmatter line from page→page wikilink
/// targets. Empty string when there are none.
pub fn related_frontmatter(related_titles: &[String]) -> String {
    if related_titles.is_empty() {
        return String::new();
    }
    let quoted: Vec<String> = related_titles
        .iter()
        .map(|t| yaml_quoted(&format!("[[{t}]]")))
        .collect();
    format!("related: [{}]\n", quoted.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_only_the_delimiter_block() {
        let body = format!(
            "## Overview\nReal prose here.\n\n{SOURCES_BLOCK_START}\n## Sources\n- [[mem_1]]\n{SOURCES_BLOCK_END}\n"
        );
        let canon = canonicalize_page_body(&body);
        assert_eq!(canon, "## Overview\nReal prose here.");
        assert!(!canon.contains(SOURCES_BLOCK_START));
        assert!(!canon.contains("[[mem_1]]"));
    }

    #[test]
    fn user_typed_mem_wikilink_in_prose_survives() {
        // No delimiter block at all — a bare `[[mem_123]]` the user wrote.
        let body = "I cited [[mem_123]] in my own note.\n\n## Sources\nhand-written, not the daemon block.";
        let canon = canonicalize_page_body(body);
        assert_eq!(canon, body.trim_end());
        assert!(canon.contains("[[mem_123]]"));
        assert!(canon.contains("## Sources"));
    }

    #[test]
    fn missing_end_delimiter_leaves_body_untouched() {
        let body = format!("prose\n\n{SOURCES_BLOCK_START}\n## Sources\n- [[mem_1]]\n");
        let canon = canonicalize_page_body(&body);
        // No END → no strip; only trailing whitespace trimmed.
        assert!(canon.contains(SOURCES_BLOCK_START));
        assert!(canon.contains("[[mem_1]]"));
    }

    #[test]
    fn preserves_prose_after_the_block() {
        let body =
            format!("head prose\n\n{SOURCES_BLOCK_START}\nx\n{SOURCES_BLOCK_END}\n\ntail prose");
        let canon = canonicalize_page_body(&body);
        assert_eq!(canon, "head prose\n\ntail prose");
    }

    #[test]
    fn render_sources_block_is_delimiter_wrapped_and_canonicalizes_to_empty() {
        let ids = ["mem_1".to_string(), "mem_2".to_string()];
        let block = render_sources_block(&ids);
        assert!(block.starts_with(SOURCES_BLOCK_START));
        assert!(block.trim_end().ends_with(SOURCES_BLOCK_END));
        assert!(block.contains("## Sources"));
        assert!(block.contains("[[mem_1]]"));
        assert!(block.contains("[[mem_2]]"));
        // A body that is exactly the block canonicalizes to empty.
        assert_eq!(canonicalize_page_body(&block), "");
    }

    #[test]
    fn render_sources_block_empty_for_no_sources() {
        let ids: [String; 0] = [];
        assert_eq!(render_sources_block(&ids), String::new());
    }

    #[test]
    fn sources_frontmatter_quotes_wikilinks() {
        let ids = ["mem_1".to_string(), "mem_2".to_string()];
        let fm = sources_frontmatter(&ids);
        assert_eq!(fm, "sources: [\"[[mem_1]]\", \"[[mem_2]]\"]\n");
    }

    #[test]
    fn sources_frontmatter_empty_emits_nothing() {
        let ids: [String; 0] = [];
        assert_eq!(sources_frontmatter(&ids), String::new());
    }

    #[test]
    fn related_frontmatter_quotes_page_titles() {
        let titles = ["Other Page".to_string()];
        let fm = related_frontmatter(&titles);
        assert_eq!(fm, "related: [\"[[Other Page]]\"]\n");
    }

    #[test]
    fn related_frontmatter_escapes_titles_to_valid_yaml() {
        let titles = ["My \"Quoted\" Page".to_string()];
        let fm = related_frontmatter(&titles);
        // The emitted frontmatter block must parse as valid YAML (no map collapse).
        let yaml = format!("title: x\n{fm}");
        let parsed: serde_yaml::Value = serde_yaml::from_str(&yaml)
            .expect("frontmatter with a quote-bearing title must be valid YAML");
        // And the related entry must round-trip to the original wikilink target.
        let related = parsed
            .get("related")
            .and_then(|v| v.as_sequence())
            .expect("related seq");
        assert_eq!(related[0].as_str().unwrap(), "[[My \"Quoted\" Page]]");
    }
}
