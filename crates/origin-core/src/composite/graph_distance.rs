// SPDX-License-Identifier: Apache-2.0
//! Graph-distance scorer: BFS via recursive CTE over `relations`, returns
//! `HashMap<memory_source_id, min_hop_distance>` for all memories reachable
//! from a set of seed entities within `max_depth` hops.
//!
//! The CTE is a UNION ALL split into two arms (forward + backward edges) so
//! both `idx_relations_from` and `idx_relations_to` are exercised by the
//! query planner.

use std::collections::HashMap;

use crate::db::MemoryDB;
use crate::OriginError;

/// Compute the minimum hop distance (via the knowledge graph) from any entity
/// in `seed_entity_ids` to every memory reachable within `max_depth` steps.
///
/// Returns a map from `memory_source_id` → distance (0 = the memory is
/// directly linked to one of the seeds, 1 = one hop away, …).
///
/// Traversal is bidirectional: both `from_entity→to_entity` and
/// `to_entity→from_entity` edges are followed so the graph is treated as
/// undirected.
///
/// Returns `Ok(HashMap::new())` immediately when `seed_entity_ids` is empty.
// Plan B Task 9 wires this into the composite scorer.
#[allow(dead_code)]
pub(crate) async fn compute_graph_distance(
    db: &MemoryDB,
    seed_entity_ids: &[&str],
    max_depth: u8,
) -> Result<HashMap<String, u8>, OriginError> {
    if seed_entity_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let conn = db.conn.lock().await;

    let placeholders = seed_entity_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");

    let sql = format!(
        "WITH RECURSIVE hop_distance(entity_id, distance) AS (
            SELECT id AS entity_id, 0 AS distance
            FROM entities WHERE id IN ({placeholders})
          UNION ALL
            SELECT r.to_entity, h.distance + 1
            FROM hop_distance h
            JOIN relations r ON r.from_entity = h.entity_id
            WHERE h.distance < ?
          UNION ALL
            SELECT r.from_entity, h.distance + 1
            FROM hop_distance h
            JOIN relations r ON r.to_entity = h.entity_id
            WHERE h.distance < ?
        ),
        reachable AS (
            SELECT entity_id, MIN(distance) AS d FROM hop_distance GROUP BY entity_id
        )
        SELECT me.memory_id, r.d AS distance
        FROM reachable r
        JOIN memory_entities me ON me.entity_id = r.entity_id"
    );

    // Bind: seed ids first, then max_depth twice (once per UNION ALL arm).
    let mut params: Vec<libsql::Value> = seed_entity_ids
        .iter()
        .map(|s| libsql::Value::Text(s.to_string()))
        .collect();
    params.push(libsql::Value::Integer(max_depth as i64));
    params.push(libsql::Value::Integer(max_depth as i64));

    let mut rows = conn
        .query(&sql, params)
        .await
        .map_err(|e| OriginError::VectorDb(format!("graph_distance query: {e}")))?;

    let mut out: HashMap<String, u8> = HashMap::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|e| OriginError::VectorDb(format!("graph_distance row: {e}")))?
    {
        let mid = row
            .get::<String>(0)
            .map_err(|e| OriginError::VectorDb(format!("graph_distance col0: {e}")))?;
        let d = row
            .get::<i64>(1)
            .map_err(|e| OriginError::VectorDb(format!("graph_distance col1: {e}")))?
            as u8;
        out.entry(mid)
            .and_modify(|v| {
                if d < *v {
                    *v = d
                }
            })
            .or_insert(d);
    }
    Ok(out)
}

