// SPDX-License-Identifier: AGPL-3.0-only
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { sha256Hex, type RepairReviewIdentity } from "./repairWorkflow";
import {
  clearRepairPreparation,
  readRepairPreparation,
  validateRepairPreparationStatus,
  writeRepairPreparation,
  type RepairPreparationRecord,
} from "./repairPreparation";
import type {
  CurrentRepairChoice,
  RepairApplyReceipt,
  RepairManifest,
  RepairPrepareOperationRequest,
} from "./repairTypes";

const CHECK_CLASSIFICATION = "memories.semantic.classification";
const CHECK_DUPLICATE_TITLES = "pages.duplicate_active_titles";
const CHECK_ENRICHMENT = "memories.enrichment_failures";
const OPERATION_ID = "123e4567-e89b-42d3-a456-426614174000";
const OTHER_OPERATION_ID = "123e4567-e89b-42d3-a456-426614174001";

function identityFor(checkId: string, reviewId: string, owner: string): RepairReviewIdentity {
  return { reviewId, checkId, occurrenceDigest: "a".repeat(64), ownerIds: [owner] };
}

const classificationIdentity = identityFor(CHECK_CLASSIFICATION, "review-classification", "memory-1");
const titlesIdentity = identityFor(CHECK_DUPLICATE_TITLES, "review-titles", "page-1");
const enrichmentIdentity = identityFor(CHECK_ENRICHMENT, "review-enrichment", "memory-2");

function choiceFor(identity: RepairReviewIdentity): CurrentRepairChoice {
  if (identity.checkId === CHECK_CLASSIFICATION) {
    return { kind: "reclassify_memory", review_id: identity.reviewId, memory_id: identity.ownerIds[0], after_memory_type: "preference" };
  }
  if (identity.checkId === CHECK_DUPLICATE_TITLES) {
    return { kind: "rename_page_title", review_id: identity.reviewId, page_id: identity.ownerIds[0], before_title: "Before", after_title: "After" };
  }
  return { kind: "complete_entity_extraction", review_id: identity.reviewId, memory_id: identity.ownerIds[0], entity_ids: ["entity-1"] };
}

function operationFor(identity: RepairReviewIdentity, operationId = OPERATION_ID): RepairPrepareOperationRequest {
  return {
    operation_id: operationId,
    request: { lint_scope: { kind: "uncategorized" }, choice: choiceFor(identity) },
  };
}

function recordFor(identity: RepairReviewIdentity, operationId = OPERATION_ID): RepairPreparationRecord {
  return { ...identity, ownerIds: [...identity.ownerIds], version: 1, operation: operationFor(identity, operationId) };
}

function preparationKey(reviewId: string): string {
  return `wenlan.repair.prepare.v1:${encodeURIComponent(reviewId)}`;
}

async function manifestFor(identity: RepairReviewIdentity): Promise<RepairManifest> {
  const unsigned = {
    manifest_schema_version: 1,
    manifest_id: `manifest-${identity.reviewId}`,
    prepared_at: 123,
    source: {
      report_schema_version: 1,
      check_catalog_version: 1,
      lint_scope: { kind: "uncategorized" as const },
      report_scope: { kind: "uncategorized" as const },
      check_id: identity.checkId,
      finding: null,
      deterministic_evidence: [],
      general_snapshots: {},
      deep_snapshots: null,
      general_producer_receipt: { runtime_commit: null },
      deep_producer_receipt: null,
      agent_work_digest: null,
      review_binding: {
        review_id: identity.reviewId,
        occurrence_digest: identity.occurrenceDigest,
        owner_ids: [...identity.ownerIds],
      },
    },
    target: { kind: "memory" as const, source_id: identity.ownerIds[0], scope: { kind: "uncategorized" as const } },
    expected_state: { version: null, canonical_receipt: "b".repeat(64) },
    writer: "reclassify_memory" as const,
    mutation: { kind: "reclassify_memory" as const, before_memory_type: "fact" as const, after_memory_type: "preference" as const },
    allowed_effects: {
      owner: { kind: "memory" as const, source_id: identity.ownerIds[0], scope: { kind: "uncategorized" as const } },
      fields: ["memory_type" as const],
    },
    rollback: { format_version: 1, relative_path: "rollback.json", digest: "c".repeat(64) },
    post_assertions: {
      target_check_id: identity.checkId,
      target_evidence_id: "d".repeat(64),
      general_baseline: [],
      deep_baseline: [],
      target_record_set: null,
      verification_policy: { kind: "general_only" as const },
      require_complete_general: true,
      reject_new_actionable: true,
      reject_new_incomplete: true,
      allowed_non_target_check_deltas: [],
    },
  };
  return { ...unsigned, manifest_digest: await sha256Hex(JSON.stringify(unsigned)) } as RepairManifest;
}

