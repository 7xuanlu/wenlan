// SPDX-License-Identifier: Apache-2.0
//! Deterministic entity-name near-dedup primitives (T16, R13).
//!
//! Pure, LLM-free helpers used by the opt-in entity-resolution cascade
//! (`WENLAN_ENABLE_ENTITY_MINHASH`). Catch near-duplicate entity names
//! ("PostgreSQL"/"Postgres") at entity-creation time via char-shingle
//! MinHash/LSH banding + an exact Jaccard confirmation, gated by a Shannon-
//! entropy filter that punts short/low-entropy names (3-char acronyms) back to
//! the existing vector cascade.
//!
//! Determinism note: [`minhash_signature`] uses 32 *seeded* permutations driven
//! by an inline FNV-1a 64-bit hash (fixed public spec, no RNG, no new crate).
//! FNV-1a -- not `std::hash::DefaultHasher` -- is mandatory here because the
//! resulting band keys are PERSISTED to `entity_minhash_bands` as permanent
//! identifiers; std does not guarantee `DefaultHasher` output across Rust
//! releases, which would silently orphan pre-upgrade rows. FNV-1a is
//! version-stable, so the same shingle set always produces the same `[u64; 32]`
//! signature on every toolchain. Golden-value tests lock the algorithm.
//!
//! Module is `pub(crate)`; wired into `post_write::create_entity` and
//! `importer::resolve_entity_bulk`.

use std::collections::HashSet;

/// Number of MinHash permutations (signature length).
pub(crate) const MINHASH_PERMUTATIONS: usize = 32;
/// Rows per LSH band. 32 perms / 4 rows = 8 bands.
pub(crate) const MINHASH_BAND_SIZE: usize = 4;
/// Exact-Jaccard threshold for an *auto*-merge. Pairs at/above this fuse.
pub(crate) const FUZZY_JACCARD_THRESHOLD: f64 = 0.9;
/// Shannon-entropy floor for a name to be eligible for fuzzy dedup.
pub(crate) const NAME_ENTROPY_THRESHOLD: f64 = 1.5;
/// Minimum char count for a name to be eligible for fuzzy dedup.
pub(crate) const MIN_NAME_LEN_FOR_FUZZY: usize = 6;

/// Character shingle width (trigrams).
const SHINGLE_K: usize = 3;

/// Lowercased character k-shingles (trigrams by default) over `name`.
///
/// UTF-8 safe: operates on `chars()`, never byte indices. For names shorter
/// than `k` (after lowercasing) the whole name is returned as a single shingle
/// so short inputs still produce a non-empty set rather than panicking.
pub(crate) fn char_shingles(name: &str, k: usize) -> HashSet<String> {
    let chars: Vec<char> = name.to_lowercase().chars().collect();
    let mut out = HashSet::new();
    if chars.is_empty() {
        return out;
    }
    if chars.len() < k || k == 0 {
        out.insert(chars.iter().collect::<String>());
        return out;
    }
    for window in chars.windows(k) {
        out.insert(window.iter().collect::<String>());
    }
    out
}

/// FNV-1a 64-bit offset basis. FIXED SPEC -- do not change.
const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
/// FNV-1a 64-bit prime. FIXED SPEC -- do not change.
const FNV_PRIME: u64 = 0x100000001b3;

