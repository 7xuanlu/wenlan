// SPDX-License-Identifier: AGPL-3.0-only
/**
 * JSON contracts for the daemon's approval-gated repair plane.
 * Keep this aligned with `crates/wenlan-types/src/repair.rs` and the lint
 * contract. Reports and manifests are producer evidence; the UI only selects
 * a choice and never manufactures a receipt.
 */

export type RepairDigest = string;
export type LintDigest = string;

export type RepairLintScope =
  | { kind: "global" }
  | { kind: "registered"; space: string }
  | { kind: "uncategorized" };

export type RepairScope = RepairLintScope;

export type LintScope =
  | { kind: "global" }
  | { kind: "registered"; opaque_scope_ref: number }
  | { kind: "uncategorized" };

export type LintCapabilityContext = "daemon_operator_endpoint_unauthenticated_unverified";

export type LintSnapshotReceipts = {
  db: {
    mode: "transactional_read_only";
    analysis_digest: LintDigest;
    post_run_digest: LintDigest | null;
  };
  pages: {
    mode: "best_effort";
    before_scan_digest: LintDigest;
    after_scan_digest: LintDigest | null;
  };
};

export interface LintProducerReceipt {
  runtime_commit: string | null;
}

export type LintValidationMethod = "exact_aggregate" | "full_enumeration" | "intrinsic_sample";

export interface LintCoverage {
  method: LintValidationMethod;
  authorized_denominator: number;
  evaluated: number;
  evidence_cap: number;
  truncated: boolean;
  evidence_returned: number;
}

export type LintMetricValue =
  | { kind: "count"; value: number }
  | { kind: "boolean"; value: boolean }
  | { kind: "catalog_code"; code: string };

export interface LintMetric {
  code: string;
  value: LintMetricValue;
}

export type LintSummaryCode =
  | "check_passed"
  | "finding_detected"
  | "prerequisite_unavailable"
  | "snapshot_inconsistent"
  | "execution_failed"
  | "expected_empty";

export type LintRecommendationCode =
  | "review_finding"
  | "restore_prerequisite"
  | "rerun_after_snapshot_stabilizes"
  | "inspect_runtime";

export type LintActionCode = "choose_model_source";

export interface RepairLintQuery {
  profile: "general" | "deep";
  space?: string;
  external_egress?: boolean;
  agent_assist?: boolean;
}

export type LintSemanticAction =
  | "reclassify_memory"
  | "review_contradiction"
  | "review_staleness"
  | "supersede_memory"
  | "add_memory_entity_link"
  | "remove_memory_entity_link"
  | "add_entity_relation"
  | "remove_entity_relation"
  | "review_page_claim"
  | "add_page_evidence"
  | "remove_page_evidence"
  | "review_retrieval";

export type LintSemanticReasonCode =
  | "classification_mismatch"
  | "potential_contradiction"
  | "potential_staleness"
  | "mention_without_link"
  | "existing_link_mismatch"
  | "shared_context_without_relation"
  | "existing_relation_mismatch"
  | "potential_unfaithful_claim"
  | "potential_inadequate_provenance"
  | "claim_overlap_without_evidence"
  | "existing_evidence_mismatch"
  | "potential_retrieval_miss"
  | "dangling_owner"
  | "temporal_evolution"
  | "related_but_not_evidence";

export type LintSemanticProviderRoute =
  | "on_device"
  | "configured_external"
  | "calling_agent";

export interface RepairSemanticFinding {
  candidate_id: number;
  proposed_action: LintSemanticAction;
  reason_code: LintSemanticReasonCode;
  confidence_basis_points: number;
  provider_route: LintSemanticProviderRoute;
  /** LintDigest is the 16-character semantic record digest. */
  evidence_ids: LintDigest[];
  counterevidence_ids: LintDigest[];
  unresolved_disagreement: boolean;
}

export type LintEvidenceRef =
  | { kind: "opaque_id"; opaque_id: number }
  | { kind: "opaque_digest"; opaque_digest: string }
  | { kind: "reason_code"; reason_code: string }
  | { kind: "safe_root_relative_path"; safe_root_relative_path: string }
  | { kind: "semantic_finding"; finding: RepairSemanticFinding };

