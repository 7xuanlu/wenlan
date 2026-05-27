// SPDX-License-Identifier: Apache-2.0
//! RelationGraph: builds an in-Rust bidirectional adjacency map for the
//! subgraph reachable from a set of seed entities within `max_depth` hops.
//!
//! Uses the same recursive CTE shape as `graph_distance` so the SQLite query
//! planner can reuse cached query plans and both `idx_relations_from` /
//! `idx_relations_to` indexes are exercised.

use std::collections::{HashMap, HashSet};

use crate::db::MemoryDB;
use crate::OriginError;

/// In-memory adjacency map over the knowledge-graph subgraph reachable from a
/// query's seed entities.
///
/// Edges are stored bidirectionally: inserting `(A, B)` also inserts `(B, A)`,
/// so `neighbors()` is symmetric regardless of the original edge direction in
/// the `relations` table.
pub(crate) struct RelationGraph {
    edges: HashMap<String, HashSet<String>>,
}

impl RelationGraph {
    /// Build the adjacency map for `seed_entity_ids` within `max_depth` hops.
    ///
    /// Returns `Ok(RelationGraph { edges: HashMap::new() })` when
    /// `seed_entity_ids` is empty (no SQL round-trip needed).
    // Plan B Task 4 (spreading-activation) and Task 9 (composite scorer) consume this.
    #[allow(dead_code)]
    pub(crate) async fn for_query(
        db: &MemoryDB,
        seed_entity_ids: &[&str],
        max_depth: u8,
    ) -> Result<Self, OriginError> {
        if seed_entity_ids.is_empty() {
            return Ok(Self {
                edges: HashMap::new(),
            });
        }

        let conn = db.conn.lock().await;

        let placeholders = seed_entity_ids
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");

        let sql = format!(
            "WITH RECURSIVE visit(entity_id, distance) AS (
                SELECT id, 0 FROM entities WHERE id IN ({placeholders})
              UNION ALL
                SELECT r.to_entity, v.distance + 1
                FROM visit v JOIN relations r ON r.from_entity = v.entity_id
                WHERE v.distance < ?
              UNION ALL
                SELECT r.from_entity, v.distance + 1
                FROM visit v JOIN relations r ON r.to_entity = v.entity_id
                WHERE v.distance < ?
            )
            SELECT DISTINCT r.from_entity, r.to_entity
            FROM relations r
            WHERE r.from_entity IN (SELECT entity_id FROM visit)
               OR r.to_entity IN (SELECT entity_id FROM visit)"
        );

        let mut params: Vec<libsql::Value> = seed_entity_ids
            .iter()
            .map(|s| libsql::Value::Text(s.to_string()))
            .collect();
        params.push(libsql::Value::Integer(max_depth as i64));
        params.push(libsql::Value::Integer(max_depth as i64));

        let mut rows = conn
            .query(&sql, params)
            .await
            .map_err(|e| OriginError::VectorDb(format!("relation_graph query: {e}")))?;

        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| OriginError::VectorDb(format!("relation_graph row: {e}")))?
        {
            let from: String = row
                .get(0)
                .map_err(|e| OriginError::VectorDb(format!("relation_graph col0: {e}")))?;
            let to: String = row
                .get(1)
                .map_err(|e| OriginError::VectorDb(format!("relation_graph col1: {e}")))?;
            edges.entry(from.clone()).or_default().insert(to.clone());
            edges.entry(to).or_default().insert(from);
        }

        Ok(Self { edges })
    }

    /// Returns the neighbors of `entity_id` in the adjacency map.
    ///
    /// Always symmetric: if B is in `neighbors(A)`, then A is in `neighbors(B)`.
    // Plan B Task 4 (spreading-activation BFS) consumes this.
    #[allow(dead_code)]
    pub(crate) fn neighbors(&self, entity_id: &str) -> Vec<String> {
        self.edges
            .get(entity_id)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
impl RelationGraph {
    /// Test-only constructor: build a `RelationGraph` directly from an
    /// adjacency map without touching SQL.  Task 4 (spreading-activation)
    /// uses this to set up synthetic graphs.
    #[allow(dead_code)]
    pub(crate) fn from_edges_for_test(edges: HashMap<String, HashSet<String>>) -> Self {
        Self { edges }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn relation_graph_built_from_cte_output_neighbors_correct() {
        let (db, _dir) = crate::db::tests::test_db().await;
        let conn = db.conn.lock().await;

        // Seed entities A, B, C. Relations: A->B and B->C.
        for (id, name) in [("ent_a", "A"), ("ent_b", "B"), ("ent_c", "C")] {
            conn.execute(
                "INSERT INTO entities (id, name, entity_type, created_at, updated_at) VALUES (?, ?, 'Topic', 0, 0)",
                [id, name],
            )
            .await
            .unwrap();
        }
        conn.execute(
            "INSERT INTO relations (id, from_entity, to_entity, relation_type, created_at) VALUES ('r1', 'ent_a', 'ent_b', 'knows', 0)",
            (),
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO relations (id, from_entity, to_entity, relation_type, created_at) VALUES ('r2', 'ent_b', 'ent_c', 'worked_at', 0)",
            (),
        )
        .await
        .unwrap();

        drop(conn);

        let graph = RelationGraph::for_query(&db, &["ent_a"], 2)
            .await
            .expect("build graph");

        // ent_a neighbors: only ent_b (forward edge A->B).
        let mut neighbors_a = graph.neighbors("ent_a");
        neighbors_a.sort();
        assert_eq!(neighbors_a, vec!["ent_b"]);

        // ent_b neighbors: both ent_a (back-edge) and ent_c (forward-edge) — undirected.
        let mut neighbors_b = graph.neighbors("ent_b");
        neighbors_b.sort();
        assert_eq!(neighbors_b, vec!["ent_a", "ent_c"]);

        // ent_c neighbors: only ent_b (back-edge B->C visited).
        let mut neighbors_c = graph.neighbors("ent_c");
        neighbors_c.sort();
        assert_eq!(neighbors_c, vec!["ent_b"]);
    }
}
