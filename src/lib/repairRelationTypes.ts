// SPDX-License-Identifier: AGPL-3.0-only
/**
 * Entity-relation repair wire contracts.
 * Mirrors `crates/wenlan-types/src/repair_relation.rs`: the reviewer's chosen
 * repair action for an entity-relation lint finding, the review-scoped
 * selection wrapping it, and the canonical manifest-level mutation. Canonical
 * relation-type vocabulary lookup, snapshot resolution, and the actual
 * write/apply path belong to the daemon; these types only validate and
 * round-trip the requested change.
 *
 * The frontend never consumes raw rollback snapshots (lossless SQL row
 * capture for the daemon's future V3 rollback); it only observes rollback
 * *metadata* through `RepairRollbackArtifact`, where the entity-relation
 * family carries `format_version` 3.
 */

/** Mirrors `EntityRelationRepairChoice`: `kind` serializes as `add`/`retire`. */
export type EntityRelationRepairChoice =
  | {
      kind: "add";
      from_entity: string;
      to_entity: string;
      relation_type: string;
      source_memory_id?: string | null;
    }
  | {
      kind: "retire";
      relation_id: string;
    };

/** Mirrors `EntityRelationRepairSelection`: which review this answers. */
export interface EntityRelationRepairSelection {
  review_id: string;
  choice: EntityRelationRepairChoice;
}

/**
 * Mirrors `RepairRelationMutation`: the manifest-level mutation for an
 * entity-relation repair. A requested type promoted into the shared vocabulary
 * normalizes to the canonical fallback `related_to`; `retire_relation_ids`
 * is sorted unique (empty for a fresh add).
 */
export type RepairRelationMutation =
  | {
      kind: "add";
      requested_relation_type: string;
      canonical_relation_type: string;
      source_memory_id?: string | null;
      confidence_basis_points: number;
      retire_relation_ids: string[];
      vocabulary_promotion?: string | null;
    }
  | { kind: "retire" };