export interface LintCheckResult {
  check_id: string;
  outcome:
    | "pass"
    | "finding"
    | "not_run_prerequisite"
    | "inconsistent_snapshot"
    | "failed_to_run";
  gate_effect: "actionable" | "advisory";
  severity: "info" | "warning" | "error";
  applicability: "applicable" | "inventory" | "expected_empty" | "not_applicable";
  precondition:
    | "ready"
    | "expected_empty"
    | "configured_off"
    | "missing_prerequisite"
    | "snapshot_unstable";
  coverage: LintCoverage;
  metrics: LintMetric[];
  summary_code: LintSummaryCode;
  recommendation_code?: LintRecommendationCode | null;
  action_code?: LintActionCode | null;
  evidence: LintEvidenceRef[];
  duration_ms: number;
  [key: string]: unknown;
}

export interface RepairLintReport {
  report_schema_version: number;
  check_catalog_version: number;
  profile: "general" | "deep";
  scope: LintScope;
  capability_context: LintCapabilityContext;
  snapshots: LintSnapshotReceipts;
  config_fingerprint: LintDigest;
  producer_receipt: LintProducerReceipt;
  agent_work?: {
    work_schema_version: number;
    work_digest: LintDigest;
    populations: Array<Record<string, unknown>>;
    records: Array<Record<string, unknown>>;
    candidates: Array<Record<string, unknown>>;
  } | null;
  checks: LintCheckResult[];
  totals: {
    checks: number;
    passed: number;
    findings: number;
    actionable_findings: number;
    advisory_findings: number;
    incomplete: number;
  };
  complete: boolean;
  [key: string]: unknown;
}

export type RepairMemoryType =
  | "identity"
  | "preference"
  | "decision"
  | "lesson"
  | "gotcha"
  | "fact";

export type RepairChoice =
  | {
      kind: "reclassify_memory";
      selected_finding: RepairSemanticFinding;
      after_memory_type: RepairMemoryType;
    }
  | {
      kind: "rename_page_title";
      review_id: string;
      page_id: string;
      before_title: string;
      after_title: string;
    }
  | {
      kind: "complete_entity_extraction";
      review_id: string;
      memory_id: string;
      entity_ids: string[];
    };

/** Server refreshes evidence and prepares while holding its analysis guard. */
export type CurrentRepairChoice =
  | { kind: "reclassify_memory"; review_id: string; memory_id: string; after_memory_type: RepairMemoryType }
  | { kind: "rename_page_title"; review_id: string; page_id: string; before_title: string; after_title: string }
  | { kind: "complete_entity_extraction"; review_id: string; memory_id: string; entity_ids: string[] };

export interface PrepareCurrentRepairRequest {
  lint_scope: RepairLintScope;
  choice: CurrentRepairChoice;
}

export interface PrepareRepairRequest {
  lint_scope: RepairLintScope;
  general_report: RepairLintReport;
  deep_report?: RepairLintReport;
  choice: RepairChoice;
}

export type RepairTarget =
  | { kind: "memory"; source_id: string; scope: RepairScope }
  | { kind: "memory_entity_link"; memory_id: string; entity_id: string; scope: RepairScope }
  | {
      kind: "memory_entity_extraction";
      memory_id: string;
      step: "entity_extract";
      entity_ids: string[];
      scope: RepairScope;
    }
  | { kind: "tag"; source: string; source_id: string; tag: string; scope: RepairScope }
  | { kind: "page_link"; source_page_id: string; label_key: string; scope: RepairScope }
  | { kind: "page"; page_id: string; scope: RepairScope }
  | { kind: "page_projection"; page_id: string; scope: RepairScope };

export type RepairWriter =
  | "reclassify_memory"
  | "rename_page_title"
  | "complete_entity_extraction"
  | "normalize_memory_source_agent"
  | "clear_memory_supersedes"
  | "unstage_orphan_revision"
  | "delete_tag_row"
  | "delete_memory_entity_link"
  | "bind_page_link"
  | "archive_empty_source_page"
  | "regenerate_page_projection"
  | "quarantine_stale_page_projection";

