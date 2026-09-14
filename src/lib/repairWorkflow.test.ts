// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  applyRequestForManifest,
  classifyApplyFailure,
  readRepairProgress,
  semanticRecordDigest,
  sha256Hex,
  validateRepairApplyReceipt,
  validateRepairManifestBinding,
  validateRepairManifestDigest,
  validateRepairVerificationReceipt,
  writeRepairProgress,
  type RepairProgressRecord,
  type RepairReviewIdentity,
} from "./repairWorkflow";
import type { RepairApplyReceipt, RepairManifest, RepairVerificationReceipt } from "./repairTypes";

vi.mock("./tauri", async (original) => ({
  ...(await original<typeof import("./tauri")>()),
  repairValidateManifest: vi.fn(),
}));
import * as tauri from "./tauri";

const identity: RepairReviewIdentity = {
  reviewId: "review-1",
  checkId: "pages.duplicate_active_titles",
  occurrenceDigest: "a".repeat(64),
  ownerIds: ["page-1"],
};

async function manifestFor(nextIdentity = identity): Promise<RepairManifest> {
  const unsigned = {
    manifest_schema_version: 1,
    manifest_id: "manifest-1",
    prepared_at: 123,
    source: {
      report_schema_version: 1,
      check_catalog_version: 1,
      lint_scope: { kind: "uncategorized" as const },
      report_scope: { kind: "uncategorized" as const },
      check_id: nextIdentity.checkId,
      finding: null,
      deterministic_evidence: [],
      general_snapshots: {},
      deep_snapshots: null,
      general_producer_receipt: { runtime_commit: null },
      deep_producer_receipt: null,
      agent_work_digest: null,
      review_binding: {
        review_id: nextIdentity.reviewId,
        occurrence_digest: nextIdentity.occurrenceDigest,
        owner_ids: [...nextIdentity.ownerIds],
      },
    },
    target: { kind: "page" as const, page_id: "page-1", scope: { kind: "uncategorized" as const } },
    expected_state: { version: null, canonical_receipt: "b".repeat(64) },
    writer: "rename_page_title" as const,
    mutation: {
      kind: "rename_page_title" as const,
      before_title: "Before",
      after_title: "After",
      after_embedding_hex: "00",
    },
    allowed_effects: {
      owner: { kind: "page" as const, page_id: "page-1", scope: { kind: "uncategorized" as const } },
      fields: ["page_title" as const],
    },
    rollback: { format_version: 1, relative_path: "rollback.json", digest: "c".repeat(64) },
    post_assertions: {
      target_check_id: nextIdentity.checkId,
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
  const manifestDigest = await sha256Hex(JSON.stringify(unsigned));
  return { ...unsigned, manifest_digest: manifestDigest } as RepairManifest;
}

function applyReceipt(manifest: RepairManifest): RepairApplyReceipt {
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

describe("source repair workflow guards", () => {
  beforeEach(() => {
    localStorage.clear();
    (tauri.repairValidateManifest as unknown as { mockResolvedValue: (value: boolean) => void }).mockResolvedValue(true);
  });

  it("matches the daemon's little-endian semantic record digest", async () => {
    await expect(semanticRecordDigest("mem-1")).resolves.toBe("abfebda9ecdb7fc0");
  });

  it("requires the exact review binding, occurrence, owner order, and check", async () => {
    const manifest = await manifestFor();
    expect(validateRepairManifestBinding(manifest, identity)).toEqual({ ok: true });
    expect(validateRepairManifestBinding(manifest, { ...identity, ownerIds: ["other"] })).toEqual({ ok: false, reason: "mismatched" });
    expect(validateRepairManifestBinding(manifest, { ...identity, occurrenceDigest: "9".repeat(64) })).toEqual({ ok: false, reason: "mismatched" });
    expect(validateRepairManifestBinding(manifest, { ...identity, checkId: "other.check" })).toEqual({ ok: false, reason: "mismatched" });
  });

  it("accepts only a manifest whose signed JSON is unchanged", async () => {
    const manifest = await manifestFor();
    await expect(validateRepairManifestDigest(manifest)).resolves.toBe(true);
    expect(tauri.repairValidateManifest).toHaveBeenCalledWith(manifest);
    (tauri.repairValidateManifest as unknown as { mockResolvedValue: (value: boolean) => void }).mockResolvedValue(false);
    await expect(validateRepairManifestDigest({ ...manifest, mutation: { ...manifest.mutation, after_title: "Changed again" } as RepairManifest["mutation"] })).resolves.toBe(false);
  });

  it("treats a stale-looking server conflict after repairApply as unknown apply", () => {
    expect(classifyApplyFailure(new Error("409 stale manifest"))).toBe("unknown_apply");
    expect(classifyApplyFailure(new Error("invalid expected state"))).toBe("unknown_apply");
  });

  it("requires matching apply and verification receipts", async () => {
    const manifest = await manifestFor();
    const receipt = applyReceipt(manifest);
    expect(validateRepairApplyReceipt(receipt, manifest)).toBe(true);
    expect(validateRepairApplyReceipt({ ...receipt, manifest_id: "other" }, manifest)).toBe(false);
    const verification: RepairVerificationReceipt = {
      receipt_schema_version: 1,
      manifest_id: manifest.manifest_id,
      manifest_digest: manifest.manifest_digest,
      apply_receipt_digest: receipt.receipt_digest,
      verified_at: 789,
      general_snapshots: {},
      deep_snapshots: null,
      receipt_digest: "4".repeat(64),
    };
    expect(validateRepairVerificationReceipt(verification, manifest, receipt)).toBe(true);
    expect(validateRepairVerificationReceipt({ ...verification, apply_receipt_digest: "5".repeat(64) }, manifest, receipt)).toBe(false);
  });

  it("refuses persisted progress whose manifest is bound to another review", async () => {
    const wrong = await manifestFor({ ...identity, reviewId: "other-review" });
    const record: RepairProgressRecord = {
      version: 1,
      reviewId: identity.reviewId,
      checkId: identity.checkId,
      occurrenceDigest: identity.occurrenceDigest,
      ownerIds: identity.ownerIds,
      phase: "applying",
      manifest: wrong,
    };
    writeRepairProgress(record);
    const read = readRepairProgress(identity);
    expect(read.record).toBeNull();
    expect(read.error).toBeInstanceOf(Error);
  });

  it("formats the exact approval sentence from the manifest digest", async () => {
    const manifest = await manifestFor();
    expect(applyRequestForManifest(manifest)).toEqual({
      manifest_id: manifest.manifest_id,
      approved_manifest_digest: manifest.manifest_digest,
      approval: `apply repair ${manifest.manifest_id} ${manifest.manifest_digest}`,
    });
  });
});
