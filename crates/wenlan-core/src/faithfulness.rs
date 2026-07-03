// SPDX-License-Identifier: Apache-2.0
//! Page faithfulness scoring logic (shared between eval bench and prod verifier).

const STOPWORDS: &[&str] = &[
    "with", "from", "that", "this", "these", "those", "have", "been", "will", "would", "could",
    "should", "their", "there", "where", "when", "what", "which", "while", "about", "after",
    "before", "between", "into", "over", "under", "very", "more", "most", "some", "such", "than",
    "then", "they", "them", "your", "yours",
];

/// Split a page body into sentences. Uses regex on terminal punctuation
/// followed by whitespace. Final sentence may not have trailing whitespace.
pub fn split_sentences(body: &str) -> Vec<&str> {
    let re = regex::Regex::new(r"(?m)[.!?]+\s+").expect("static regex");
    re.split(body).filter(|s| !s.trim().is_empty()).collect()
}

/// Extract content-bearing tokens from a sentence: lowercase, length >= 4,
/// excluding stopwords. Used for faithfulness overlap scoring.
pub fn content_tokens(sentence: &str) -> Vec<String> {
    sentence
        .split(|c: char| !c.is_alphanumeric())
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| t.len() >= 4 && !STOPWORDS.contains(&t.as_str()))
        .collect()
}

/// Fraction (0..=1) of the sentence's content tokens appearing as whole-word
/// matches in the source text. Zero content tokens => 1.0 (vacuously faithful).
pub fn overlap_fraction(sentence: &str, source: &str) -> f64 {
    let toks = content_tokens(sentence);
    if toks.is_empty() {
        return 1.0;
    }
    let lo_source = source.to_ascii_lowercase();
    let mut hits = 0usize;
    for t in &toks {
        let pattern = format!(r"\b{}\b", regex::escape(t));
        let found = regex::Regex::new(&pattern)
            .map(|re| re.is_match(&lo_source))
            .unwrap_or_else(|_| lo_source.contains(t.as_str()));
        if found {
            hits += 1;
        }
    }
    hits as f64 / toks.len() as f64
}

/// True if at least 50% of the sentence's content tokens appear in the source.
pub fn score_sentence_faithful(sentence: &str, source: &str) -> bool {
    overlap_fraction(sentence, source) >= 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_sentences_basic_punctuation() {
        let s = split_sentences("First sentence. Second sentence! Third question? Final.");
        assert_eq!(s.len(), 4);
    }

    #[test]
    fn content_tokens_strips_stopwords_and_short() {
        let toks = content_tokens("This is a Rust programming language with memory safety.");
        assert!(toks.contains(&"rust".to_string()));
        assert!(!toks.contains(&"this".to_string()));
    }

    #[test]
    fn overlap_fraction_exact_and_boundary() {
        assert_eq!(overlap_fraction("word", ""), 0.0);
        assert_eq!(overlap_fraction(".", "anything"), 1.0); // vacuous
                                                            // 2 of 4 content tokens present => exactly 0.5 => faithful
        let sent = "Rust provides memory safety guarantees.";
        let src = "rust ... memory ..."; // hits: rust, memory; misses: provides, safety, guarantees
        let f = overlap_fraction(sent, src);
        assert!(f > 0.0 && f < 0.5);
        assert!(!score_sentence_faithful(sent, src));
    }

    #[test]
    fn score_sentence_faithful_majority_overlap() {
        let sentence = "Rust provides memory safety guarantees.";
        let all = "Rust provides memory safety guarantees";
        assert!(score_sentence_faithful(sentence, all));
        assert!(!score_sentence_faithful(sentence, "Rust is great"));
    }
}