export type RepairMutation =
  | {
      kind: "reclassify_memory";
      before_memory_type: RepairMemoryType | null;
      after_memory_type: RepairMemoryType;
    }
  | {
      kind: "rename_page_title";
      before_title: string;
      after_title: string;
      after_embedding_hex: string;
    }
  | { kind: "complete_entity_extraction"; entity_ids: string[] }
  | { kind: "normalize_memory_source_agent"; before_source_agent: string }
  | { kind: "clear_memory_supersedes"; before_supersedes: string }
  | { kind: "unstage_orphan_revision" }
  | { kind: "delete_tag_row"; source: string; source_id: string; tag: string }
  | { kind: "delete_memory_entity_link"; memory_id: string; entity_id: string }
  | { kind: "bind_page_link"; before_target_page_id: string | null; after_target_page_id: string }
  | { kind: "archive_empty_source_page"; before_status: string; after_status: string }
  | { kind: "regenerate_page_projection"; database_version: number }
  | { kind: "quarantine_stale_page_projection"; source_path: string; quarantine_path: string };

export type RepairMemoryField =
  | "memory_type"
  | "source_agent"
  | "supersedes"
  | "pending_revision"
  | "tag_row"
  | "memory_entity_link"
  | "memory_entity_links"
  | "enrichment_step"
  | "entity_establishment"
  | "target_page_id"
  | "page_status"
  | "page_title"
  | "page_version"
  | "page_embedding"
  | "page_projection"
  | "page_projection_quarantine";

export interface RepairAllowedEffects {
  owner: RepairTarget;
  fields: RepairMemoryField[];
}

export interface RepairReviewBinding {
  review_id: string;
  occurrence_digest: RepairDigest;
  owner_ids: string[];
}

export interface RepairSource {
  report_schema_version: number;
  check_catalog_version: number;
  lint_scope: RepairLintScope;
  report_scope: LintScope;
  check_id: string;
  finding?: RepairSemanticFinding | null;
  deterministic_evidence?: LintEvidenceRef[];
  general_snapshots: Record<string, unknown>;
  deep_snapshots?: Record<string, unknown> | null;
  general_producer_receipt: { runtime_commit: string | null };
  deep_producer_receipt?: { runtime_commit: string | null } | null;
  agent_work_digest?: LintDigest | null;
  review_binding?: RepairReviewBinding | null;
}

export interface RepairExpectedState {
  /** Omitted by the daemon for targets without a version column. */
  version?: number | null;
  canonical_receipt: RepairDigest;
}

export interface RepairCheckBaseline {
  check_id: string;
  outcome: string;
  gate_effect: string;
  affected_records?: number | null;
  evidence: LintEvidenceRef[];
}

export type RepairVerificationPolicy =
  | { kind: "legacy_whole_reports" }
  | { kind: "applicable_checks"; required_deep_check_ids: string[] }
  | { kind: "general_only" };

export interface RepairPostAssertions {
  target_check_id: string;
  target_evidence_id: RepairDigest;
  general_baseline: RepairCheckBaseline[];
  deep_baseline: RepairCheckBaseline[];
  target_record_set?: { record_count: number; digest: RepairDigest } | null;
  verification_policy: RepairVerificationPolicy;
  require_complete_general: boolean;
  reject_new_actionable: boolean;
  reject_new_incomplete: boolean;
  allowed_non_target_check_deltas: string[];
}

export interface RepairRollbackArtifact {
  format_version: number;
  relative_path: string;
  digest: RepairDigest;
}

export interface RepairManifest {
  manifest_schema_version: number;
  manifest_id: string;
  prepared_at: number;
  source: RepairSource;
  target: RepairTarget;
  expected_state: RepairExpectedState;
  writer: RepairWriter;
  mutation: RepairMutation;
  allowed_effects: RepairAllowedEffects;
  rollback: RepairRollbackArtifact;
  post_assertions: RepairPostAssertions;
  manifest_digest: RepairDigest;
  [key: string]: unknown;
}

export interface ApplyRepairRequest {
  manifest_id: string;
  approved_manifest_digest: RepairDigest;
  approval: string;
}

