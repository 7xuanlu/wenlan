// SPDX-License-Identifier: Apache-2.0
//! Spreading-activation BFS over a `RelationGraph`.
//!
//! Seeds start at activation 1.0; each hop multiplies by `decay`.  Propagation
//! to a neighbour is suppressed when the resulting value would fall below
//! `threshold` (pre-insertion gate), so the returned map contains only entries
//! whose activation meets the threshold.  Iteration halts when no new entries
//! are added or `max_iter` hops are exhausted.

use std::collections::HashMap;

use super::relation_graph::RelationGraph;

/// Tuning knobs for `activate`.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub(crate) struct ActivationParams {
    /// Multiplicative decay applied at each hop (0 < decay < 1).
    pub decay: f64,
    /// Minimum propagated value required to insert a neighbour.
    /// Propagation that would produce a value below this is skipped.
    pub threshold: f64,
    /// Maximum BFS hops from the seed layer.
    pub max_iter: u8,
}

impl Default for ActivationParams {
    fn default() -> Self {
        Self {
            decay: 0.5,
            threshold: 0.1,
            max_iter: 3,
        }
    }
}

/// Propagate activation from `seed_entities` through `relations` and return a
/// map of entity → activation value.
///
/// Gate: propagation to a neighbour is skipped when `src_act * decay <
/// threshold`.  This means the returned map contains only entries whose
/// activation is ≥ threshold (seeds are always included because they start at
/// 1.0).
#[allow(dead_code)]
pub(crate) fn activate(
    seed_entities: &[String],
    relations: &RelationGraph,
    params: ActivationParams,
) -> HashMap<String, f64> {
    let mut activation: HashMap<String, f64> = HashMap::new();
    for e in seed_entities {
        activation.insert(e.clone(), 1.0);
    }

    let mut frontier: Vec<String> = seed_entities.to_vec();
    for _ in 0..params.max_iter {
        let mut next: Vec<String> = Vec::new();
        for source in &frontier {
            let src_act = *activation.get(source).unwrap_or(&0.0);
            for target in relations.neighbors(source) {
                let propagated = src_act * params.decay;
                // Pre-insertion gate: suppress if the propagated value would
                // fall below threshold.  This is the gate that prevents
                // low-value neighbours from entering the map at all.
                if propagated < params.threshold {
                    continue;
                }
                let entry = activation.entry(target.clone()).or_insert(0.0);
                if propagated > *entry {
                    *entry = propagated;
                    next.push(target);
                }
            }
        }
        if next.is_empty() {
            break;
        }
        frontier = next;
    }
    activation
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composite::relation_graph::RelationGraph;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn activate_decays_correctly_at_each_hop() {
        // A -- B -- C (undirected chain stored bidirectionally)
        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
        edges.insert("A".into(), ["B".into()].into_iter().collect());
        edges.insert("B".into(), ["A".into(), "C".into()].into_iter().collect());
        edges.insert("C".into(), ["B".into()].into_iter().collect());
        let graph = RelationGraph::from_edges_for_test(edges);

        let act = activate(&["A".into()], &graph, ActivationParams::default());

        assert_eq!(act["A"], 1.0);
        assert_eq!(act["B"], 0.5);
        assert_eq!(act["C"], 0.25);
    }

    #[test]
    fn activate_stops_below_threshold() {
        // Chain A→B→C→D→E with decay=0.5, threshold=0.1.
        // Activations: A=1.0, B=0.5, C=0.25, D=0.125.
        // D would propagate to E with value 0.0625, but 0.0625 < 0.1
        // (threshold gate fires before insertion), so E must NOT appear.
        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
        edges.insert("A".into(), ["B".into()].into_iter().collect());
        edges.insert("B".into(), ["A".into(), "C".into()].into_iter().collect());
        edges.insert("C".into(), ["B".into(), "D".into()].into_iter().collect());
        edges.insert("D".into(), ["C".into(), "E".into()].into_iter().collect());
        edges.insert("E".into(), ["D".into()].into_iter().collect());
        let graph = RelationGraph::from_edges_for_test(edges);

        let act = activate(&["A".into()], &graph, ActivationParams::default());

        assert!(
            act.contains_key("D"),
            "D should be reachable (act=0.125 >= 0.1)"
        );
        assert!(
            !act.contains_key("E"),
            "E must not appear (propagated=0.0625 < threshold 0.1)"
        );
    }

    #[test]
    fn activate_respects_max_iter() {
        // Chain A→B→C→D→E→F; with max_iter=2 only 2 hops from the seed
        // layer are explored, so nodes beyond 2 hops must not appear.
        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
        edges.insert("A".into(), ["B".into()].into_iter().collect());
        edges.insert("B".into(), ["A".into(), "C".into()].into_iter().collect());
        edges.insert("C".into(), ["B".into(), "D".into()].into_iter().collect());
        edges.insert("D".into(), ["C".into(), "E".into()].into_iter().collect());
        edges.insert("E".into(), ["D".into(), "F".into()].into_iter().collect());
        edges.insert("F".into(), ["E".into()].into_iter().collect());
        let graph = RelationGraph::from_edges_for_test(edges);

        let params = ActivationParams {
            max_iter: 2,
            ..ActivationParams::default()
        };
        let act = activate(&["A".into()], &graph, params);

        // Hop 1: B; hop 2: C.  D, E, F must not appear.
        assert!(act.contains_key("A"));
        assert!(act.contains_key("B"));
        assert!(act.contains_key("C"));
        assert!(!act.contains_key("D"), "D is 3 hops away, must not appear");
        assert!(!act.contains_key("E"), "E is 4 hops away, must not appear");
        assert!(!act.contains_key("F"), "F is 5 hops away, must not appear");
    }

    #[test]
    fn activate_zero_seeds_returns_empty() {
        let graph = RelationGraph::from_edges_for_test(HashMap::new());
        let act = activate(&[], &graph, ActivationParams::default());
        assert!(act.is_empty());
    }
}
