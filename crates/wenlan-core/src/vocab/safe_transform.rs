//! Deterministic, provably-safe vocabulary transforms — the auto-heal band.
//!
//! A transform may ONLY rewrite a value toward an existing canonical. Anything
//! that touches meaning (tense, role, direction, opposites) has no rule here and
//! falls through to `None`, i.e. the review queue. ponytail: three rules only —
//! casefold, separator-normalize, unique trailing-s singularize. Grow this set
//! only with a matching rejection test.

/// Normalize case and separators: lowercase, and map `-`/space to `_`.
fn normalize_shape(value: &str) -> String {
    value.trim().to_lowercase().replace([' ', '-'], "_")
}

/// Candidate forms of `value` that a safe transform could produce.
fn candidate_forms(value: &str) -> Vec<String> {
    let base = normalize_shape(value);
    let mut forms = vec![base.clone()];
    // Unique trailing-s singularize (concepts -> concept). Only the simple
    // trailing 's'; never 'ies'->'y' or irregulars (too semantic to be "safe").
    if let Some(singular) = base.strip_suffix('s') {
        if !singular.is_empty() {
            forms.push(singular.to_string());
        }
    }
    forms
}

/// Returns `Some(canonical)` iff a deterministic transform maps `value` to
/// exactly one member of `canonicals`. The output is ALWAYS in `canonicals`.
pub fn safe_transform(value: &str, canonicals: &[String]) -> Option<String> {
    // An already-canonical value (exact, case-sensitive match) is not "dirty"
    // — nothing to transform. Compare against the literal list, not a
    // casefolded set, so a differently-cased variant (e.g. "Concept") still
    // falls through to the transform below instead of short-circuiting here.
    if canonicals.iter().any(|c| c == value) {
        return None;
    }
    let forms = candidate_forms(value);
    let hits: Vec<&String> = canonicals
        .iter()
        .filter(|c| forms.contains(&c.to_lowercase()))
        .collect();
    // Exactly one canonical target, or refuse (ambiguous or none).
    match hits.as_slice() {
        [only] => Some((*only).clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::safe_transform;

    fn canon() -> Vec<String> {
        ["concept", "technology", "project", "works_on", "member_of"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn casefold_and_singularize_to_existing_canonical() {
        assert_eq!(safe_transform("concepts", &canon()), Some("concept".into()));
        assert_eq!(safe_transform("Concept", &canon()), Some("concept".into()));
        assert_eq!(
            safe_transform("works-on", &canon()),
            Some("works_on".into())
        );
        assert_eq!(
            safe_transform("works on", &canon()),
            Some("works_on".into())
        );
    }

    #[test]
    fn rejects_when_target_is_not_canonical() {
        // No canonical `aw`; must fail closed to review, never mint `aw`.
        assert_eq!(safe_transform("aws", &canon()), None);
        // Semantic / role / tense words never match a deterministic transform.
        assert_eq!(safe_transform("owns", &canon()), None);
        assert_eq!(safe_transform("design_inspiration", &canon()), None);
        assert_eq!(safe_transform("generates", &canon()), None);
    }

    #[test]
    fn rejects_ambiguous_singularize() {
        // Genuine ambiguity: "Dogs" -> forms {"dogs","dog"}, both are canonicals.
        let c: Vec<String> = ["dog", "dogs"].iter().map(|s| s.to_string()).collect();
        assert_eq!(safe_transform("Dogs", &c), None);
    }
}