export interface RepairOperationStatus {
  manifest_id: string;
  manifest_digest: RepairDigest;
  state:
    | { phase: "prepared" }
    | { phase: "in_progress" }
    | { phase: "indeterminate" }
    | { phase: "applied_unverified"; apply_receipt: RepairApplyReceipt }
    | { phase: "verified"; apply_receipt: RepairApplyReceipt; verification_receipt: RepairVerificationReceipt }
    | { phase: "cancelled"; cancelled_at: number };
}

export interface RepairApplyReceipt {
  receipt_schema_version: number;
  manifest_id: string;
  manifest_digest: RepairDigest;
  applied_at: number;
  before_target_receipt: RepairDigest;
  after_target_receipt: RepairDigest;
  non_target_before: RepairDigest;
  non_target_after: RepairDigest;
  post_apply_db_digest?: RepairDigest | null;
  actual_effects: RepairAllowedEffects;
  writer: RepairWriter;
  receipt_digest: RepairDigest;
  [key: string]: unknown;
}

export interface RepairRecovery {
  manifest: RepairManifest;
  apply_receipt: RepairApplyReceipt | null;
}

export interface VerifyRepairRequest {
  manifest_id: string;
  manifest_digest: RepairDigest;
  apply_receipt_digest: RepairDigest;
  general_report: RepairLintReport;
  deep_report?: RepairLintReport;
  next_apply?: ApplyRepairRequest | null;
}

export interface RepairVerificationReceipt {
  receipt_schema_version: number;
  manifest_id: string;
  manifest_digest: RepairDigest;
  apply_receipt_digest: RepairDigest;
  verified_at: number;
  general_snapshots: Record<string, unknown>;
  deep_snapshots?: Record<string, unknown> | null;
  receipt_digest: RepairDigest;
  [key: string]: unknown;
}

/**
 * UI-authored approval to resume normal runtime after a terminal
 * verification receipt. Mirrors `RepairRuntimeResumeApproval` in
 * `crates/wenlan-types/src/repair_runtime.rs`. Native code measures process
 * identity itself and only restarts positively owned services; the UI never
 * chooses an instance or PID.
 */
export interface RepairRuntimeResumeApproval {
  apply: ApplyRepairRequest;
  verification_receipt_digest: RepairDigest;
}

export interface RepairPlanRequest {
  scope: RepairLintScope;
  general_report: RepairLintReport;
  deep_report?: RepairLintReport;
}

export interface RepairPlanSummary {
  plan_id: string;
  plan_digest: RepairDigest;
  scope: RepairLintScope;
  entry_count: number;
  deterministic_complete: boolean;
  semantic_complete: boolean;
}

export interface RepairPlanEntry {
  check_id: string;
  occurrence_digest: RepairDigest;
  affected_records: Array<{ kind: string; durable_id: string }>;
  resolution:
    | { disposition: "ready"; manifest: RepairManifest }
    | {
        disposition: "review";
        review_item: {
          review_id: string;
          check_id: string;
          issue: string;
          choices: string[];
          suggested_research_queries: string[];
        };
      }
    | {
        disposition: "blocked";
        blocked: { reason_code: string; detail: string; next_action: string };
      }
    | { disposition: "system_action"; system_action: { kind: string; summary: string; evidence: string[] } };
}

export interface RepairPlanEntriesRequest {
  plan_id: string;
  plan_digest: RepairDigest;
  offset: number;
  limit: number;
}

export interface RepairPlanEntriesPage {
  plan_id: string;
  plan_digest: RepairDigest;
  scope: RepairLintScope;
  offset: number;
  next_offset?: number | null;
  total_entries: number;
  entries: RepairPlanEntry[];
}


export interface RepairPrepareOperationRequest {
  operation_id: string;
  request: PrepareCurrentRepairRequest;
}

export interface RepairPrepareOperationStatus {
  operation_id: string;
  state:
    | { phase: "not_started" }
    | { phase: "in_progress" }
    | { phase: "interrupted" }
    | { phase: "ready"; manifest: RepairManifest; operation: RepairOperationStatus }
    | { phase: "cancelled"; cancelled_at: number };
}
