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
}
