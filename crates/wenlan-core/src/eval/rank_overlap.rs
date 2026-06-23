use std::collections::HashSet;

/// Normalized top-weighted RBO. 1.0 = identical order; lower = more reordering,
/// top weighted most by persistence `p`. Handles unequal-length and empty lists.
// ponytail: O(depth^2) (a HashSet per prefix depth). Fine for top_k <= ~20.
pub fn rbo(a: &[String], b: &[String], p: f64) -> f64 {
    let depth = a.len().max(b.len());
    if depth == 0 {
        return 1.0;
    }
    let mut num = 0.0_f64;
    let mut den = 0.0_f64;
    let mut weight = 1.0_f64;
    for d in 1..=depth {
        let set_a: HashSet<&str> = a.iter().take(d).map(|s| s.as_str()).collect();
        let set_b: HashSet<&str> = b.iter().take(d).map(|s| s.as_str()).collect();
        let inter = set_a.intersection(&set_b).count() as f64;
        let agreement = inter / d as f64;
        num += weight * agreement;
        den += weight;
        weight *= p;
    }
    num / den
}

#[cfg(test)]
mod tests {
    use super::*;
    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn identical_lists_score_one() {
        let a = v(&["x", "y", "z"]);
        assert!((rbo(&a, &a, 0.9) - 1.0).abs() < 1e-9);
    }
    #[test]
    fn empty_lists_score_one() {
        let e: Vec<String> = Vec::new();
        assert!((rbo(&e, &e, 0.9) - 1.0).abs() < 1e-9);
    }
    #[test]
    fn hand_computed_swap_value() {
        // a=[x,y,z], b=[x,z,y], p=0.5: d1 1.0(w1) d2 0.5(w.5) d3 1.0(w.25)
        // num=1.5 den=1.75 => 0.857142...
        let a = v(&["x", "y", "z"]);
        let b = v(&["x", "z", "y"]);
        assert!((rbo(&a, &b, 0.5) - 0.857142).abs() < 1e-4);
    }
    #[test]
    fn reversed_scores_low() {
        let a = v(&["x", "y", "z"]);
        let b = v(&["z", "y", "x"]);
        assert!(rbo(&a, &b, 0.5) < 0.5);
    }
    #[test]
    fn within_top_k_demotion_trips() {
        let a = v(&["A", "B", "C", "D", "E"]);
        let b = v(&["B", "C", "D", "E", "A"]);
        assert!(
            rbo(&a, &b, 0.9) < 0.7,
            "within-top-K demotion must drop RBO well below 1.0"
        );
    }
}
