// SPDX-License-Identifier: AGPL-3.0-only
import type {
  ApplyRepairRequest,
  RepairApplyReceipt,
  RepairLintReport,
  RepairManifest,
  RepairOperationStatus,
  RepairSemanticFinding,
  RepairVerificationReceipt,
} from "./repairTypes";
import { repairValidateManifest } from "./tauri";

export const REPAIR_PROGRESS_VERSION = 1;
export const REPAIR_PROGRESS_PREFIX = "wenlan.repair.progress.v1:";

export type RepairProgressPhase =
  | "prepared"
  | "applying"
  | "applied_unverified"
  | "verified";

export interface RepairReviewIdentity {
  reviewId: string;
  checkId: string;
  occurrenceDigest: string;
  ownerIds: string[];
}

export interface RepairProgressRecord extends RepairReviewIdentity {
  version: typeof REPAIR_PROGRESS_VERSION;
  phase: RepairProgressPhase;
  manifest: RepairManifest;
  applyReceipt?: RepairApplyReceipt;
  verificationReceipt?: RepairVerificationReceipt;
}

export interface RepairProgressRead {
  record: RepairProgressRecord | null;
  error: unknown | null;
}

export function repairProgressKey(reviewId: string): string {
  return `${REPAIR_PROGRESS_PREFIX}${encodeURIComponent(reviewId)}`;
}

function browserStorage(): Storage {
  if (typeof window === "undefined" || !window.localStorage) {
    throw new Error("local repair progress storage is unavailable");
  }
  return window.localStorage;
}