/// Convert a hop distance to a score in (0, 1].
///
/// d=0 → 1.0, d=1 → 0.5, d=2 → 0.33, …
// Plan B Task 9 wires this into the composite scorer.
#[allow(dead_code)]
pub(crate) fn graph_distance_score(d: u8) -> f64 {
    1.0 / (1.0 + d as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn graph_distance_depth_zero_one_two() {
        let (db, _dir) = crate::db::tests::test_db().await;
        let conn = db.conn.lock().await;

        // Build entities A -- knows --> B -- worked_at --> C (depth 0=A, 1=B, 2=C).
        // D is isolated.
        for (id, name) in [
            ("ent_a", "A"),
            ("ent_b", "B"),
            ("ent_c", "C"),
            ("ent_d", "D"),
        ] {
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

        // Two memories: one linked to ent_c, one linked to isolated ent_d.
        conn.execute(
            "INSERT INTO memories (id, content, source, source_id, title, chunk_index, last_modified, chunk_type) VALUES ('c_c', '', 'memory', 'mem_c', '', 0, 0, 'text')",
            (),
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO memories (id, content, source, source_id, title, chunk_index, last_modified, chunk_type) VALUES ('c_d', '', 'memory', 'mem_d', '', 0, 0, 'text')",
            (),
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO memory_entities (memory_id, entity_id) VALUES ('mem_c', 'ent_c')",
            (),
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO memory_entities (memory_id, entity_id) VALUES ('mem_d', 'ent_d')",
            (),
        )
        .await
        .unwrap();

        // Drop the lock before calling compute_graph_distance (which takes its own lock).
        drop(conn);

        let distances = compute_graph_distance(&db, &["ent_a"], 2)
            .await
            .expect("compute");

        // mem_c reached at distance 2; mem_d unreachable within max_depth=2.
        assert_eq!(distances.get("mem_c"), Some(&2));
        assert!(!distances.contains_key("mem_d"));
    }

    #[tokio::test]
    async fn graph_distance_caps_at_max_depth() {
        let (db, _dir) = crate::db::tests::test_db().await;
        let conn = db.conn.lock().await;

        // A->B->C->D chain; seed A, max_depth=2; D should NOT appear.
        for (id, name) in [("ea", "A"), ("eb", "B"), ("ec", "C"), ("ed", "D")] {
            conn.execute(
                "INSERT INTO entities (id, name, entity_type, created_at, updated_at) VALUES (?, ?, 'Topic', 0, 0)",
                [id, name],
            )
            .await
            .unwrap();
        }
        conn.execute(
            "INSERT INTO relations (id, from_entity, to_entity, relation_type, created_at) VALUES ('ra', 'ea', 'eb', 'knows', 0)",
            (),
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO relations (id, from_entity, to_entity, relation_type, created_at) VALUES ('rb', 'eb', 'ec', 'knows', 0)",
            (),
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO relations (id, from_entity, to_entity, relation_type, created_at) VALUES ('rc', 'ec', 'ed', 'knows', 0)",
            (),
        )
        .await
        .unwrap();

        // Memory linked to D (depth 3 from A).
        conn.execute(
            "INSERT INTO memories (id, content, source, source_id, title, chunk_index, last_modified, chunk_type) VALUES ('md', '', 'memory', 'src_d', '', 0, 0, 'text')",
            (),
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO memories (id, content, source, source_id, title, chunk_index, last_modified, chunk_type) VALUES ('mc', '', 'memory', 'src_c', '', 0, 0, 'text')",
            (),
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO memory_entities (memory_id, entity_id) VALUES ('src_d', 'ed')",
            (),
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO memory_entities (memory_id, entity_id) VALUES ('src_c', 'ec')",
            (),
        )
        .await
        .unwrap();

        drop(conn);

        let distances = compute_graph_distance(&db, &["ea"], 2)
            .await
            .expect("compute");

        // C is at depth 2 (reachable); D is at depth 3 (beyond max_depth=2).
        assert_eq!(distances.get("src_c"), Some(&2));
        assert!(!distances.contains_key("src_d"));
    }

    #[tokio::test]
    async fn graph_distance_handles_cycles() {
        let (db, _dir) = crate::db::tests::test_db().await;
        let conn = db.conn.lock().await;

        // A -> B, B -> A (cycle). Seed A, max_depth=3; should not infinite-loop.
        for (id, name) in [("ca", "A"), ("cb", "B")] {
            conn.execute(
                "INSERT INTO entities (id, name, entity_type, created_at, updated_at) VALUES (?, ?, 'Topic', 0, 0)",
                [id, name],
            )
            .await
            .unwrap();
        }
        conn.execute(
            "INSERT INTO relations (id, from_entity, to_entity, relation_type, created_at) VALUES ('cy1', 'ca', 'cb', 'knows', 0)",
            (),
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO relations (id, from_entity, to_entity, relation_type, created_at) VALUES ('cy2', 'cb', 'ca', 'knows', 0)",
            (),
        )
        .await
        .unwrap();

        // Memory linked to B.
        conn.execute(
            "INSERT INTO memories (id, content, source, source_id, title, chunk_index, last_modified, chunk_type) VALUES ('mb', '', 'memory', 'src_b', '', 0, 0, 'text')",
            (),
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO memory_entities (memory_id, entity_id) VALUES ('src_b', 'cb')",
            (),
        )
        .await
        .unwrap();

        drop(conn);

        // Should complete without hanging; B is at distance 1 from A.
        let distances = compute_graph_distance(&db, &["ca"], 3)
            .await
            .expect("compute");

        assert_eq!(distances.get("src_b"), Some(&1));
    }

    #[tokio::test]
    async fn graph_distance_uses_both_relations_indexes() {
        let (db, _dir) = crate::db::tests::test_db().await;
        let conn = db.conn.lock().await;

        // Seed 5000 forward edges so the planner has rows enough to choose indexes.
        conn.execute("BEGIN", ()).await.unwrap();
        for i in 0..5000_u32 {
            let eid = format!("idx_e{i}");
            conn.execute(
                "INSERT INTO entities (id, name, entity_type, created_at, updated_at) VALUES (?, ?, 'Topic', 0, 0)",
                libsql::params![eid.as_str(), eid.as_str()],
            )
            .await
            .unwrap();
        }
        // Insert enough relations to encourage index use (star topology from e0).
        for i in 1..5000_u32 {
            let rid = format!("idx_r{i}");
            let to = format!("idx_e{i}");
            conn.execute(
                "INSERT INTO relations (id, from_entity, to_entity, relation_type, created_at) VALUES (?, 'idx_e0', ?, 'knows', 0)",
                libsql::params![rid.as_str(), to.as_str()],
            )
            .await
            .unwrap();
        }
        conn.execute("COMMIT", ()).await.unwrap();

        // Build the same SQL the function uses (one seed, max_depth=1).
        let sql = "WITH RECURSIVE hop_distance(entity_id, distance) AS (
            SELECT id AS entity_id, 0 AS distance
            FROM entities WHERE id IN (?)
          UNION ALL
            SELECT r.to_entity, h.distance + 1
            FROM hop_distance h
            JOIN relations r ON r.from_entity = h.entity_id
            WHERE h.distance < ?
          UNION ALL
            SELECT r.from_entity, h.distance + 1
            FROM hop_distance h
            JOIN relations r ON r.to_entity = h.entity_id
            WHERE h.distance < ?
        ),
        reachable AS (
            SELECT entity_id, MIN(distance) AS d FROM hop_distance GROUP BY entity_id
        )
        SELECT me.memory_id, r.d AS distance
        FROM reachable r
        JOIN memory_entities me ON me.entity_id = r.entity_id";

        let mut rows = conn
            .query(
                &format!("EXPLAIN QUERY PLAN {sql}"),
                libsql::params!["idx_e0", 1_i64, 1_i64],
            )
            .await
            .unwrap();

        let mut plan = String::new();
        while let Some(row) = rows.next().await.unwrap() {
            let detail = row.get::<String>(3).unwrap_or_default();
            if !detail.is_empty() {
                plan.push_str(&detail);
                plan.push('\n');
            }
        }

        assert!(
            plan.contains("idx_relations_from"),
            "plan missing idx_relations_from:\n{plan}"
        );
        assert!(
            plan.contains("idx_relations_to"),
            "plan missing idx_relations_to:\n{plan}"
        );
    }
}
