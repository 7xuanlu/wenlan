// SPDX-License-Identifier: Apache-2.0
//! Per-claim citation numbering, marker parsing, and union-calibrated
//! verification (pure functions). See
//! `docs/superpowers/specs/2026-07-03-per-claim-citations-design.md`.

use wenlan_types::pages::PageCitation;

/// Cap on source text length embedded in the numbered block, matching
/// `MEM_SNIPPET_CAP` in `synthesis/distill.rs`.
const SOURCE_TEXT_CAP: usize = 800;

/// One numbered source available for citation at distill time.
pub struct NumberedSource {
    pub index: u32,
    pub source_kind: String,
    pub locator: String,
    pub text: String,
}

/// Render the numbered source block fed to the LLM prompt: `"[1] text\n\n[2] text"`.
/// Source text is capped at `SOURCE_TEXT_CAP` chars (char-safe).
pub fn build_numbered_block(sources: &[NumberedSource]) -> String {
    sources
        .iter()
        .map(|s| {
            let capped: String = s.text.chars().take(SOURCE_TEXT_CAP).collect();
            format!("[{}] {}", s.index, capped)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Remove every `[N]` marker from body prose, collapsing the resulting
/// doubled whitespace.
pub fn strip_markers(body: &str) -> String {
    let marker_re = regex::Regex::new(r"\[\d+\]").expect("static regex");
    let stripped = marker_re.replace_all(body, "");
    let space_re = regex::Regex::new(r" {2,}").expect("static regex");
    space_re.replace_all(&stripped, " ").trim().to_string()
}

/// Per-body citation counts.
pub struct CitationStats {
    pub verified: usize,
    pub unverified: usize,
    pub stripped: usize,
}

impl CitationStats {
    pub fn summary(&self) -> String {
        format!(
            "{} verified, {} unverified, {} stripped",
            self.verified, self.unverified, self.stripped
        )
    }
}

/// Normalize raw LLM marker output: `[ 1 ]` -> `[1]`, `[1,3]` -> `[1][3]`.
fn normalize_markers(body: &str) -> String {
    let spaced_re = regex::Regex::new(r"\[\s*(\d+)\s*\]").expect("static regex");
    let normalized = spaced_re.replace_all(body, "[$1]");

    let comma_re = regex::Regex::new(r"\[(\d+(?:\s*,\s*\d+)+)\]").expect("static regex");
    comma_re
        .replace_all(&normalized, |caps: &regex::Captures| {
            caps[1]
                .split(',')
                .map(|n| format!("[{}]", n.trim()))
                .collect::<String>()
        })
        .into_owned()
}

/// Strip out-of-range markers (index 0 or > sources.len()), counting each
/// removal into `stripped`. Returns the cleaned body.
fn strip_out_of_range(body: &str, num_sources: usize, stripped: &mut usize) -> String {
    let marker_re = regex::Regex::new(r"\[(\d+)\]").expect("static regex");
    let mut out = String::with_capacity(body.len());
    let mut last_end = 0;
    for cap in marker_re.captures_iter(body) {
        let m = cap.get(0).expect("group 0 always present");
        let n: usize = cap[1].parse().unwrap_or(0);
        out.push_str(&body[last_end..m.start()]);
        if n >= 1 && n <= num_sources {
            out.push_str(m.as_str());
        } else {
            *stripped += 1;
        }
        last_end = m.end();
    }
    out.push_str(&body[last_end..]);
    out
}

/// Normalize markers, strip out-of-range ones, then score every remaining
/// marker occurrence per sentence against the union of its claim's cited
/// sources. Returns the (possibly marker-stripped) body, the per-occurrence
/// citation records in body order, and aggregate stats.
///
/// Sentence boundaries are computed on a marker-free "bare" copy of the
/// body: `split_sentences` requires the terminal punctuation to be directly
/// followed by whitespace, but a marker sits between them (`"claim.[1] Next"`).
/// Removing the marker restores that adjacency (`"claim. Next"`) while each
/// marker's removal position (recorded before it is dropped) still tells us
/// which sentence it belonged to.
pub fn process_citation_output(
    body: &str,
    sources: &[NumberedSource],
) -> (String, Vec<PageCitation>, CitationStats) {
    let normalized = normalize_markers(body);
    let mut stripped = 0usize;
    let clean_body = strip_out_of_range(&normalized, sources.len(), &mut stripped);

    let marker_re = regex::Regex::new(r"\[(\d+)\]").expect("static regex");
    let mut bare_body = String::with_capacity(clean_body.len());
    let mut marker_positions: Vec<(u32, usize)> = Vec::new();
    let mut last_end = 0;
    for cap in marker_re.captures_iter(&clean_body) {
        let m = cap.get(0).expect("group 0 always present");
        let n: u32 = cap[1].parse().unwrap_or(0);
        bare_body.push_str(&clean_body[last_end..m.start()]);
        marker_positions.push((n, bare_body.len()));
        last_end = m.end();
    }
    bare_body.push_str(&clean_body[last_end..]);

    // Sentence spans over the bare body, using the same delimiter
    // `faithfulness::split_sentences` splits on.
    let delim_re = regex::Regex::new(r"(?m)[.!?]+\s+").expect("static regex");
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut prev = 0;
    for m in delim_re.find_iter(&bare_body) {
        spans.push((prev, m.start()));
        prev = m.end();
    }
    spans.push((prev, bare_body.len()));

    let mut citations = Vec::new();
    let mut occurrence = 0u32;
    let mut verified = 0usize;
    let mut unverified = 0usize;

    let mut i = 0;
    while i < marker_positions.len() {
        let span_idx = spans
            .iter()
            .rposition(|s| s.0 <= marker_positions[i].1)
            .unwrap_or(0);
        let mut group = vec![marker_positions[i]];
        let mut j = i + 1;
        while j < marker_positions.len() {
            let next_span_idx = spans
                .iter()
                .rposition(|s| s.0 <= marker_positions[j].1)
                .unwrap_or(0);
            if next_span_idx != span_idx {
                break;
            }
            group.push(marker_positions[j]);
            j += 1;
        }

        let (span_start, span_end) = spans[span_idx];
        let sentence = bare_body[span_start..span_end].trim();
        let union: String = group
            .iter()
            .filter_map(|&(n, _)| sources.get((n - 1) as usize))
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let claim_verified = crate::faithfulness::overlap_fraction(sentence, &union) >= 0.5;
        if claim_verified {
            verified += group.len();
        } else {
            unverified += group.len();
        }

        for &(n, _) in &group {
            occurrence += 1;
            if let Some(src) = sources.get((n - 1) as usize) {
                let score = crate::faithfulness::overlap_fraction(sentence, &src.text);
                citations.push(PageCitation {
                    occurrence,
                    marker: n,
                    source_kind: src.source_kind.clone(),
                    locator: src.locator.clone(),
                    score,
                    status: if claim_verified {
                        "verified"
                    } else {
                        "unverified"
                    }
                    .to_string(),
                });
            }
        }

        i = j;
    }

    (
        clean_body,
        citations,
        CitationStats {
            verified,
            unverified,
            stripped,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn srcs() -> Vec<NumberedSource> {
        vec![
            NumberedSource {
                index: 1,
                source_kind: "memory".into(),
                locator: "mem_a".into(),
                text: "The daemon binds to port 7878 by default".into(),
            },
            NumberedSource {
                index: 2,
                source_kind: "memory".into(),
                locator: "mem_b".into(),
                text: "FastEmbed uses BGE-Base embeddings with 768 dimensions".into(),
            },
        ]
    }

    #[test]
    fn numbered_block_format() {
        let b = build_numbered_block(&srcs());
        assert!(b.starts_with("[1] The daemon"));
        assert!(b.contains("\n\n[2] FastEmbed"));
    }

    #[test]
    fn verified_claim_gets_citation() {
        let body = "The daemon binds to port 7878 by default.[1] Unrelated hallucinated claim about quantum computing.[2]";
        let (out, cites, stats) = process_citation_output(body, &srcs());
        assert_eq!(out, body); // in-range markers stay in the body
        assert_eq!(cites.len(), 2);
        assert_eq!(cites[0].status, "verified");
        assert_eq!(cites[0].locator, "mem_a");
        assert_eq!(cites[1].status, "unverified");
        assert_eq!(stats.verified, 1);
        assert_eq!(stats.unverified, 1);
    }

    #[test]
    fn out_of_range_marker_stripped() {
        let body = "A claim.[7] Another about the daemon port 7878 binding default.[1]";
        let (out, cites, stats) = process_citation_output(body, &srcs());
        assert!(!out.contains("[7]"));
        assert!(out.contains("[1]"));
        assert_eq!(cites.len(), 1);
        assert_eq!(stats.stripped, 1);
    }

    #[test]
    fn malformed_markers_normalized() {
        let body = "The daemon binds port 7878 default.[ 1 ] Embeddings use BGE-Base 768 dimensions FastEmbed.[1,2]";
        let (out, cites, _s) = process_citation_output(body, &srcs());
        assert!(out.contains("default.[1]"));
        assert!(out.contains("[1][2]"));
        assert_eq!(cites.len(), 3);
    }

    #[test]
    fn reused_marker_gets_per_occurrence_status() {
        let body =
            "The daemon binds to port 7878 by default.[1] Completely unrelated quantum claim.[1]";
        let (_o, cites, _s) = process_citation_output(body, &srcs());
        assert_eq!(cites.len(), 2);
        assert_eq!((cites[0].occurrence, &cites[0].status[..]), (1, "verified"));
        assert_eq!(
            (cites[1].occurrence, &cites[1].status[..]),
            (2, "unverified")
        );
    }

    #[test]
    fn multi_marker_claim_verified_against_union() {
        // Claim draws half its tokens from each source: fails each alone, passes the union.
        let body = "The daemon port 7878 uses BGE-Base embeddings with 768 dimensions.[1][2]";
        let (_o, cites, _s) = process_citation_output(body, &srcs());
        assert!(cites.iter().all(|c| c.status == "verified"));
        assert!(cites[0].score < 0.5 || cites[1].score < 0.5); // per-source audit scores can sit below the floor
    }

    #[test]
    fn strip_markers_removes_all() {
        assert_eq!(
            strip_markers("Claim one.[1] Claim two.[12]"),
            "Claim one. Claim two."
        );
        assert_eq!(strip_markers("No markers here."), "No markers here.");
    }

    #[test]
    fn zero_markers_yields_empty_records() {
        let (out, cites, stats) = process_citation_output("Plain body.", &srcs());
        assert_eq!(out, "Plain body.");
        assert!(cites.is_empty());
        assert_eq!(stats.verified + stats.unverified + stats.stripped, 0);
    }
}
