// SPDX-License-Identifier: AGPL-3.0-only
import { getEntityDetail, getMemoryDetail, type Entity, type GraphRelation, type MemoryItem } from "./tauri";

/** Display data for the exact queued owners. Fresh semantic authorization and
 * scope validation remain in daemon preparation, never in this display lookup. */
export interface RelationRepairSources {
  entities: Entity[];
  memories: MemoryItem[];
  relations: GraphRelation[];
}

export async function loadRelationRepairSources(ownerIds: readonly string[]): Promise<RelationRepairSources> {
  if (ownerIds.length < 2 || new Set(ownerIds).size !== ownerIds.length) {
    throw new Error("relation review owners are invalid");
  }
  const owners = await Promise.all(ownerIds.map(async (id) => {
    // Queue owners have durable IDs but no kind discriminator. Resolve only
    // these IDs through the existing typed readers, and refuse ambiguous IDs.
    const [entity, memory] = await Promise.allSettled([getEntityDetail(id), getMemoryDetail(id)]);
    const detail = entity.status === "fulfilled" && entity.value?.entity?.id === id ? entity.value : null;
    const source = memory.status === "fulfilled" && memory.value?.source_id === id ? memory.value : null;
    if (!!detail === !!source) throw new Error("relation review source is unavailable or ambiguous");
    if (detail?.entity.status === "archived") throw new Error("relation review entity is archived");
    return { detail, source };
  }));
  const entities = owners.flatMap(({ detail }) => detail ? [detail.entity] : []);
  if (entities.length < 2) throw new Error("relation review endpoints are unavailable");
  const entityIds = new Set(entities.map((entity) => entity.id));
  const relations = new Map<string, GraphRelation>();
  for (const { detail } of owners) {
    if (!detail) continue;
    for (const relation of detail.relations) {
      if (!entityIds.has(relation.entity_id)) continue;
      if (relation.direction !== "incoming" && relation.direction !== "outgoing") {
        throw new Error("relation direction is unavailable");
      }
      const edge: GraphRelation = {
        id: relation.id,
        from_entity: relation.direction === "outgoing" ? detail.entity.id : relation.entity_id,
        to_entity: relation.direction === "outgoing" ? relation.entity_id : detail.entity.id,
        relation_type: relation.relation_type,
        source_agent: relation.source_agent,
        created_at: relation.created_at,
      };
      const prior = relations.get(edge.id);
      if (prior && (prior.from_entity !== edge.from_entity || prior.to_entity !== edge.to_entity || prior.relation_type !== edge.relation_type)) {
        throw new Error("relation endpoints changed while loading");
      }
      relations.set(edge.id, edge);
    }
  }
  return { entities, memories: owners.flatMap(({ source }) => source ? [source] : []), relations: [...relations.values()] };
}
