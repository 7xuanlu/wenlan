// SPDX-License-Identifier: Apache-2.0
use super::traits::{ChunkContext, ChunkInfo, ChunkingStrategy};
use super::MAX_CONTENT_TOKENS;
use text_splitter::{ChunkConfig, MarkdownSplitter};
use tokenizers::Tokenizer;

/// Either splitter flavour: character-sized (fallback) or token-sized (BGE).
/// `MarkdownSplitter` is generic over its sizer, so the two monomorphizations
/// are distinct types; an enum lets `MarkdownStrategy` stay non-generic and
/// object-safe behind `Box<dyn ChunkingStrategy>`.
enum Splitter {
    Chars(MarkdownSplitter<text_splitter::Characters>),
    // Boxed: a `Tokenizer` carries its vocab maps, dwarfing the char variant.
    Tokens(Box<MarkdownSplitter<Tokenizer>>),
}

/// Markdown chunking strategy backed by `text-splitter` (which uses
/// `pulldown-cmark` for CommonMark parsing).
///
/// Advantages over the previous hand-rolled splitter:
/// - Understands every markdown block structure (headings, code blocks,
///   lists, tables, blockquotes) via pulldown-cmark semantic levels — not
///   just headings. A long list or table stays intact instead of getting
///   sliced mid-item.
/// - Never splits inside a fenced code block, inline code, or other inline
///   structure. Fewer edge-case bugs than hand-rolled state tracking.
/// - Byte-accurate offsets via `chunk_indices()`.
///
/// Built via [`MarkdownStrategy::with_tokenizer`] the splitter sizes chunks by
/// BGE token count (max [`MAX_CONTENT_TOKENS`]), eliminating silent truncation
/// during embedding. [`MarkdownStrategy::new`] falls back to a conservative
/// character-based cap when no tokenizer is available.
pub struct MarkdownStrategy {
    splitter: Splitter,
}

impl MarkdownStrategy {
    /// Conservative character-based max. 1500 chars is safely under BGE-Base's
    /// 512-token limit for typical English prose (~3 chars/token). Dense
    /// content (code, URLs, CJK) can be shorter per-token; the range-based
    /// capacity lets text-splitter pack chunks tighter when possible.
    ///
    /// This is only the fallback path (no tokenizer available). Prefer
    /// [`Self::with_tokenizer`], which caps by real token count.
    const MIN_CHARS: usize = 800;
    const MAX_CHARS: usize = 1500;

    pub fn new() -> Self {
        let config = ChunkConfig::new(Self::MIN_CHARS..Self::MAX_CHARS);
        Self {
            splitter: Splitter::Chars(MarkdownSplitter::new(config)),
        }
    }

    /// Token-aware constructor: chunks are sized so each encodes to at most
    /// [`MAX_CONTENT_TOKENS`] BGE tokens. The desired lower bound lets
    /// text-splitter keep larger semantic units (whole sections, tables)
    /// intact while never exceeding the token budget.
    pub fn with_tokenizer(tokenizer: Tokenizer) -> Self {
        let config =
            ChunkConfig::new((MAX_CONTENT_TOKENS / 2)..=MAX_CONTENT_TOKENS).with_sizer(tokenizer);
        Self {
            splitter: Splitter::Tokens(Box::new(MarkdownSplitter::new(config))),
        }
    }
}

impl Default for MarkdownStrategy {
    fn default() -> Self {
        Self::new()
    }
}

impl ChunkingStrategy for MarkdownStrategy {
    fn chunk(&self, context: ChunkContext) -> Vec<ChunkInfo> {
        let content = context.content;
        if content.trim().is_empty() {
            return Vec::new();
        }

        // The two splitter flavours return different `impl Iterator` types, so
        // collect the (offset, text) pairs inside the match before mapping.
        let indices: Vec<(usize, &str)> = match &self.splitter {
            Splitter::Chars(s) => s.chunk_indices(content).collect(),
            Splitter::Tokens(s) => s.chunk_indices(content).collect(),
        };

        indices
            .into_iter()
            .map(|(byte_offset, chunk_text)| {
                let semantic_unit = detect_semantic_unit(chunk_text);
                let end = byte_offset + chunk_text.len();
                ChunkInfo {
                    content: chunk_text.to_string(),
                    chunk_type: "markdown".to_string(),
                    language: None,
                    byte_range: Some((byte_offset, end)),
                    semantic_unit: Some(semantic_unit),
                }
            })
            .collect()
    }
}