function applyReceiptFor(manifest: RepairManifest): RepairApplyReceipt {
  return {
    receipt_schema_version: 1,
    manifest_id: manifest.manifest_id,
    manifest_digest: manifest.manifest_digest,
    applied_at: 456,
    before_target_receipt: "e".repeat(64),
    after_target_receipt: "f".repeat(64),
    non_target_before: "1".repeat(64),
    non_target_after: "2".repeat(64),
    actual_effects: manifest.allowed_effects,
    writer: manifest.writer,
    receipt_digest: "3".repeat(64),
  };
}

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("repair preparation durability", () => {
  it("round-trips the exact operation id and request for each current writer", () => {
    for (const identity of [classificationIdentity, titlesIdentity, enrichmentIdentity]) {
      const record = recordFor(identity);
      writeRepairPreparation(record);
      const read = readRepairPreparation(identity);
      expect(read.error).toBeNull();
      expect(read.record).toEqual(record);
      expect(read.record?.operation.operation_id).toBe(OPERATION_ID);
    }
  });

  it("rejects stale or differently bound records without erasing them", () => {
    const record = recordFor(classificationIdentity);
    writeRepairPreparation(record);
    // A review that was never prepared is missing, not stale.
    expect(readRepairPreparation(identityFor(CHECK_CLASSIFICATION, "unknown-review", "memory-1"))).toEqual({ record: null, error: null });
    const staleIdentities: RepairReviewIdentity[] = [
      { ...classificationIdentity, checkId: CHECK_DUPLICATE_TITLES },
      { ...classificationIdentity, occurrenceDigest: "9".repeat(64) },
      { ...classificationIdentity, ownerIds: ["memory-other"] },
      { ...classificationIdentity, ownerIds: ["memory-1", "memory-2"] },
    ];
    for (const stale of staleIdentities) {
      const read = readRepairPreparation(stale);
      expect(read.record).toBeNull();
      expect(read.error).toBeInstanceOf(Error);
    }
    // A record filed under another review key but bound to this review is
    // differently bound, not a usable preparation for that review.
    const otherReview = { ...classificationIdentity, reviewId: "other-review" };
    localStorage.setItem(preparationKey(otherReview.reviewId), JSON.stringify(record));
    const cross = readRepairPreparation(otherReview);
    expect(cross.record).toBeNull();
    expect(cross.error).toBeInstanceOf(Error);
    // The saved bytes are preserved for forensics, never erased silently.
    expect(localStorage.getItem(preparationKey(classificationIdentity.reviewId))).not.toBeNull();
    expect(localStorage.getItem(preparationKey(otherReview.reviewId))).not.toBeNull();
  });

  it("treats corrupt or malformed storage as an error and keeps it", () => {
    const record = recordFor(classificationIdentity);
    const malformed: unknown[] = [
      "{not json",
      { ...record, version: 2 },
      { ...record, operation: { ...record.operation, operation_id: "123E4567-E89B-42D3-A456-426614174000" } },
      { ...record, operation: { ...record.operation, operation_id: "not-a-uuid" } },
      { ...record, operation: { request: { lint_scope: { kind: "registered", space: "  " }, choice: choiceFor(classificationIdentity) } } },
      { ...record, operation: { request: { lint_scope: { kind: "uncategorized" }, choice: { ...choiceFor(classificationIdentity), review_id: "other-review" } } } },
      { ...record, operation: { request: { lint_scope: { kind: "uncategorized" }, choice: { kind: "quarantine_page" } } } },
      // A rename choice is not valid under the classification check.
      { ...record, operation: { request: { lint_scope: { kind: "uncategorized" }, choice: { kind: "rename_page_title", review_id: record.reviewId, page_id: "memory-1", before_title: "B", after_title: "A" } } } },
    ];
    for (const value of malformed) {
      localStorage.setItem(
        preparationKey(classificationIdentity.reviewId),
        typeof value === "string" ? value : JSON.stringify(value),
      );
      const read = readRepairPreparation(classificationIdentity);
      expect(read.record).toBeNull();
      expect(read.error).not.toBeNull();
      expect(localStorage.getItem(preparationKey(classificationIdentity.reviewId))).not.toBeNull();
    }
  });

  it("keeps storage failures observable and validates before writing", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementationOnce(() => {
      throw new Error("denied");
    });
    const denied = readRepairPreparation(classificationIdentity);
    expect(denied.record).toBeNull();
    expect(denied.error).toBeInstanceOf(Error);

    vi.spyOn(Storage.prototype, "setItem").mockImplementationOnce(() => {
      throw new Error("denied");
    });
    expect(() => writeRepairPreparation(recordFor(classificationIdentity))).toThrow("denied");

    // Write validation throws before any byte reaches storage, so the root
    // caller sends nothing for a malformed operation.
    const setter = vi.spyOn(Storage.prototype, "setItem").mockClear();
    const invalid = recordFor(classificationIdentity);
    invalid.operation = { ...invalid.operation, operation_id: "NOT-A-UUID" };
    expect(() => writeRepairPreparation(invalid)).toThrow();
    expect(setter).not.toHaveBeenCalled();
  });

  it("clears only the exact review key", () => {
    const other = identityFor(CHECK_CLASSIFICATION, "review b/c", "memory-9");
    writeRepairPreparation(recordFor(classificationIdentity));
    writeRepairPreparation(recordFor(other));
    clearRepairPreparation(classificationIdentity.reviewId);
    expect(localStorage.getItem(preparationKey(classificationIdentity.reviewId))).toBeNull();
    expect(readRepairPreparation(other).record).toEqual(recordFor(other));
    expect(localStorage.getItem(preparationKey("review b/c"))).not.toBeNull();
  });
});

