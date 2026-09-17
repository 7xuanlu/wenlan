// SPDX-License-Identifier: AGPL-3.0-only
import type { RefinementProposalSummary } from "../../src/lib/tauri";
import { createSpacesNavigationFixture, type SpacesNavigationFixture } from "./spacesNavigation";

/** Review-only, deterministic scenarios. The ordinary fixture stays unchanged
 * unless a named scenario is requested; no daemon or personal data is read. */
export function createReviewDecisionFixture(scenario: string): SpacesNavigationFixture {
  const original = createSpacesNavigationFixture();
  const vocabularyEntity = { ...original.entities[0], id: "entity-review-method", name: "Source triangulation", entity_type: "concept" };
  const base = scenario === "all" || scenario === "vocab_promote" ? {
    ...original,
    entities: [...original.entities, vocabularyEntity],
    entityDetails: [...original.entityDetails, { entity: vocabularyEntity, observations: [], relations: [] }],
  } : original;
  const common = { confidence: 0.82, created_at: "2026-09-13T10:00:00Z" };
  const proposals: RefinementProposalSummary[] = [
    { ...common, id: "review-archive", action: "page_keep_or_archive", source_ids: ["page-history"], payload: { action: "page_keep_or_archive", page_id: "page-history", source_count: 1, allowed_actions: ["accept", "dismiss"] } },
    { ...common, id: "review-page-merge", action: "page_merge", source_ids: ["page-architecture", "page-errors"], payload: { action: "page_merge", left_page_id: "page-architecture", right_page_id: "page-errors", source_overlap: 0, source_overlap_ratio: 0 } },
    { ...common, id: "review-entity-merge", action: "entity_merge", source_ids: ["entity-babbage", "entity-ada"], payload: { action: "entity_merge", existing_id: "entity-ada", new_id: "entity-babbage", similarity: 0.82 } },
    { ...common, id: "review-relation", action: "relation_conflict", source_ids: ["relation-new", "relation-1"], payload: { action: "relation_conflict", existing_id: "relation-1", new_id: "relation-new", from: "Ada Lovelace", to: "Charles Babbage", old_type: "collaborated with", new_type: "studied with" } },
    { ...common, id: "review-contradiction", action: "detect_contradiction", source_ids: ["memory-1", "memory-0"], payload: { action: "detect_contradiction" } },
    { ...common, id: "review-suggestion", action: "suggest_entity", source_ids: ["memory-0"], payload: { action: "suggest_entity", name_hint: "Source integrity" } },
    { ...common, id: "review-dedup", action: "dedup_merge", source_ids: ["memory-1", "memory-0"], payload: { action: "dedup_merge" } },
    { ...common, id: "review-cross-space", action: "cross_space_discovery", source_ids: ["memory-0", "memory-1"], payload: { action: "cross_space_discovery", memory_count: 2, spaces: ["Wenlan", "Research"] } },
    { ...common, id: "review-repair", action: "lint_repair_review", source_ids: ["memory-0"], payload: { action: "lint_repair_review", check_id: "source-check", occurrence_digest: "a".repeat(64), owner_binding_digest: "b".repeat(64), issue: "The quoted source no longer matches the recorded text.", choices: ["Keep the original quotation", "Research the changed source"], suggested_research_queries: [] } },
    { ...common, id: "review-vocabulary", action: "vocab_promote", source_ids: ["entity-review-method", "entity-ada"], payload: { action: "vocab_promote", kind: "entity", old_value: "research-method", category: "concept" } },
    { ...common, id: "review-unknown", action: "unknown", source_ids: [], payload: null },
  ];
  const selected = proposals.filter((proposal) => proposal.action === scenario);
  if (scenario === "missing-target") return { ...base, pages: base.pages.filter((page) => page.id !== "page-history") };
  if (selected.length) return { ...base, refinements: selected };
  if (scenario !== "all") return base;
  return { ...base, refinements: proposals,
    distillReview: { ...base.distillReview,
      stale_pages: [{ page_id: "page-cjk", title: "CJK layout", summary: "Source wording changed after compilation.", source_memory_ids: ["memory-4"], sources_updated_count: 1, stale_reason: "source_updated", user_edited: false }],
      orphan_topics: [{ label: "Source integrity", count: 3 }],
    }, pendingRevisions: [
    { target_source_id: "memory-0", revision_source_id: "revision-memory", revision_content: "Typed fixtures must agree with the daemon's target and source contracts.", target_kind: "memory", source_agent: "test-reviewer", last_modified: 1_789_295_400 },
    { target_source_id: "page-history", revision_source_id: "revision-page", revision_content: "# History semantics\n\nKeep original sources accessible before making a decision.", target_kind: "page", source_agent: "test-reviewer", last_modified: 1_789_295_400 },
  ] };
}
