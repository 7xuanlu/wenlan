//! Two-column PDF reading-order acceptance gate (decision-9 / §6 v1).
//!
//! This is the go/no-go gate for keeping `pdf_extract` as the v1 PDF text
//! extractor. Two real two-column academic papers (ResNet, BERT) are committed
//! under `tests/fixtures/pdf/`. For each we pin KNOWN consecutive sentence pairs
//! and assert the second sentence still appears shortly AFTER the first in the
//! extracted text — an ordered-text property, not just a text-volume floor.
//!
//! Why ordered-text and not just min-text: the classic two-column failure mode
//! (sort glyphs by y then x) interleaves the two columns line-by-line, producing
//! plenty of words but scrambled reading order. A min-text / presence gate passes
//! that garbage; an ordered-adjacency gate does not. The negative-control test
//! below proves the gate rejects scrambled-but-plentiful text.
//!
//! GREEN here = pdf_extract preserves reading order on these papers -> keep it as
//! v1. A documented RED (an ordered assertion failing) is the trigger to swap in
//! `pdfium-render` before shipping.

use std::fs;
use std::path::PathBuf;

/// Assert `second` appears after `first` within `max_gap` bytes of the end of
/// `first`, searching case-insensitively. Returns the gap (bytes between the end
/// of `first` and the start of `second`) on success.
///
/// All indexing happens on a single lowercased copy of the haystack so byte
/// offsets stay valid even though the fixtures contain multi-byte ligatures
/// (`ﬁ`, `ﬂ`); never mix a position computed on `to_lowercase()` with an index
/// into the original string.
fn ordered_adjacent(
    haystack: &str,
    first: &str,
    second: &str,
    max_gap: usize,
) -> Result<usize, String> {
    let hay = haystack.to_lowercase();
    let f = first.to_lowercase();
    let s = second.to_lowercase();

    let first_pos = hay
        .find(&f)
        .ok_or_else(|| format!("first sentence not found: {first:?}"))?;
    // `f` matched `hay` at `first_pos`, so `first_pos + f.len()` is a valid char
    // boundary in `hay`; slicing the lowercased haystack (never the original) is
    // safe despite multi-byte ligatures elsewhere in the text.
    let after = &hay[first_pos + f.len()..];
    let gap = after
        .find(&s)
        .ok_or_else(|| format!("second sentence not found after first: {second:?}"))?;

    if gap > max_gap {
        return Err(format!(
            "second sentence {gap} bytes after first (max_gap {max_gap})"
        ));
    }
    Ok(gap)
}

/// A pair of sentences that are consecutive in the paper's reading order.
/// Strings are taken verbatim from the whitespace-normalized extraction and
/// deliberately avoid ligatures / hyphenated line-breaks so they match exactly.
struct Pair {
    first: &'static str,
    second: &'static str,
    /// Max bytes allowed between the end of `first` and the start of `second`.
    /// Small enough that a full intruding column (hundreds of bytes) fails.
    max_gap: usize,
}

/// ResNet — "Deep Residual Learning for Image Recognition", arXiv:1512.03385.
fn resnet_pairs() -> Vec<Pair> {
    vec![
        // Abstract, consecutive sentences.
        Pair {
            first: "An ensemble of these residual nets achieves 3.57% error on the ImageNet test set.",
            second: "This result won the 1st place on the ILSVRC 2015",
            max_gap: 60,
        },
        // Introduction body (two-column region): consecutive sentences must stay
        // contiguous — a line-interleave scramble would splice right-column text
        // between them.
        Pair {
            first: "Deep convolutional neural networks [22, 21] have led to a series of breakthroughs for image",
            second: "Deep networks naturally integrate low/mid/high",
            max_gap: 80,
        },
    ]
}

/// BERT — "BERT: Pre-training of Deep Bidirectional Transformers for Language
/// Understanding", arXiv:1810.04805.
fn bert_pairs() -> Vec<Pair> {
    vec![
        // Introduction body (two-column region), consecutive sentences.
        Pair {
            first: "Language model pre-training has been shown to be effective for improving many natural language processing tasks",
            second: "These include sentence-level tasks such as natural language inference",
            max_gap: 130,
        },
        // Abstract, consecutive sentences.
        Pair {
            first: "BERT is conceptually simple and empirically powerful.",
            second: "It obtains new state-of-the-art",
            max_gap: 20,
        },
    ]
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/pdf")).join(name)
}

fn extract(name: &str) -> String {
    let bytes = fs::read(fixture_path(name)).expect("read fixture pdf");
    wenlan_core::sources::directory::extract_pdf_text(&bytes).expect("pdf extraction")
}

/// Run the ordered-text gate over one fixture: a min-text floor plus every known
/// consecutive pair staying ordered and adjacent.
fn assert_reading_order(paper: &str, file: &str, pairs: Vec<Pair>) {
    let text = extract(file);

    // Min-text floor: a real paper extracts thousands of words.
    let words = text.split_whitespace().count();
    assert!(
        words > 500,
        "{paper}: extraction too short ({words} words, expected > 500)"
    );

    // Ordered-text property: this is what separates the gate from a volume check.
    for pair in pairs {
        ordered_adjacent(&text, pair.first, pair.second, pair.max_gap).unwrap_or_else(|e| {
            panic!(
                "{paper}: reading order not preserved: {e}\n  first:  {:?}\n  second: {:?}",
                pair.first, pair.second
            )
        });
    }
}

/// Non-vacuity guard (encodes the task's RED): scrambled two-column reading order
/// must be REJECTED by the gate. Both sentences are present (a presence/min-text
/// gate would accept), but a full intruding column separates them, so the
/// ordered-adjacency gate must fail.
#[test]
fn ordered_gate_rejects_scrambled_reading_order() {
    let first = "An ensemble of these residual nets achieves 3.57% error on the ImageNet test set.";
    let second = "This result won the 1st place on the ILSVRC 2015";
    let intrusion = "unrelated column text ".repeat(40); // ~880 bytes of other-column glyphs
    let scrambled = format!("{first} {intrusion} {second}");

    assert!(
        ordered_adjacent(&scrambled, first, second, 200).is_err(),
        "ordered gate must reject scrambled reading order; a presence/min-text gate is insufficient"
    );
}

#[test]
fn pdf_ordered_text_resnet_preserves_reading_order() {
    assert_reading_order("ResNet", "resnet_1512.03385.pdf", resnet_pairs());
}

#[test]
fn pdf_ordered_text_bert_preserves_reading_order() {
    assert_reading_order("BERT", "bert_1810.04805.pdf", bert_pairs());
}
