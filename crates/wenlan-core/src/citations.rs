// SPDX-License-Identifier: Apache-2.0
//! Per-claim citation numbering, marker parsing, and union-calibrated
//! verification (pure functions), plus the annotate-only backfill sweep that
//! drains legacy (`citations IS NULL`) pages. See
//! `docs/superpowers/specs/2026-07-03-per-claim-citations-design.md`.

use std::sync::Arc;

use wenlan_types::pages::PageCitation;

use crate::db::MemoryDB;
use crate::llm_provider::{LlmProvider, LlmRequest};
use crate::prompts::PromptRegistry;
use crate::WenlanError;

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

/// Max legacy pages annotated per sweep tick.
const BACKFILL_BATCH_SIZE: usize = 5;
/// Consecutive guard-rejected annotate attempts before a page is poison-pilled
/// (`citations = '[]'`, changelog notes the giveup).
const MAX_ANNOTATE_ATTEMPTS: i64 = 3;
/// Changelog cap, matching `post_write.rs`'s `DEFAULT_CHANGELOG_CAP`.
const CHANGELOG_CAP: usize = 20;

/// `app_metadata` key tracking consecutive guard-rejected attempts for a page.
fn attempt_key(page_id: &str) -> String {
    format!("citation_backfill_attempts:{page_id}")
}

/// Collapse all whitespace runs to a single space and trim. Used by the
/// annotate-only guard to compare the model output against the input body
/// independent of incidental whitespace reflow.
fn normalize_ws(s: &str) -> String {
    let re = regex::Regex::new(r"\s+").expect("static regex");
    re.replace_all(s.trim(), " ").to_string()
}

/// Build a changelog entry for the annotate-only sweep and append it to the
/// page's existing changelog (best-effort: a read/parse failure falls back to
/// a single-entry array rather than losing the write).
async fn build_backfill_changelog(
    db: &MemoryDB,
    page_id: &str,
    version: i64,
    citations_summary: &str,
) -> String {
    let entry = serde_json::json!({
        "version": version,
        "at": chrono::Utc::now().timestamp(),
        "edited_by": "citation_backfill",
        "citations_summary": citations_summary,
    });
    let existing = db
        .get_page_changelog(page_id)
        .await
        .unwrap_or_else(|_| "[]".to_string());
    crate::db::append_changelog_entry(&existing, entry, CHANGELOG_CAP)
        .unwrap_or_else(|_| "[]".to_string())
}

