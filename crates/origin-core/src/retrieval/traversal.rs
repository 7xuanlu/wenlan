// SPDX-License-Identifier: Apache-2.0
//! T4b — k-hop entity-graph traversal (pure core).
//!
//! `augment_with_graph` (db.rs) is single-hop today: query -> top entities ->
//! observations. T4b extends it to walk the entity->relation->entity graph up to
//! `max_hops` from those anchor entities, surfacing memories that are one
//! relation removed from the query's entities. This module holds the *pure*
//! traversal so the bound + cycle logic is unit-testable without a database:
//! adjacency in, expanded node set out.
//!
//! The walk is hard-bounded three ways so it can never run away on a dense or
//! cyclic graph:
//!   1. `max_hops` -- BFS depth limit (default mirrors single-hop = 1).
//!   2. `max_nodes` -- total visited-set cap; once reached the walk stops.
//!   3. a `visited` set -- every node is expanded at most once, so a relation
//!      cycle (A->B->A) terminates instead of looping forever.
//!
//! Wired into `db::augment_with_graph` behind `ORIGIN_ENABLE_GRAPH_KHOP`
//! (default OFF). When off, the anchor entity set is used verbatim and behaviour
//! is byte-identical to the pre-T4b single-hop path.

use std::collections::{HashMap, HashSet, VecDeque};

/// Undirected adjacency list keyed by entity id. Built from the `relations`
/// table by inserting BOTH directions of every edge (from->to and to->from) so a
/// relation surfaces its neighbour regardless of which way it was stored.
pub(crate) type Adjacency = HashMap<String, Vec<String>>;

/// Build an undirected adjacency map from `(from_entity, to_entity)` edge pairs.
///
/// Both directions are inserted. Self-loops (from == to) and empty ids are
/// dropped -- they add no reachable node and only waste a visited-set slot.
/// Duplicate edges are kept as-is in the neighbour vectors; the BFS `visited`
/// set dedups on expansion, so duplicates never cause repeated work or
/// double-counting.
pub(crate) fn build_adjacency(edges: &[(String, String)]) -> Adjacency {
    let mut adj: Adjacency = HashMap::new();
    for (from, to) in edges {
        if from == to || from.is_empty() || to.is_empty() {
            continue;
        }
        adj.entry(from.clone()).or_default().push(to.clone());
        adj.entry(to.clone()).or_default().push(from.clone());
    }
    adj
}

/// Breadth-first k-hop expansion over an entity adjacency map.
///
/// Returns the deduped, deterministically-sorted set of entity ids reachable
/// from `seeds` within `max_hops`, INCLUDING the seeds themselves (hop 0).
///
/// Hard bounds (all enforced):
/// - `max_hops`: a node `max_hops` edges away is included; one further is not.
///   `max_hops == 0` returns the deduped seeds only (no expansion).
/// - `max_nodes`: the visited set never exceeds this; once it is reached the
///   walk stops immediately and returns what it has. A `max_nodes` smaller than
///   the seed count is raised to the seed count so an anchor set is never
///   silently emptied.
/// - cycle safety: a node is enqueued/expanded at most once (the `visited` set),
///   so a relation cycle `A->B->A` terminates.
///
/// Pure: no I/O, no clock, deterministic for a given input. Sorted output makes
/// downstream RRF + tie-breaking reproducible.
pub(crate) fn bfs_khop(
    adjacency: &Adjacency,
    seeds: &[String],
    max_hops: usize,
    max_nodes: usize,
) -> Vec<String> {
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();

    // Effective node cap: at minimum admit the seeds (deduped) so the anchor set
    // is never dropped to empty by a tiny/zero cap.
    let seed_unique: usize = seeds
        .iter()
        .filter(|s| !s.is_empty())
        .collect::<HashSet<_>>()
        .len();
    let cap = max_nodes.max(seed_unique);

    for s in seeds {
        if s.is_empty() {
            continue;
        }
        if visited.insert(s.clone()) {
            queue.push_back((s.clone(), 0));
        }
        if visited.len() >= cap {
            break;
        }
    }

    while let Some((node, depth)) = queue.pop_front() {
        if depth >= max_hops {
            continue;
        }
        if let Some(neighbours) = adjacency.get(&node) {
            for nb in neighbours {
                if visited.len() >= cap {
                    break;
                }
                if visited.insert(nb.clone()) {
                    queue.push_back((nb.clone(), depth + 1));
                }
            }
        }
        if visited.len() >= cap {
            break;
        }
    }

    let mut out: Vec<String> = visited.into_iter().collect();
    out.sort();
    out
}

/// Parse `ORIGIN_GRAPH_KHOP_DEPTH` -> usize in [0, 3]. Default 1 on unset or
/// parse failure (1 hop == byte-parity with the legacy single-hop expansion).
/// Capped at 3 to match the T9 `parse_hop_depth` ceiling -- k-hop cost grows fast.
pub(crate) fn parse_khop_depth(val: Option<&str>) -> usize {
    let raw = match val {
        Some(s) => s,
        None => return 1,
    };
    match raw.trim().parse::<isize>() {
        Ok(n) if n >= 0 => (n as usize).min(3),
        _ => 1,
    }
}