/// FNV-1a 64-bit hash over raw bytes, starting from an explicit initial state.
///
/// Why not `DefaultHasher`: std documents `DefaultHasher`/`SipHash` output as
/// "not guaranteed to be the same across Rust releases", but the band keys this
/// produces are PERSISTED to `entity_minhash_bands` as permanent identifiers.
/// A future toolchain could silently change the algorithm and orphan every row
/// written by an older binary (no error, no migration). FNV-1a is a fixed,
/// public spec (offset basis + prime above), so the byte->u64 mapping is stable
/// forever. The golden-value tests below lock it.
fn fnv1a64(init: u64, bytes: &[u8]) -> u64 {
    let mut hash = init;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Hash one shingle under a given permutation seed.
///
/// Determinism contract: 32 independent, *version-stable* hash families are
/// obtained by folding the permutation `seed` (8 little-endian bytes) into the
/// FNV-1a state before the shingle bytes. Same input -> same u64 across every
/// Rust release (FNV-1a is a fixed spec), which is required because these feed
/// the persisted band keys.
fn seeded_hash(seed: u64, shingle: &str) -> u64 {
    let state = fnv1a64(FNV_OFFSET_BASIS, &seed.to_le_bytes());
    fnv1a64(state, shingle.as_bytes())
}

/// Deterministic MinHash signature: per seed, the minimum seeded-hash over all
/// shingles. Empty shingle sets yield an all-`u64::MAX` signature (no shingles
/// => no collisions with any non-empty set, which is the safe default).
pub(crate) fn minhash_signature(shingles: &HashSet<String>) -> [u64; MINHASH_PERMUTATIONS] {
    let mut sig = [u64::MAX; MINHASH_PERMUTATIONS];
    for (seed, slot) in sig.iter_mut().enumerate() {
        let mut min = u64::MAX;
        for shingle in shingles {
            let h = seeded_hash(seed as u64, shingle);
            if h < min {
                min = h;
            }
        }
        *slot = min;
    }
    sig
}

/// LSH band keys: split the signature into 8 contiguous bands of 4 rows and
/// hash each band into a single `u64`. Two names that share ANY band key are
/// LSH candidates (high recall); exact [`jaccard`] then confirms.
pub(crate) fn lsh_bands(sig: &[u64; MINHASH_PERMUTATIONS]) -> Vec<u64> {
    sig.chunks(MINHASH_BAND_SIZE)
        .map(|band| {
            // FNV-1a over the band rows (each row as 8 little-endian bytes).
            // Version-stable: these keys are persisted, see fnv1a64.
            let mut key = FNV_OFFSET_BASIS;
            for &v in band {
                key = fnv1a64(key, &v.to_le_bytes());
            }
            key
        })
        .collect()
}

/// Exact Jaccard similarity |A n B| / |A u B|. Returns 0.0 on an empty union
/// (no div-by-zero), so two empty sets are treated as dissimilar.
pub(crate) fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 0.0;
    }
    let intersection = a.intersection(b).count();
    let union = a.len() + b.len() - intersection;
    if union == 0 {
        return 0.0;
    }
    intersection as f64 / union as f64
}

/// Shannon entropy (bits) over the character-frequency distribution of `name`.
/// `-sum p*log2(p)`. Empty string => 0.0. Repetitive names ("AAA") score low;
/// varied names ("Backend Team") score high. UTF-8 safe (`chars()`).
pub(crate) fn shannon_entropy(name: &str) -> f64 {
    let chars: Vec<char> = name.chars().collect();
    if chars.is_empty() {
        return 0.0;
    }
    let total = chars.len() as f64;
    let mut freq: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
    for c in &chars {
        *freq.entry(*c).or_insert(0) += 1;
    }
    let mut entropy = 0.0;
    for &count in freq.values() {
        let p = count as f64 / total;
        entropy -= p * p.log2();
    }
    entropy
}

/// Eligibility gate for fuzzy dedup: a name must be long enough
/// (`>= MIN_NAME_LEN_FOR_FUZZY` chars) AND varied enough
/// (`shannon_entropy >= NAME_ENTROPY_THRESHOLD`). Short acronyms ("AAN", "API")
/// and low-entropy strings ("aaaaaa") are punted to the vector cascade so the
/// fuzzy layer never over-merges distinct short names.
pub(crate) fn has_high_entropy(name: &str) -> bool {
    name.chars().count() >= MIN_NAME_LEN_FOR_FUZZY
        && shannon_entropy(name) >= NAME_ENTROPY_THRESHOLD
}

/// Convenience: shingle + sign + band a name in one call. Returns the band keys
/// used to index/query the `entity_minhash_bands` table.
pub(crate) fn name_band_keys(name: &str) -> Vec<u64> {
    let shingles = char_shingles(name, SHINGLE_K);
    let sig = minhash_signature(&shingles);
    lsh_bands(&sig)
}