function sameIds(left: unknown, right: readonly string[]): boolean {
  return Array.isArray(left) && left.length === right.length &&
    left.every((value, index) => value === right[index]);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function matchesIdentity(record: unknown, identity: RepairReviewIdentity): record is RepairProgressRecord {
  if (!isRecord(record)) return false;
  return record.version === REPAIR_PROGRESS_VERSION &&
    record.reviewId === identity.reviewId &&
    record.checkId === identity.checkId &&
    record.occurrenceDigest === identity.occurrenceDigest &&
    sameIds(record.ownerIds, identity.ownerIds) &&
    ["prepared", "applying", "applied_unverified", "verified"].includes(String(record.phase)) &&
    isRecord(record.manifest);
}

/** A storage exception is observable to the caller: an apply must be blocked. */
export function readRepairProgress(identity: RepairReviewIdentity): RepairProgressRead {
  try {
    const raw = browserStorage().getItem(repairProgressKey(identity.reviewId));
    if (!raw) return { record: null, error: null };
    const parsed: unknown = JSON.parse(raw);
    if (!matchesIdentity(parsed, identity)) {
      return { record: null, error: new Error("saved repair progress belongs to another proposal") };
    }
    const binding = validateRepairManifestBinding(parsed.manifest, identity);
    return binding.ok
      ? { record: parsed, error: null }
      : { record: null, error: new Error("saved repair manifest is not bound to this proposal") };
  } catch (error) {
    return { record: null, error };
  }
}

export function writeRepairProgress(record: RepairProgressRecord): void {
  browserStorage().setItem(repairProgressKey(record.reviewId), JSON.stringify(record));
}

export function clearRepairProgress(reviewId: string): void {
  browserStorage().removeItem(repairProgressKey(reviewId));
}

/**
 * The daemon's semantic record digest is the little-endian u64 represented by
 * the first eight bytes of SHA-256(kind + ":" + durable id), not the full
 * 64-character digest. This mirrors `semantic_record_digest` in the Rust
 * semantic lint producer.
 */
export async function sha256Hex(value: string): Promise<string> {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) throw new Error("SHA-256 is unavailable in this browser");
  const bytes = new TextEncoder().encode(value);
  const digest = await subtle.digest("SHA-256", bytes);
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

export async function semanticRecordDigest(memoryId: string): Promise<string> {
  const full = await sha256Hex(`memory:${memoryId}`);
  const firstEight = full.match(/.{2}/g)?.slice(0, 8) ?? [];
  return firstEight.reverse().join("");
}

/**
 * The daemon signs the manifest by hashing the exact JSON object with
 * `manifest_digest` omitted. Keep the returned property order intact: this is
 * an integrity check for persisted recovery data, not a new serialization
 * format for requests.
 */
export async function validateRepairManifestDigest(manifest: RepairManifest): Promise<boolean> {
  // Rust owns canonical JSON serialization (including numeric formatting and
  // omitted optional fields). The bridge is read-only and returns only the
  // typed digest validation result.
  return repairValidateManifest(manifest);
}

export async function findClassificationFinding(
  report: RepairLintReport,
  memoryId: string,
): Promise<RepairSemanticFinding | null> {
  if (report.profile !== "deep") return null;
  const expectedDigest = await semanticRecordDigest(memoryId);
  const check = report.checks.find(
    (candidate) => candidate.check_id === "memories.semantic.classification",
  );
  if (!check || check.outcome !== "finding") return null;
  for (const evidence of check.evidence) {
    if (evidence.kind !== "semantic_finding") continue;
    const finding = evidence.finding;
    if (
      finding.proposed_action === "reclassify_memory" &&
      !finding.unresolved_disagreement &&
      finding.evidence_ids.includes(expectedDigest)
    ) {
      return finding;
    }
  }
  return null;
}

function isDigest(value: unknown): value is string {
  return typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
}

export type RepairBindingValidation =
  | { ok: true }
  | { ok: false; reason: "missing" | "mismatched" };

/** Require the producer to bind the prepared proposal to this exact queue item. */
export function validateRepairManifestBinding(
  manifest: RepairManifest,
  identity: RepairReviewIdentity,
): RepairBindingValidation {
  const binding = manifest.source?.review_binding;
  const bound = binding?.review_id === identity.reviewId &&
    binding.occurrence_digest === identity.occurrenceDigest &&
    sameIds(binding.owner_ids, identity.ownerIds);
  const checkBound = manifest.source?.check_id === identity.checkId &&
    manifest.post_assertions?.target_check_id === identity.checkId;
  if (!binding || !manifest.source || !manifest.post_assertions) {
    return { ok: false, reason: "missing" };
  }
  if (!bound || !checkBound || !isDigest(manifest.manifest_digest)) {
    return { ok: false, reason: "mismatched" };
  }
  return { ok: true };
}

export function applyRequestForManifest(manifest: RepairManifest): ApplyRepairRequest {
  return {
    manifest_id: manifest.manifest_id,
    approved_manifest_digest: manifest.manifest_digest,
    approval: `apply repair ${manifest.manifest_id} ${manifest.manifest_digest}`,
  };
}

export type ApplyFailureKind = "pre_apply" | "unknown_apply";

/**
 * Once the apply endpoint has been invoked, its error may have occurred after
 * the canonical commit and before the receipt became observable. Error text
 * cannot prove that no write happened, including a stale-looking conflict.
 */
export function classifyApplyFailure(error: unknown): ApplyFailureKind {
  void error;
  return "unknown_apply";
}

export function validateRepairApplyReceipt(
  receipt: RepairApplyReceipt,
  manifest: RepairManifest,
): boolean {
  return receipt.manifest_id === manifest.manifest_id &&
    receipt.manifest_digest === manifest.manifest_digest &&
    isDigest(receipt.receipt_digest);
}

export function validateRepairVerificationReceipt(
  receipt: RepairVerificationReceipt,
  manifest: RepairManifest,
  applyReceipt: RepairApplyReceipt,
): boolean {
  return receipt.manifest_id === manifest.manifest_id &&
    receipt.manifest_digest === manifest.manifest_digest &&
    receipt.apply_receipt_digest === applyReceipt.receipt_digest &&
    isDigest(receipt.receipt_digest);
}


/** Bind every status, including cancellation, to the exact approved manifest. */
export function validateRepairOperationStatus(
  value: unknown,
  manifest: RepairManifest,
): value is RepairOperationStatus {
  if (!isRecord(value) || value.manifest_id !== manifest.manifest_id ||
    value.manifest_digest !== manifest.manifest_digest || !isRecord(value.state)) return false;
  const state = value.state;
  switch (state.phase) {
    case "prepared":
    case "in_progress":
    case "indeterminate": return true;
    case "cancelled": return typeof state.cancelled_at === "number" && Number.isSafeInteger(state.cancelled_at) && state.cancelled_at > 0;
    case "applied_unverified":
    case "verified": {
      if (!isRecord(state.apply_receipt) ||
        !validateRepairApplyReceipt(state.apply_receipt as unknown as RepairApplyReceipt, manifest)) return false;
      return state.phase === "applied_unverified" || (isRecord(state.verification_receipt) &&
        validateRepairVerificationReceipt(state.verification_receipt as unknown as RepairVerificationReceipt,
          manifest, state.apply_receipt as unknown as RepairApplyReceipt));
    }
    default: return false;
  }
}