/// Parse `ORIGIN_GRAPH_KHOP_MAX_NODES` -> usize in [1, 512]. Default 25 on unset
/// or parse failure. Bounds total expanded entities so a hub entity can't pull a
/// combinatorial blast of tangential memories.
pub(crate) fn parse_khop_max_nodes(val: Option<&str>) -> usize {
    let raw = match val {
        Some(s) => s,
        None => return 25,
    };
    match raw.trim().parse::<isize>() {
        Ok(n) if n >= 1 => (n as usize).min(512),
        _ => 25,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edges(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    fn seeds(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    /// A->B->C chain: max_hops=2 reaches C; disconnected D is never returned.
    #[test]
    fn test_traversal_two_hop_reaches_chain_end() {
        let adj = build_adjacency(&edges(&[("A", "B"), ("B", "C"), ("X", "D")]));
        let out = bfs_khop(&adj, &seeds(&["A"]), 2, 100);
        assert!(out.contains(&"A".to_string()), "seed always included");
        assert!(out.contains(&"B".to_string()), "1 hop");
        assert!(out.contains(&"C".to_string()), "2 hops reachable");
        assert!(
            !out.contains(&"D".to_string()),
            "disconnected D must NOT be reached"
        );
    }

    /// max_hops=1 stops at B; C (2 hops) is excluded.
    #[test]
    fn test_traversal_respects_max_hops() {
        let adj = build_adjacency(&edges(&[("A", "B"), ("B", "C")]));
        let out = bfs_khop(&adj, &seeds(&["A"]), 1, 100);
        assert!(out.contains(&"A".to_string()));
        assert!(out.contains(&"B".to_string()), "1 hop included");
        assert!(
            !out.contains(&"C".to_string()),
            "C is 2 hops away, must be excluded at max_hops=1"
        );
    }

    /// CYCLE TERMINATION: A->B->A must terminate (not infinite-loop) and return the
    /// 2-node cycle without duplication.
    #[test]
    fn test_traversal_cycle_terminates() {
        // A<->B forms a cycle once both directions are inserted; add B->A explicitly
        // too so the relation table really contains the back-edge.
        let adj = build_adjacency(&edges(&[("A", "B"), ("B", "A")]));
        let out = bfs_khop(&adj, &seeds(&["A"]), 3, 100);
        // Terminates (the test completing at all proves no infinite loop) and the
        // visited set dedups: exactly {A, B}, no repeats.
        assert_eq!(out, vec!["A".to_string(), "B".to_string()]);
    }

    /// Larger cycle A->B->C->A also terminates and yields each node once.
    #[test]
    fn test_traversal_triangle_cycle_terminates() {
        let adj = build_adjacency(&edges(&[("A", "B"), ("B", "C"), ("C", "A")]));
        let out = bfs_khop(&adj, &seeds(&["A"]), 5, 100);
        assert_eq!(out, vec!["A".to_string(), "B".to_string(), "C".to_string()]);
    }

    /// Bounded fan-out: a hub with many neighbours is capped by max_nodes.
    #[test]
    fn test_traversal_bounded_fanout() {
        let mut pairs: Vec<(String, String)> = Vec::new();
        for i in 0..100 {
            pairs.push(("HUB".to_string(), format!("L{i}")));
        }
        let adj = build_adjacency(&pairs);
        let out = bfs_khop(&adj, &seeds(&["HUB"]), 1, 10);
        assert!(
            out.len() <= 10,
            "max_nodes=10 caps the visited set; got {}",
            out.len()
        );
    }

    /// Empty seed set -> empty output, no panic.
    #[test]
    fn test_traversal_empty_seed_returns_empty() {
        let adj = build_adjacency(&edges(&[("A", "B")]));
        let out = bfs_khop(&adj, &[], 2, 100);
        assert!(out.is_empty());
    }

    /// max_hops=0 returns deduped seeds only (no expansion) -- this is the
    /// byte-parity case the flag-off path leans on (depth defaults to 1 single-hop,
    /// but a 0 depth must still be a clean no-expand).
    #[test]
    fn test_traversal_zero_hops_is_seeds_only() {
        let adj = build_adjacency(&edges(&[("A", "B"), ("B", "C")]));
        let out = bfs_khop(&adj, &seeds(&["A", "A"]), 0, 100);
        assert_eq!(out, vec!["A".to_string()], "deduped seeds, no neighbours");
    }

    /// Seeds are never dropped even when max_nodes is smaller than the seed set.
    #[test]
    fn test_traversal_seeds_survive_tiny_cap() {
        let adj = build_adjacency(&edges(&[("A", "B")]));
        let out = bfs_khop(&adj, &seeds(&["A", "B", "C"]), 1, 1);
        // cap is raised to the seed count (3); seeds all admitted, no expansion room.
        assert!(out.contains(&"A".to_string()));
        assert!(out.contains(&"B".to_string()));
        assert!(out.contains(&"C".to_string()));
    }

    /// Self-loops are dropped by build_adjacency.
    #[test]
    fn test_build_adjacency_drops_self_loops() {
        let adj = build_adjacency(&edges(&[("A", "A"), ("A", "B")]));
        assert_eq!(adj.get("A"), Some(&vec!["B".to_string()]));
    }

    #[test]
    fn test_parse_khop_depth_defaults_and_clamps() {
        assert_eq!(parse_khop_depth(None), 1);
        assert_eq!(parse_khop_depth(Some("0")), 0);
        assert_eq!(parse_khop_depth(Some("2")), 2);
        assert_eq!(parse_khop_depth(Some("3")), 3);
        assert_eq!(parse_khop_depth(Some("99")), 3);
        assert_eq!(parse_khop_depth(Some("-1")), 1);
        assert_eq!(parse_khop_depth(Some("abc")), 1);
    }

    #[test]
    fn test_parse_khop_max_nodes_defaults_and_clamps() {
        assert_eq!(parse_khop_max_nodes(None), 25);
        assert_eq!(parse_khop_max_nodes(Some("10")), 10);
        assert_eq!(parse_khop_max_nodes(Some("999")), 512);
        assert_eq!(parse_khop_max_nodes(Some("0")), 25);
        assert_eq!(parse_khop_max_nodes(Some("bad")), 25);
    }
}