/// Inspect the first non-blank line of a chunk and label it as `section_hN`
/// if it starts with a heading marker, else `section_h0` (continuation or
/// body-only chunk).
fn detect_semantic_unit(chunk: &str) -> String {
    for line in chunk.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        // Count leading '#' characters (up to 6)
        let level = trimmed.chars().take(7).take_while(|c| *c == '#').count();
        if (1..=6).contains(&level)
            && trimmed
                .chars()
                .nth(level)
                .is_some_and(|c| c.is_whitespace())
        {
            return format!("section_h{level}");
        }
        return "section_h0".to_string();
    }
    "section_h0".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_context<'a>(
        content: &'a str,
        metadata: &'a HashMap<String, String>,
    ) -> ChunkContext<'a> {
        ChunkContext {
            content,
            title: "Test",
            file_path: "test.md",
            metadata,
        }
    }

    #[test]
    fn test_simple_header_split() {
        let strategy = MarkdownStrategy::new();
        let text = "# Title\nContent here\n## Subtitle\nMore content";
        let chunks = strategy.chunk(make_context(text, &HashMap::new()));

        // Small content may stay as a single chunk since it's below MIN_CHARS.
        // Verify that whatever chunks we get have valid semantic units.
        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert_eq!(chunk.chunk_type, "markdown");
            assert!(chunk.semantic_unit.is_some());
        }
        // All content should be preserved across chunks
        let joined = chunks
            .iter()
            .map(|c| c.content.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(joined.contains("Title"));
        assert!(joined.contains("Subtitle"));
    }

    #[test]
    fn test_no_headers() {
        let strategy = MarkdownStrategy::new();
        let text = "Just plain markdown content without headers";
        let chunks = strategy.chunk(make_context(text, &HashMap::new()));

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].semantic_unit, Some("section_h0".to_string()));
        assert!(chunks[0].content.contains("plain markdown content"));
    }

    #[test]
    fn test_large_section_subdivision() {
        let strategy = MarkdownStrategy::new();
        // Create a realistic long prose section that exceeds MAX_CHARS.
        // Use distinct sentences so text-splitter has natural break points.
        let sentence = "This is a sentence of prose that describes something meaningful. ";
        let large = sentence.repeat(50); // ~3250 chars
        let text = format!("# Title\n\n{large}");
        let chunks = strategy.chunk(make_context(&text, &HashMap::new()));

        assert!(chunks.len() > 1, "should subdivide a 3000+ char section");
        for chunk in &chunks {
            assert!(
                chunk.content.len() <= MarkdownStrategy::MAX_CHARS + 100,
                "chunk exceeds max: {} chars",
                chunk.content.len()
            );
        }
    }

    #[test]
    fn test_chunk_type() {
        let strategy = MarkdownStrategy::new();
        let chunks = strategy.chunk(make_context("# Test\nContent", &HashMap::new()));
        assert_eq!(chunks[0].chunk_type, "markdown");
    }

    #[test]
    fn test_empty_content() {
        let strategy = MarkdownStrategy::new();
        let chunks = strategy.chunk(make_context("", &HashMap::new()));
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_whitespace_only_content() {
        let strategy = MarkdownStrategy::new();
        let chunks = strategy.chunk(make_context("   \n\n  \t  ", &HashMap::new()));
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_chunk_doesnt_split_inside_code_block() {
        let strategy = MarkdownStrategy::new();
        let text = "## Section\n\n```python\n# This is a comment\ndef hello():\n    pass\n```\n\nMore content";
        let chunks = strategy.chunk(make_context(text, &HashMap::new()));

        // The `# This is a comment` inside the code block is NOT a heading —
        // pulldown-cmark surfaces it inside a code block event, so the chunk
        // containing it should be labelled section_h2 (the outer heading)
        // or section_h0 (body continuation), never section_h1.
        for chunk in &chunks {
            if chunk.content.contains("def hello()") {
                let unit = chunk.semantic_unit.as_deref().unwrap_or("");
                assert_ne!(
                    unit, "section_h1",
                    "code block comment must not be misread as a heading"
                );
            }
        }
        // All the code should be preserved somewhere in the output
        let joined = chunks
            .iter()
            .map(|c| c.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("# This is a comment"));
        assert!(joined.contains("def hello()"));
    }

    #[test]
    fn test_byte_ranges_are_within_bounds() {
        let strategy = MarkdownStrategy::new();
        let text =
            "# First Section\n\nSome content here.\n\n## Second Section\n\nMore content here.";
        let chunks = strategy.chunk(make_context(text, &HashMap::new()));

        for chunk in &chunks {
            let (start, end) = chunk.byte_range.unwrap();
            assert!(
                start <= text.len(),
                "start {start} exceeds len {}",
                text.len()
            );
            assert!(end <= text.len(), "end {end} exceeds len {}", text.len());
            assert!(start <= end, "start {start} > end {end}");
        }
    }

    #[test]
    fn test_detect_semantic_unit_h1() {
        assert_eq!(detect_semantic_unit("# Title\nbody"), "section_h1");
    }

    #[test]
    fn test_detect_semantic_unit_h2() {
        assert_eq!(detect_semantic_unit("## Subtitle\nbody"), "section_h2");
    }

    #[test]
    fn test_detect_semantic_unit_h6() {
        assert_eq!(detect_semantic_unit("###### Deep\nbody"), "section_h6");
    }

    #[test]
    fn test_detect_semantic_unit_body_only() {
        assert_eq!(detect_semantic_unit("Just prose."), "section_h0");
    }

    #[test]
    fn test_detect_semantic_unit_leading_blank_lines() {
        assert_eq!(detect_semantic_unit("\n\n## Title\nbody"), "section_h2");
    }

    #[test]
    fn test_detect_semantic_unit_hashtag_not_heading() {
        // `#rust` is a hashtag, not a heading — no space after `#`.
        assert_eq!(detect_semantic_unit("#rust is cool"), "section_h0");
    }

    #[test]
    fn test_detect_semantic_unit_seven_hashes_not_heading() {
        // Markdown only allows h1-h6; 7+ hashes is not a heading.
        assert_eq!(detect_semantic_unit("####### Over-deep"), "section_h0");
    }

    /// Load the real BGE tokenizer from the shared FastEmbed cache, mirroring
    /// how `db::shared_embedder` locates the ONNX model. Returns `None` when the
    /// cache is absent (test then skips, same contract as the db tests that
    /// require the embedder to be present).
    fn bge_tokenizer() -> Option<tokenizers::Tokenizer> {
        let cache = crate::db::resolve_fastembed_cache_dir(std::path::Path::new(".nonexistent"))?;
        crate::chunker::load_bge_tokenizer(&cache)
    }

    /// A prose document of roughly 8k tokens.
    fn long_prose() -> String {
        "This is a sentence of prose that describes something meaningful and then keeps going. "
            .repeat(600)
    }

    /// A dense, token-heavy document: symbolic/code content tokenizes at close
    /// to one token per character, so a char-sized chunk blows past the token
    /// budget even though its character count looks safe.
    fn dense_document() -> String {
        "fn f(x:i32)->i32{let y=x*2+1;y-3/4%5;} a=[1,2,3];b={k:v};c<d>e|f&g^h~i; ".repeat(400)
    }

    /// Property: every chunk from the markdown strategy stays within the BGE
    /// token budget once the strategy is tokenizer-aware.
    #[test]
    fn markdown_chunks_within_token_budget() {
        let Some(tokenizer) = bge_tokenizer() else {
            eprintln!("SKIP markdown_chunks_within_token_budget: BGE tokenizer not in cache");
            return;
        };
        let strategy = MarkdownStrategy::with_tokenizer(tokenizer.clone());
        let prose = long_prose();
        let dense = dense_document();
        for doc in [prose.as_str(), dense.as_str()] {
            let chunks = strategy.chunk(make_context(doc, &HashMap::new()));
            assert!(!chunks.is_empty(), "expected at least one chunk");
            for chunk in &chunks {
                let n_tokens = tokenizer
                    .encode_fast(chunk.content.as_str(), false)
                    .expect("encode chunk")
                    .get_ids()
                    .len();
                assert!(
                    n_tokens <= crate::chunker::MAX_CONTENT_TOKENS,
                    "chunk encodes to {n_tokens} tokens (> {}); first 80 chars: {:?}",
                    crate::chunker::MAX_CONTENT_TOKENS,
                    chunk.content.chars().take(80).collect::<String>()
                );
            }
        }
    }
}
