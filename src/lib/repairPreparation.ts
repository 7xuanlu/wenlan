// SPDX-License-Identifier: AGPL-3.0-only
/**
 * Durable client-side record of the exact `repair_prepare_operation` request
 * the UI is about to send. The caller writes the record BEFORE invoking the
 * daemon, so a later status poll can prove it is observing its own operation
 * and not another review's. The caller owns selecting and generating the
 * operation UUID; this module only validates its shape.
 *
 * No network calls and no apply/verify invocation happen here. Ready-state
 * digest integrity stays with the root caller through
 * `validateRepairManifestDigest` over native code.
 */
import type {
  CurrentRepairChoice,
  RepairLintScope,
  RepairManifest,
  RepairPrepareOperationRequest,
  RepairPrepareOperationStatus,
} from "./repairTypes";
import {
  validateRepairManifestBinding,
  validateRepairOperationStatus,
  type RepairReviewIdentity,
} from "./repairWorkflow";

const REPAIR_PREPARATION_PREFIX = "wenlan.repair.prepare.v1:";
const PREPARATION_VERSION = 1;

const CHECK_CLASSIFICATION = "memories.semantic.classification";
const CHECK_DUPLICATE_TITLES = "pages.duplicate_active_titles";
const CHECK_ENRICHMENT = "memories.enrichment_failures";

const MEMORY_TYPES = ["identity", "preference", "decision", "lesson", "gotcha", "fact"];

/** Lowercase hyphenated UUID, mirroring `RepairPrepareOperationRequest.validate` in Rust. */
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

export interface RepairPreparationRecord extends RepairReviewIdentity {
  version: 1;
  operation: RepairPrepareOperationRequest;
}

function preparationKey(reviewId: string): string {
  return `${REPAIR_PREPARATION_PREFIX}${encodeURIComponent(reviewId)}`;
}