/// Annotate-only backfill sweep: pick up to `BACKFILL_BATCH_SIZE` active pages
/// with `citations IS NULL`, insert `[N]` markers against their memory-kind
/// evidence, and save. A deterministic prose guard (marker-stripped output
/// must whitespace-normalize-equal the input body) rejects any output that
/// rewrites text; 3 consecutive rejections poison-pill the page to
/// `citations = '[]'` so the sweep doesn't retry it forever (it regains
/// citations naturally at its next growth re-distill).
pub async fn run_citation_backfill_tick(
    db: &MemoryDB,
    llm: &Arc<dyn LlmProvider>,
    prompts: &PromptRegistry,
) -> Result<(), WenlanError> {
    for page_id in db.get_pages_missing_citations(BACKFILL_BATCH_SIZE).await? {
        let Some(page) = db.get_page(&page_id).await? else {
            continue;
        };

        let evidence = db.get_page_evidence(&page_id).await.unwrap_or_default();
        let locators: Vec<String> = evidence
            .iter()
            .filter(|e| e.source_kind == "memory")
            .filter_map(|e| e.locator.clone())
            .collect();
        if locators.is_empty() {
            let changelog = build_backfill_changelog(
                db,
                &page_id,
                page.version,
                "citation backfill gave up: no memory evidence",
            )
            .await;
            let _ = db
                .set_page_citations_with_changelog(&page_id, Some("[]"), &changelog)
                .await;
            continue;
        }

        let mems = db.get_memories_by_source_ids(&locators).await?;
        let numbered: Vec<NumberedSource> = mems
            .iter()
            .enumerate()
            .map(|(i, m)| NumberedSource {
                index: (i + 1) as u32,
                source_kind: "memory".to_string(),
                locator: m.source_id.clone(),
                text: m.content.chars().take(SOURCE_TEXT_CAP).collect(),
            })
            .collect();

        let user_prompt = format!(
            "## Page Body\n{}\n\n## Numbered Sources\n{}",
            page.content,
            build_numbered_block(&numbered)
        );
        let raw = llm
            .generate(LlmRequest {
                system_prompt: Some(prompts.annotate_citations.clone()),
                user_prompt,
                max_tokens: llm.recommended_max_output(),
                temperature: 0.0,
                label: Some("citation_annotate".to_string()),
                timeout_secs: None,
            })
            .await
            .map_err(|e| WenlanError::Llm(e.to_string()))?;
        let out = crate::llm_provider::strip_think_tags(&raw)
            .trim()
            .to_string();

        // ⚖ deterministic prose guard: markers-stripped output must equal the
        // input body (whitespace-normalized) — legacy prose is never changed
        // by this sweep.
        let same =
            normalize_ws(&strip_markers(&out)) == normalize_ws(&strip_markers(&page.content));
        if same {
            let (body, cites, stats) = process_citation_output(&out, &numbered);
            let json = serde_json::to_string(&cites).unwrap_or_else(|_| "[]".into());
            let changelog =
                build_backfill_changelog(db, &page_id, page.version + 1, &stats.summary()).await;
            let _ = db
                .try_update_page_content_with_changelog(
                    &page_id,
                    &body,
                    &[],
                    "citation_backfill",
                    false,
                    &changelog,
                    Some(&json),
                )
                .await;
            let _ = db.set_app_metadata(&attempt_key(&page_id), "0").await;
        } else {
            let attempts = db
                .get_app_metadata(&attempt_key(&page_id))
                .await?
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0)
                + 1;
            if attempts >= MAX_ANNOTATE_ATTEMPTS {
                let changelog = build_backfill_changelog(
                    db,
                    &page_id,
                    page.version,
                    "citation backfill gave up: annotate guard rejected 3x",
                )
                .await;
                let _ = db
                    .set_page_citations_with_changelog(&page_id, Some("[]"), &changelog)
                    .await;
                let _ = db.set_app_metadata(&attempt_key(&page_id), "0").await;
            } else {
                let _ = db
                    .set_app_metadata(&attempt_key(&page_id), &attempts.to_string())
                    .await;
                log::info!(
                    "[citation_backfill] page {page_id} guard rejected (attempt {attempts})"
                );
            }
        }
    }
    Ok(())
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

    // -- Task 7: annotate-only backfill tick --

    use crate::llm_provider::{LlmProvider, MockProvider};
    use crate::prompts::PromptRegistry;
    use std::sync::Arc;

    /// Insert a bare `memories` row so `get_memories_by_source_ids` can find it.
    /// Mirrors the raw-insert pattern used by `synthesis::distill` tests.
    async fn insert_test_memory(db: &crate::db::MemoryDB, source_id: &str, content: &str) {
        let now_ts = chrono::Utc::now().timestamp();
        let conn = db.conn.lock().await;
        conn.execute(
            "INSERT INTO memories (id, source_id, title, content, chunk_index, chunk_type, memory_type, space, source_agent, created_at, last_modified, confirmed, stability, source) \
             VALUES (?1, ?1, ?1, ?2, 0, 'text', 'fact', NULL, 'claude-code', ?3, ?3, 1, 'confirmed', 'memory')",
            libsql::params![source_id.to_string(), content.to_string(), now_ts],
        )
        .await
        .unwrap();
    }

    const BACKFILL_BODY: &str = "The daemon binds to port 7878 by default.";
    const BACKFILL_MEM_CONTENT: &str = "The daemon binds to port 7878 by default";

    async fn seed_backfill_page(db: &crate::db::MemoryDB, page_id: &str, with_evidence: bool) {
        let now = chrono::Utc::now().to_rfc3339();
        db.insert_page(page_id, "T", None, BACKFILL_BODY, None, None, &[], &now)
            .await
            .unwrap();
        if with_evidence {
            insert_test_memory(db, "mem_a", BACKFILL_MEM_CONTENT).await;
            db.link_page_evidence(page_id, "memory", Some("mem_a"), None, "test")
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn backfill_happy_path_saves_citations_body_unchanged_prose() {
        let (db, _dir) = crate::db::tests::test_db().await;
        seed_backfill_page(&db, "p_happy", true).await;

        let annotated = format!("{BACKFILL_BODY}[1]");
        let llm: Arc<dyn LlmProvider> = Arc::new(MockProvider::new(&annotated));
        let prompts = PromptRegistry::default();

        run_citation_backfill_tick(&db, &llm, &prompts)
            .await
            .unwrap();

        let page = db.get_page("p_happy").await.unwrap().unwrap();
        assert_eq!(page.content, annotated, "annotated body should be saved");
        assert_eq!(page.citations.len(), 1, "citations: {:?}", page.citations);
        assert_eq!(page.citations[0].status, "verified");
        assert!(
            !db.get_pages_missing_citations(10)
                .await
                .unwrap()
                .contains(&"p_happy".to_string()),
            "page should no longer be citations-missing"
        );
    }

    #[tokio::test]
    async fn backfill_guard_rejects_rewritten_prose_and_records_attempt() {
        let (db, _dir) = crate::db::tests::test_db().await;
        seed_backfill_page(&db, "p_guard", true).await;

        let rewritten = "A completely different sentence about something else.[1]";
        let llm: Arc<dyn LlmProvider> = Arc::new(MockProvider::new(rewritten));
        let prompts = PromptRegistry::default();

        run_citation_backfill_tick(&db, &llm, &prompts)
            .await
            .unwrap();

        let page = db.get_page("p_guard").await.unwrap().unwrap();
        assert_eq!(page.content, BACKFILL_BODY, "prose must never be rewritten");
        assert!(page.citations.is_empty());
        assert!(
            db.get_pages_missing_citations(10)
                .await
                .unwrap()
                .contains(&"p_guard".to_string()),
            "citations should still be NULL (not processed)"
        );
        let attempts = db.get_app_metadata(&attempt_key("p_guard")).await.unwrap();
        assert_eq!(attempts.as_deref(), Some("1"));
    }

    #[tokio::test]
    async fn backfill_poison_pill_after_three_guard_rejections() {
        let (db, _dir) = crate::db::tests::test_db().await;
        seed_backfill_page(&db, "p_poison", true).await;

        let rewritten = "A completely different sentence about something else.[1]";
        let llm: Arc<dyn LlmProvider> = Arc::new(MockProvider::new(rewritten));
        let prompts = PromptRegistry::default();

        for _ in 0..3 {
            run_citation_backfill_tick(&db, &llm, &prompts)
                .await
                .unwrap();
        }

        let page = db.get_page("p_poison").await.unwrap().unwrap();
        assert_eq!(page.content, BACKFILL_BODY, "prose must never be rewritten");
        assert!(page.citations.is_empty());
        assert!(
            !db.get_pages_missing_citations(10)
                .await
                .unwrap()
                .contains(&"p_poison".to_string()),
            "citations should be '[]' (gave up), not NULL"
        );
        let log = db.get_page_changelog("p_poison").await.unwrap();
        assert!(
            log.contains("citation backfill gave up"),
            "changelog: {log}"
        );
        let attempts = db.get_app_metadata(&attempt_key("p_poison")).await.unwrap();
        assert_eq!(
            attempts.as_deref(),
            Some("0"),
            "attempt key must be cleared"
        );
    }

    #[tokio::test]
    async fn backfill_no_evidence_page_gives_up_without_llm_call() {
        let (db, _dir) = crate::db::tests::test_db().await;
        seed_backfill_page(&db, "p_noevidence", false).await;

        // An unavailable provider errors on every call; if the tick tried to
        // call it, the whole tick would return Err and this test would fail.
        let llm: Arc<dyn LlmProvider> = Arc::new(MockProvider::unavailable());
        let prompts = PromptRegistry::default();

        run_citation_backfill_tick(&db, &llm, &prompts)
            .await
            .unwrap();

        let page = db.get_page("p_noevidence").await.unwrap().unwrap();
        assert!(page.citations.is_empty());
        assert!(
            !db.get_pages_missing_citations(10)
                .await
                .unwrap()
                .contains(&"p_noevidence".to_string()),
            "citations should be '[]' (gave up), not NULL"
        );
        let log = db.get_page_changelog("p_noevidence").await.unwrap();
        assert!(
            log.contains("citation backfill gave up"),
            "changelog: {log}"
        );
    }
}