/// Exact trigram Jaccard between two names (shingle + compare). Used by the
/// cascade to confirm an LSH candidate before auto-merging.
pub(crate) fn name_jaccard(a: &str, b: &str) -> f64 {
    jaccard(&char_shingles(a, SHINGLE_K), &char_shingles(b, SHINGLE_K))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_shingles_trigram_window() {
        let s = char_shingles("postgres", 3);
        assert!(s.contains("pos"));
        assert!(s.contains("ost"));
        assert!(s.contains("stg"));
        // ASCII name with no repeated trigrams: count == len - (k-1) == 8 - 2 == 6
        assert_eq!(s.len(), "postgres".len() - 2);
    }

    #[test]
    fn char_shingles_is_lowercased() {
        let upper = char_shingles("PostgreSQL", 3);
        let lower = char_shingles("postgresql", 3);
        assert_eq!(upper, lower);
    }

    #[test]
    fn char_shingles_utf8_no_panic() {
        // "café" has a multibyte char; must not panic on char boundaries.
        let s = char_shingles("café", 3);
        assert!(!s.is_empty());
    }

    #[test]
    fn char_shingles_short_name_returns_all() {
        // shorter than k => whole-name singleton set (non-empty, no panic)
        let s = char_shingles("ab", 3);
        assert_eq!(s.len(), 1);
        assert!(s.contains("ab"));
    }

    #[test]
    fn char_shingles_empty_is_empty() {
        assert!(char_shingles("", 3).is_empty());
    }

    #[test]
    fn jaccard_identical_is_one() {
        let s = char_shingles("PostgreSQL", 3);
        assert_eq!(jaccard(&s, &s), 1.0);
    }

    #[test]
    fn jaccard_empty_union_is_zero() {
        let empty: HashSet<String> = HashSet::new();
        assert_eq!(jaccard(&empty, &empty), 0.0);
    }

    #[test]
    fn jaccard_disjoint_is_zero() {
        let a = char_shingles("xyzqrs", 3);
        let b = char_shingles("abcdef", 3);
        assert_eq!(jaccard(&a, &b), 0.0);
    }

    #[test]
    fn jaccard_postgres_variants_high() {
        let a = char_shingles("PostgreSQL", 3);
        let b = char_shingles("Postgres", 3);
        // Postgres trigrams are a near-subset of PostgreSQL; recall case must be substantial.
        assert!(
            jaccard(&a, &b) >= 0.5,
            "PostgreSQL/Postgres trigram jaccard = {}",
            jaccard(&a, &b)
        );
    }

    #[test]
    fn jaccard_abbreviation_pair_high() {
        // Canonical SHOULD-merge recall case: a plural/suffix variant.
        let a = char_shingles("Backend Team", 3);
        let b = char_shingles("Backend Teams", 3);
        assert!(
            jaccard(&a, &b) >= 0.9,
            "Backend Team/Backend Teams jaccard = {}",
            jaccard(&a, &b)
        );
    }

    #[test]
    fn jaccard_distinct_projects_low() {
        let a = char_shingles("Project Alpha", 3);
        let b = char_shingles("Project Beta", 3);
        assert!(
            jaccard(&a, &b) < 0.9,
            "Project Alpha/Project Beta jaccard = {}",
            jaccard(&a, &b)
        );
    }

    #[test]
    fn jaccard_react_redux_low() {
        let a = char_shingles("React", 3);
        let b = char_shingles("Redux", 3);
        assert!(
            jaccard(&a, &b) < 0.9,
            "React/Redux jaccard = {}",
            jaccard(&a, &b)
        );
    }

    #[test]
    fn shannon_entropy_repetitive_low() {
        assert!(shannon_entropy("AAA") < 1.5);
        assert!(shannon_entropy("AAN") < 1.5);
    }

    #[test]
    fn shannon_entropy_varied_high() {
        assert!(
            shannon_entropy("Backend Team") >= 1.5,
            "entropy = {}",
            shannon_entropy("Backend Team")
        );
    }

    #[test]
    fn shannon_entropy_empty_is_zero() {
        assert_eq!(shannon_entropy(""), 0.0);
    }

    #[test]
    fn has_high_entropy_short_name_false() {
        assert!(!has_high_entropy("AAN"));
        assert!(!has_high_entropy("API"));
    }

    #[test]
    fn has_high_entropy_low_entropy_false() {
        // 6 chars but all identical => entropy 0 => false.
        assert!(!has_high_entropy("aaaaaa"));
    }

    #[test]
    fn has_high_entropy_full_name_true() {
        assert!(has_high_entropy("PostgreSQL"));
        assert!(has_high_entropy("Postgres"));
    }

    #[test]
    fn minhash_signature_deterministic() {
        let s = char_shingles("PostgreSQL database engine", 3);
        let a = minhash_signature(&s);
        let b = minhash_signature(&s);
        assert_eq!(a, b, "same shingle set must yield identical signature");
    }

    #[test]
    fn minhash_signature_len_32() {
        let s = char_shingles("PostgreSQL", 3);
        let sig = minhash_signature(&s);
        assert_eq!(sig.len(), 32);
    }

    #[test]
    fn lsh_bands_count() {
        let s = char_shingles("PostgreSQL", 3);
        let bands = lsh_bands(&minhash_signature(&s));
        assert_eq!(bands.len(), 8, "32 perms / 4 rows == 8 bands");
    }

    #[test]
    fn lsh_bands_collide_for_near_dup() {
        let pg = lsh_bands(&minhash_signature(&char_shingles("PostgreSQL", 3)));
        let pgs = lsh_bands(&minhash_signature(&char_shingles("Postgres", 3)));
        let shared = pg.iter().filter(|k| pgs.contains(k)).count();
        assert!(
            shared >= 1,
            "PostgreSQL/Postgres should share >=1 band key, got {shared}"
        );

        let azure = lsh_bands(&minhash_signature(&char_shingles("Microsoft Azure", 3)));
        let shared_far = pg.iter().filter(|k| azure.contains(k)).count();
        assert_eq!(
            shared_far, 0,
            "PostgreSQL/Microsoft Azure should share 0 band keys"
        );
    }

    #[test]
    fn minhash_jaccard_approximation() {
        // Estimated Jaccard from shared-signature fraction within +/-0.15 of exact.
        let a = char_shingles("PostgreSQL", 3);
        let b = char_shingles("Postgres", 3);
        let exact = jaccard(&a, &b);
        let sig_a = minhash_signature(&a);
        let sig_b = minhash_signature(&b);
        let matches = sig_a
            .iter()
            .zip(sig_b.iter())
            .filter(|(x, y)| x == y)
            .count();
        let estimated = matches as f64 / MINHASH_PERMUTATIONS as f64;
        assert!(
            (estimated - exact).abs() <= 0.15,
            "minhash estimate {estimated} vs exact {exact} diverged > 0.15"
        );
    }

    #[test]
    fn name_band_keys_matches_pipeline() {
        let direct = lsh_bands(&minhash_signature(&char_shingles("PostgreSQL", 3)));
        assert_eq!(name_band_keys("PostgreSQL"), direct);
    }

    #[test]
    fn name_jaccard_matches_shingle_jaccard() {
        let a = char_shingles("PostgreSQL", 3);
        let b = char_shingles("Postgres", 3);
        assert_eq!(name_jaccard("PostgreSQL", "Postgres"), jaccard(&a, &b));
    }

    // ── Golden values ────────────────────────────────────────────────────────
    //
    // These lock the FNV-1a hash that produces the PERSISTED band keys. The band
    // keys written to `entity_minhash_bands` are permanent identifiers: if the
    // hash ever changes, keys written by an older binary stop matching keys
    // queried by a newer one and dedup silently breaks for every pre-existing
    // entity. These asserts fail loudly if the algorithm drifts.
    //
    // DO NOT update these values without a band-table rebuild migration that
    // re-indexes every entity under the new hash.

    #[test]
    fn golden_band_keys_postgresql() {
        // Golden values -- locking the persisted-band-key hash; DO NOT update
        // without a band-table rebuild migration.
        let expected: Vec<u64> = vec![
            15505343357709547738,
            11614669564543327082,
            8181423715502984041,
            2036825120049504309,
            17116818506814607712,
            4923842695072352247,
            9833741854967118892,
            1016496347732194460,
        ];
        assert_eq!(
            name_band_keys("PostgreSQL"),
            expected,
            "FNV-1a band-key hash drifted -- persisted entity_minhash_bands rows would orphan"
        );
    }

    #[test]
    fn golden_minhash_signature_first_slot() {
        // Golden value -- locking the persisted-band-key hash; DO NOT update
        // without a band-table rebuild migration.
        let sig = minhash_signature(&char_shingles("PostgreSQL database engine", 3));
        assert_eq!(
            sig[0], 1934270594977211692u64,
            "FNV-1a MinHash permutation 0 drifted -- signatures no longer reproducible"
        );
    }
}