describe("preparation status validation", () => {
  it("accepts live phases only for the exact operation id", () => {
    const operation = operationFor(classificationIdentity);
    for (const phase of ["not_started", "in_progress", "interrupted"]) {
      expect(validateRepairPreparationStatus(
        { operation_id: OPERATION_ID, state: { phase } }, operation, classificationIdentity,
      )).toBe(true);
    }
    for (const invalid of [
      null,
      { operation_id: OTHER_OPERATION_ID, state: { phase: "in_progress" } },
      { operation_id: OPERATION_ID, state: { phase: "success" } },
      { operation_id: OPERATION_ID },
      { operation_id: OPERATION_ID, state: null },
    ]) {
      expect(validateRepairPreparationStatus(invalid, operation, classificationIdentity)).toBe(false);
    }
  });

  it("accepts well-formed cancellation and rejects malformed terminal states", () => {
    const operation = operationFor(classificationIdentity);
    expect(validateRepairPreparationStatus(
      { operation_id: OPERATION_ID, state: { phase: "cancelled", cancelled_at: 123 } },
      operation, classificationIdentity,
    )).toBe(true);
    for (const cancelledAt of [0, -1, 1.5, Number.NaN, "123", undefined]) {
      expect(validateRepairPreparationStatus(
        { operation_id: OPERATION_ID, state: { phase: "cancelled", cancelled_at: cancelledAt } },
        operation, classificationIdentity,
      )).toBe(false);
    }
    expect(validateRepairPreparationStatus(
      { operation_id: OTHER_OPERATION_ID, state: { phase: "cancelled", cancelled_at: 123 } },
      operation, classificationIdentity,
    )).toBe(false);
  });

  it("accepts Ready only when the manifest and nested status bind to this review", async () => {
    const operation = operationFor(classificationIdentity);
    const manifest = await manifestFor(classificationIdentity);
    const nested = { manifest_id: manifest.manifest_id, manifest_digest: manifest.manifest_digest, state: { phase: "prepared" } };
    expect(validateRepairPreparationStatus(
      { operation_id: OPERATION_ID, state: { phase: "ready", manifest, operation: nested } },
      operation, classificationIdentity,
    )).toBe(true);

    const crossManifest = await manifestFor({ ...classificationIdentity, reviewId: "other-review" });
    const crossNested = { manifest_id: crossManifest.manifest_id, manifest_digest: crossManifest.manifest_digest, state: { phase: "prepared" } };
    // Manifest bound to another review.
    expect(validateRepairPreparationStatus(
      { operation_id: OPERATION_ID, state: { phase: "ready", manifest: crossManifest, operation: crossNested } },
      operation, classificationIdentity,
    )).toBe(false);
    // Nested status bound to another manifest.
    expect(validateRepairPreparationStatus(
      { operation_id: OPERATION_ID, state: { phase: "ready", manifest, operation: crossNested } },
      operation, classificationIdentity,
    )).toBe(false);
    // Nested terminal state without its receipts.
    expect(validateRepairPreparationStatus(
      {
        operation_id: OPERATION_ID,
        state: { phase: "ready", manifest, operation: { manifest_id: manifest.manifest_id, manifest_digest: manifest.manifest_digest, state: { phase: "verified" } } },
      },
      operation, classificationIdentity,
    )).toBe(false);
    // Cross-operation Ready receipt.
    expect(validateRepairPreparationStatus(
      { operation_id: OTHER_OPERATION_ID, state: { phase: "ready", manifest, operation: nested } },
      operation, classificationIdentity,
    )).toBe(false);
  });

  it("accepts a Ready whose nested status carries matching receipts", async () => {
    const operation = operationFor(classificationIdentity);
    const manifest = await manifestFor(classificationIdentity);
    const receipt = applyReceiptFor(manifest);
    const nested = {
      manifest_id: manifest.manifest_id,
      manifest_digest: manifest.manifest_digest,
      state: { phase: "applied_unverified", apply_receipt: receipt },
    };
    expect(validateRepairPreparationStatus(
      { operation_id: OPERATION_ID, state: { phase: "ready", manifest, operation: nested } },
      operation, classificationIdentity,
    )).toBe(true);
    const tampered = {
      manifest_id: manifest.manifest_id,
      manifest_digest: manifest.manifest_digest,
      state: { phase: "applied_unverified", apply_receipt: { ...receipt, manifest_id: "other" } },
    };
    expect(validateRepairPreparationStatus(
      { operation_id: OPERATION_ID, state: { phase: "ready", manifest, operation: tampered } },
      operation, classificationIdentity,
    )).toBe(false);
  });
});
