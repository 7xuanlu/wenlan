// SPDX-License-Identifier: Apache-2.0
//! Reweave phase: re-link existing entities across memories using updated graph state.

use crate::db::MemoryDB;
use crate::error::OriginError;

/// Reweave entity links: find memories with no entity_id and try to match them
/// against existing entities via vector similarity.
pub async fn reweave_entity_links(
    db: &MemoryDB,
    limit: usize,
    entity_link_distance: f64,
) -> Result<usize, OriginError> {
    let unlinked = db.get_unlinked_memories(limit).await?;
    let mut linked = 0usize;
    for (source_id, content) in &unlinked {
        let entities = db.search_entities_by_vector(content, 3).await?;
        for entity in &entities {
            if entity.distance < entity_link_distance as f32 {
                db.update_memory_entity_id(source_id, &entity.entity.id)
                    .await?;
                linked += 1;
                break;
            }
        }
    }
    if linked > 0 {
        log::info!("[refinery] reweave: linked {} memories to entities", linked);
    }
    Ok(linked)
}