function browserStorage(): Storage {
  if (typeof window === "undefined" || !window.localStorage) {
    throw new Error("local repair preparation storage is unavailable");
  }
  return window.localStorage;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function sameIds(left: unknown, right: readonly string[]): boolean {
  return Array.isArray(left) && left.length === right.length &&
    left.every((value, index) => value === right[index]);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.length > 0;
}

function isLintScope(value: unknown): value is RepairLintScope {
  if (!isRecord(value) || typeof value.kind !== "string") return false;
  switch (value.kind) {
    case "global":
    case "uncategorized":
      return true;
    case "registered":
      return typeof value.space === "string" && value.space.trim().length > 0;
    default:
      return false;
  }
}

/**
 * The only writers the current review UI can produce. Each choice carries the
 * single owner id and may only appear under its own check; nothing here
 * invents a new repair family.
 */
function isChoiceForIdentity(choice: unknown, identity: RepairReviewIdentity): choice is CurrentRepairChoice {
  if (!isRecord(choice) || typeof choice.kind !== "string") return false;
  if (choice.review_id !== identity.reviewId) return false;
  // The preparation always describes one review queue item with one target.
  if (!Array.isArray(identity.ownerIds) || identity.ownerIds.length !== 1) return false;
  const owner = identity.ownerIds[0];
  if (typeof owner !== "string" || owner.length === 0) return false;
  switch (choice.kind) {
    case "reclassify_memory":
      return identity.checkId === CHECK_CLASSIFICATION &&
        choice.memory_id === owner &&
        typeof choice.after_memory_type === "string" &&
        MEMORY_TYPES.includes(choice.after_memory_type);
    case "rename_page_title":
      return identity.checkId === CHECK_DUPLICATE_TITLES &&
        choice.page_id === owner &&
        typeof choice.before_title === "string" && choice.before_title.trim().length > 0 &&
        typeof choice.after_title === "string" && choice.after_title.trim().length > 0;
    case "complete_entity_extraction":
      return identity.checkId === CHECK_ENRICHMENT &&
        choice.memory_id === owner &&
        Array.isArray(choice.entity_ids) &&
        choice.entity_ids.every((id) => typeof id === "string" && id.length > 0);
    default:
      return false;
  }
}

function isOperationForIdentity(operation: unknown, identity: RepairReviewIdentity): operation is RepairPrepareOperationRequest {
  if (!isRecord(operation)) return false;
  if (typeof operation.operation_id !== "string" || !UUID_PATTERN.test(operation.operation_id)) return false;
  if (!isRecord(operation.request)) return false;
  return isLintScope(operation.request.lint_scope) && isChoiceForIdentity(operation.request.choice, identity);
}

function isRecordForIdentity(value: unknown, identity: RepairReviewIdentity): value is RepairPreparationRecord {
  if (!isRecord(value)) return false;
  return value.version === PREPARATION_VERSION &&
    value.reviewId === identity.reviewId &&
    value.checkId === identity.checkId &&
    value.occurrenceDigest === identity.occurrenceDigest &&
    sameIds(value.ownerIds, identity.ownerIds) &&
    isNonEmptyString(value.reviewId) &&
    isNonEmptyString(value.checkId) &&
    isNonEmptyString(value.occurrenceDigest) &&
    isOperationForIdentity(value.operation, identity);
}

/**
 * A missing record is null with no error. A corrupt, stale, or differently
 * bound record is null WITH an error; it is never erased silently.
 * Storage and parse failures stay observable through `error`.
 */
export function readRepairPreparation(
  identity: RepairReviewIdentity,
): { record: RepairPreparationRecord | null; error: unknown | null } {
  try {
    const raw = browserStorage().getItem(preparationKey(identity.reviewId));
    if (!raw) return { record: null, error: null };
    const parsed: unknown = JSON.parse(raw);
    if (!isRecordForIdentity(parsed, identity)) {
      return { record: null, error: new Error("saved repair preparation belongs to another proposal") };
    }
    return { record: parsed, error: null };
  } catch (error) {
    return { record: null, error };
  }
}

/**
 * Validates the full record BEFORE touching storage and throws validation or
 * storage errors, so the root caller sends nothing when this throws.
 */
export function writeRepairPreparation(record: RepairPreparationRecord): void {
  if (!isRecordForIdentity(record, record)) {
    throw new Error("repair preparation is not bound to its review proposal");
  }
  browserStorage().setItem(preparationKey(record.reviewId), JSON.stringify(record));
}

/** Removes only the exact review key; storage failures propagate. */
export function clearRepairPreparation(reviewId: string): void {
  browserStorage().removeItem(preparationKey(reviewId));
}

/**
 * Structural and binding validation only: the exact operation id, legal live
 * phases, a well-formed cancellation timestamp, or a Ready whose manifest is
 * bound to this review and whose nested operation status is bound to that
 * same manifest. Cross-manifest, cross-review, malformed, and unknown states
 * are rejected. The root caller still runs `validateRepairManifestDigest`
 * through native code before consuming Ready.
 */
export function validateRepairPreparationStatus(
  value: unknown,
  operation: RepairPrepareOperationRequest,
  identity: RepairReviewIdentity,
): value is RepairPrepareOperationStatus {
  if (!isRecord(value) || !isOperationForIdentity(operation, identity)) return false;
  if (typeof operation.operation_id !== "string") return false;
  if (typeof value.operation_id !== "string" || value.operation_id !== operation.operation_id) return false;
  if (!isRecord(value.state) || typeof value.state.phase !== "string") return false;
  switch (value.state.phase) {
    case "not_started":
    case "in_progress":
    case "interrupted":
      return true;
    case "cancelled": {
      const at = value.state.cancelled_at;
      return typeof at === "number" && Number.isSafeInteger(at) && at > 0;
    }
    case "ready": {
      if (!isRecord(value.state.manifest)) return false;
      const manifest = value.state.manifest as RepairManifest;
      if (!validateRepairManifestBinding(manifest, identity).ok) return false;
      return validateRepairOperationStatus(value.state.operation, manifest);
    }
    default:
      return false;
  }
}
