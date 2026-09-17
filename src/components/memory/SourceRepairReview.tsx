// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import {
  daemonErrorMessage,
  getMemoryDetail,
  getPage,
  listEntities,
  repairApply,
  repairLint,
  repairPrepareCurrent,
  repairRecovery,
  repairVerify,
  type Entity,
  type MemoryItem,
  type MemoryType,
  type Page,
  type RefinementPayload,
} from "../../lib/tauri";
import type { ReviewItem } from "./useReviewQueue";
import "./SourceRepairReview.css";
import {
  applyRequestForManifest,
  classifyApplyFailure,
  readRepairProgress,
  validateRepairApplyReceipt,
  validateRepairManifestBinding,
  validateRepairManifestDigest,
  validateRepairVerificationReceipt,
  writeRepairProgress,
  type RepairProgressPhase,
  type RepairProgressRecord,
  type RepairReviewIdentity,
} from "../../lib/repairWorkflow";
import type {
  CurrentRepairChoice,
  RepairLintReport,
  RepairLintScope,
  RepairManifest,
  RepairApplyReceipt,
  RepairVerificationReceipt,
} from "../../lib/repairTypes";

type RepairReviewItem = Extract<ReviewItem, { kind: "refinement" }>;
type RepairPayload = Extract<RefinementPayload, { action: "lint_repair_review" }>;

const MEMORY_TYPES: MemoryType[] = [
  "identity",
  "preference",
  "decision",
  "lesson",
  "gotcha",
  "fact",
];

const CHECK_CLASSIFICATION = "memories.semantic.classification";
const CHECK_DUPLICATE_TITLES = "pages.duplicate_active_titles";
const CHECK_ENRICHMENT = "memories.enrichment_failures";
const CHECK_PROVENANCE = "pages.semantic.provenance_adequacy";
const CHECK_FAITHFULNESS = "pages.semantic.faithfulness";

const paneStyle: React.CSSProperties = {
  fontFamily: "var(--mem-font-body)",
  fontSize: 14,
  lineHeight: 1.65,
  color: "var(--mem-text)",
  overflowWrap: "anywhere",
  minWidth: 0,
};

const labelStyle: React.CSSProperties = {
  display: "grid",
  gap: 8,
  color: "var(--mem-text-secondary)",
  fontSize: "var(--mem-text-meta)",
};

const buttonStyle: React.CSSProperties = {
  fontFamily: "var(--mem-font-body)",
  fontSize: "var(--mem-text-control)",
  borderRadius: 8,
  padding: "8px 15px",
  cursor: "pointer",
  border: "1px solid var(--mem-border)",
  backgroundColor: "var(--mem-surface)",
  color: "var(--mem-text)",
  justifySelf: "end",
};

function repairPayload(item: RepairReviewItem): RepairPayload | null {
  return item.payload?.action === "lint_repair_review" ? item.payload : null;
}

function targetIdFor(item: RepairReviewItem): string | null {
  if (item.sourceIds.length !== 1) return null;
  const id = item.sourceIds[0];
  return typeof id === "string" && id.trim().length > 0 ? id : null;
}

function scopeFor(space: string | null | undefined): RepairLintScope {
  const value = space?.trim();
  return value ? { kind: "registered", space: value } : { kind: "uncategorized" };
}

function lintQueryForScope(profile: "general" | "deep", scope: RepairLintScope) {
  return {
    profile,
    ...(scope.kind === "registered"
      ? { space: scope.space }
      : scope.kind === "uncategorized"
        ? { space: "uncategorized" }
        : {}),
    external_egress: false,
    // Agent-assist reports contain pending work rather than the provider
    // packet required to bind a semantic repair.
    agent_assist: false,
  };
}

function targetSpace(target: MemoryItem | Page | null): string | null {
  const value = target?.space ?? target?.domain ?? null;
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : null;
}

function sameScope(entity: Entity, space: string | null): boolean {
  const entitySpace = entity.space ?? entity.domain ?? null;
  return (typeof entitySpace === "string" ? entitySpace.trim() : entitySpace) === space;
}

function isMemoryType(value: string | null | undefined): value is MemoryType {
  return !!value && MEMORY_TYPES.includes(value as MemoryType);
}

function diagnostic(value: unknown): string {
  if (value instanceof Error && value.message) return value.message;
  if (typeof value === "string" && value.trim()) return value;
  return "unknown error";
}

function parseRepairRecovery(value: unknown): {
  manifest: RepairManifest;
  applyReceipt: RepairApplyReceipt | null;
} | null {
  if (value === null) return null;
  if (typeof value !== "object" || value === null) {
    throw new Error("repair recovery returned a malformed response");
  }
  const candidate = value as { manifest?: unknown; apply_receipt?: unknown };
  if (typeof candidate.manifest !== "object" || candidate.manifest === null ||
      (candidate.apply_receipt !== null && typeof candidate.apply_receipt !== "object")) {
    throw new Error("repair recovery returned a malformed response");
  }
  return {
    manifest: candidate.manifest as RepairManifest,
    applyReceipt: candidate.apply_receipt as RepairApplyReceipt | null,
  };
}

function reportReady(report: RepairLintReport | undefined, profile: "general" | "deep"): report is RepairLintReport {
  return report?.profile === profile && report.complete === true;
}

function hasApplicableCheck(report: RepairLintReport | undefined, checkId: string): report is RepairLintReport {
  return report?.profile === "deep" && !!report.checks.some(
    (check) => check.check_id === checkId && (check.outcome === "pass" || check.outcome === "finding"),
  );
}

function needsDeepVerification(manifest: RepairManifest, checkId: string | null): boolean {
  const policy = manifest.post_assertions?.verification_policy;
  if (policy?.kind === "applicable_checks") {
    return policy.required_deep_check_ids.includes(CHECK_CLASSIFICATION);
  }
  return checkId === CHECK_CLASSIFICATION;
}

export interface SourceRepairReviewProps {
  item: RepairReviewItem;
  onBusyChange: (busy: boolean) => void;
  onVerified: () => void;
  technicalDetails?: React.ReactNode;
  actionHost?: HTMLElement | null;
}

type FlowState =
  | "inspecting"
  | "ready"
  | "preparing"
  | "prepared"
  | "applying"
  | "uncertain_apply"
  | "verifying"
  | "applied_unverified"
  | "verified"
  | "error";

type ErrorKind = "target" | "unsupported" | "stale" | "storage" | "recovery" | "check_unavailable" | "prepare" | "pre_apply" | "verify" | "unknown_apply";

export default function SourceRepairReview({
  item,
  onBusyChange,
  onVerified,
  technicalDetails,
  actionHost,
}: SourceRepairReviewProps) {
  const { t } = useTranslation();
  const payload = repairPayload(item);
  const targetId = targetIdFor(item);
  const checkId = payload?.check_id ?? null;
  const supported = item.action === "lint_repair_review" &&
    (checkId === CHECK_CLASSIFICATION || checkId === CHECK_DUPLICATE_TITLES || checkId === CHECK_ENRICHMENT);
  // These semantic page findings are deliberately read-only here. They may
  // show the page that needs review, but this UI has no claim writer yet.
  const readOnlyPageCheck = item.action === "lint_repair_review" &&
    (checkId === CHECK_PROVENANCE || checkId === CHECK_FAITHFULNESS) &&
    item.sourceIds.length === 1 && targetId !== null;
  const identity = useMemo<RepairReviewIdentity | null>(() =>
    payload && checkId
      ? {
          reviewId: item.id,
          checkId,
          occurrenceDigest: payload.occurrence_digest,
          ownerIds: [...item.sourceIds],
        }
      : null, [checkId, item.id, item.sourceIds, payload]);

  const [flow, setFlow] = useState<FlowState>(supported || readOnlyPageCheck ? "inspecting" : "error");
  const [errorKind, setErrorKind] = useState<ErrorKind | null>(supported || readOnlyPageCheck ? null : "unsupported");
  const [errorDetail, setErrorDetail] = useState<string | null>(null);
  const [target, setTarget] = useState<MemoryItem | Page | null>(null);
  const [entities, setEntities] = useState<Entity[]>([]);
  const [manifest, setManifest] = useState<RepairManifest | null>(null);
  const [applyReceipt, setApplyReceipt] = useState<RepairApplyReceipt | null>(null);
  const [verificationReceipt, setVerificationReceipt] = useState<RepairVerificationReceipt | null>(null);
  const [selectedType, setSelectedType] = useState<MemoryType | "">("");
  const [newTitle, setNewTitle] = useState("");
  const [selectedEntities, setSelectedEntities] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [recoveryAttempt, setRecoveryAttempt] = useState(0);
  const requestInFlightRef = useRef(false);
  const retainedBusyRef = useRef(false);
  const approvalPreflightRef = useRef(false);
  const mountedRef = useRef(true);
  const reportedVerifiedRef = useRef(false);

  const setBusyState = (next: boolean) => {
    if (!mountedRef.current) return;
    setBusy(next);
    onBusyChange(next);
  };

  const beginRequest = (): boolean => {
    if (requestInFlightRef.current) return false;
    requestInFlightRef.current = true;
    retainedBusyRef.current = false;
    setBusyState(true);
    return true;
  };

  const finishRequest = (retainBusy = false) => {
    requestInFlightRef.current = false;
    retainedBusyRef.current = retainBusy;
    if (!retainBusy) setBusyState(false);
    else if (mountedRef.current) {
      // The host remains navigation-locked while recovery is unresolved, but
      // the local recovery control itself must be usable after the request
      // settles.
      setBusy(false);
      onBusyChange(true);
    }
  };

  const setError = (kind: ErrorKind, detail: unknown, retainBusy = false) => {
    if (!mountedRef.current) return;
    setErrorKind(kind);
    setErrorDetail(diagnostic(detail));
    finishRequest(retainBusy);
  };

  const currentMemory = target && "source_id" in target ? target : null;
  const currentPage = target && "id" in target ? target : null;
  const currentType = currentMemory?.memory_type ?? null;
  const currentSpace = targetSpace(target);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      // The host dialog is being unmounted, so late replies must not publish
      // state. It also owns the final navigation decision.
    };
  }, []);

  useEffect(() => {
    if ((!supported && !readOnlyPageCheck) || !identity || !payload) {
      setFlow("error");
      setErrorKind("unsupported");
      setBusyState(false);
      return;
    }
    let cancelled = false;
    requestInFlightRef.current = true;
    retainedBusyRef.current = false;
    setBusyState(true);
    setFlow("inspecting");
    setErrorKind(null);
    setErrorDetail(null);
    setTarget(null);
    setEntities([]);
    setManifest(null);
    setApplyReceipt(null);
    setVerificationReceipt(null);
    setSelectedEntities(new Set());
    setSelectedType("");
    setNewTitle("");

    const saved = supported ? readRepairProgress(identity) : { record: null, error: null };
    // Capture this before any asynchronous target read. React state from the
    // render that started the request is stale inside the catch handler; the
    // immutable local read result is the authority for whether recovery must
    // stay blocked.
    const localProgressUnreadable = saved.error != null;
    if (saved.error) {
      setErrorKind("storage");
      setErrorDetail(diagnostic(saved.error));
    } else if (saved.record) {
      setManifest(saved.record.manifest);
      setApplyReceipt(saved.record.applyReceipt ?? null);
      setVerificationReceipt(saved.record.verificationReceipt ?? null);
      if (saved.record.phase !== "prepared") {
        const receipt = saved.record.applyReceipt;
        const hasValidReceipt = receipt != null && validateRepairApplyReceipt(receipt, saved.record.manifest);
        setFlow(hasValidReceipt ? "applied_unverified" : "uncertain_apply");
        if (!hasValidReceipt) setApplyReceipt(null);
        retainedBusyRef.current = true;
      } else {
        setFlow("prepared");
      }
    }

    let recoveryRecord = saved.record;
    const load = async () => {
      let recoveryDigestFailed = false;
      let recoveryReadFailed = false;
      try {
        if (saved.record) {
          recoveryDigestFailed = true;
          const validDigest = await validateRepairManifestDigest(saved.record.manifest);
          recoveryDigestFailed = false;
          if (!validDigest) {
            setManifest(null);
            setFlow("error");
            setErrorKind("stale");
            setErrorDetail("saved repair manifest digest is invalid");
            finishRequest(false);
            return;
          }
        }

        // A local prepared/recovery record is authoritative. Only a cold cache
        // asks the daemon for the exact pending artifact; a failed read must
        // never be mistaken for an empty recovery state.
        if (supported && !saved.error && !saved.record) {
          let recovered: ReturnType<typeof parseRepairRecovery>;
          try {
            recovered = parseRepairRecovery(await repairRecovery(identity.reviewId));
            if (cancelled || !mountedRef.current) return;
          } catch (error) {
            if (cancelled || !mountedRef.current) return;
            setFlow("error");
            setErrorKind("recovery");
            setErrorDetail(diagnostic(error));
            finishRequest(true);
            return;
          }
          if (recovered) {
            recoveryReadFailed = true;
            const binding = validateRepairManifestBinding(recovered.manifest, identity);
            if (!binding.ok) throw new Error("repair recovery manifest is not bound to this proposal");
            const validDigest = await validateRepairManifestDigest(recovered.manifest);
            if (cancelled || !mountedRef.current) return;
            if (!validDigest) throw new Error("repair recovery manifest digest is invalid");
            if (recovered.applyReceipt && !validateRepairApplyReceipt(recovered.applyReceipt, recovered.manifest)) {
              throw new Error("repair recovery apply receipt did not match the manifest");
            }
            const hydratedRecord: RepairProgressRecord = {
              ...identity,
              version: 1,
              phase: recovered.applyReceipt ? "applied_unverified" : "applying",
              manifest: recovered.manifest,
              ...(recovered.applyReceipt ? { applyReceipt: recovered.applyReceipt } : {}),
            };
            try {
              writeRepairProgress(hydratedRecord);
            } catch (error) {
              if (cancelled || !mountedRef.current) return;
              setManifest(null);
              setApplyReceipt(null);
              setFlow("error");
              setErrorKind("recovery");
              setErrorDetail(diagnostic(error));
              finishRequest(true);
              return;
            }
            recoveryRecord = hydratedRecord;
            recoveryReadFailed = false;
            setManifest(recovered.manifest);
            setApplyReceipt(recovered.applyReceipt);
            setFlow(recovered.applyReceipt ? "applied_unverified" : "uncertain_apply");
            retainedBusyRef.current = true;
          }
        }

        if (cancelled || !mountedRef.current) return;
        if (!targetId) throw new Error("the proposal has no single target");
        let loadedTarget: MemoryItem | Page | null;
        if (checkId === CHECK_DUPLICATE_TITLES || readOnlyPageCheck) {
          loadedTarget = await getPage(targetId);
          if (!loadedTarget || loadedTarget.id !== targetId) throw new Error("page target is no longer available");
        } else {
          loadedTarget = await getMemoryDetail(targetId);
          if (!loadedTarget || loadedTarget.source_id !== targetId) throw new Error("memory target is no longer available");
        }
        if (cancelled || !mountedRef.current) return;
        setTarget(loadedTarget);
        // Recovery already carries the exact entity ids in its manifest. Do
        // not replace that authoritative artifact with a fresh editable list.
        if (checkId === CHECK_ENRICHMENT && (!recoveryRecord || recoveryRecord.phase === "prepared")) {
          const loadedEntities = await listEntities(undefined, targetSpace(loadedTarget) ?? "uncategorized");
          if (cancelled || !mountedRef.current) return;
          setEntities(loadedEntities.filter((entity) => sameScope(entity, targetSpace(loadedTarget))));
        }
        if (cancelled || !mountedRef.current) return;
        if (!supported) {
          setFlow("ready");
          finishRequest(false);
          return;
        }
        if (localProgressUnreadable) {
          // An unreadable progress record may describe an already-started
          // repair. Never overwrite it with a newly prepared change.
          setFlow("error");
          setErrorKind("storage");
          finishRequest(true);
          return;
        }
        // A saved uncertain/applied record keeps the host busy; no mutation or
        // verification is started on remount. Recovery is always explicit.
        const recovering = recoveryRecord &&
          (recoveryRecord.phase === "applying" || recoveryRecord.phase === "applied_unverified" || recoveryRecord.phase === "verified");
        if (!recovering) {
          setFlow(saved.record ? "prepared" : "ready");
          retainedBusyRef.current = false;
          finishRequest(false);
        } else {
          finishRequest(true);
        }
      } catch (error) {
        if (cancelled || !mountedRef.current) return;
        if (localProgressUnreadable) {
          setFlow("error");
          setErrorKind("storage");
          setErrorDetail(diagnostic(error));
          finishRequest(true);
          return;
        }
        if (recoveryReadFailed) {
          setFlow("error");
          setErrorKind("recovery");
          setErrorDetail(diagnostic(error));
          finishRequest(true);
          return;
        }
        const recovering = !recoveryDigestFailed && recoveryRecord != null &&
          (recoveryRecord.phase === "applying" || recoveryRecord.phase === "applied_unverified" || recoveryRecord.phase === "verified");
        if (recovering && recoveryRecord) {
          // Keep the exact saved manifest recoverable even if its target detail
          // disappeared during a remount. Never unlock or reinterpret an apply
          // that may already have committed.
          const receipt = recoveryRecord.applyReceipt;
          setFlow(receipt && validateRepairApplyReceipt(receipt, recoveryRecord.manifest)
            ? "applied_unverified" : "uncertain_apply");
          setErrorKind("target");
          setErrorDetail(diagnostic(error));
          finishRequest(true);
          return;
        }
        setFlow("error");
        setErrorKind(recoveryDigestFailed ? "storage" : "target");
        setErrorDetail(diagnostic(error));
        finishRequest(false);
      }
    };
    void load();
    return () => {
      cancelled = true;
      requestInFlightRef.current = false;
    };
  // The keyed host component remounts for a new proposal; these are the
  // identity fields needed to prevent a late reply crossing that boundary.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [checkId, identity?.occurrenceDigest, identity?.reviewId, readOnlyPageCheck, recoveryAttempt, supported, targetId]);

  const writeProgress = (record: RepairProgressRecord): boolean => {
    try {
      writeRepairProgress(record);
      return true;
    } catch (error) {
      if (mountedRef.current) {
        setErrorKind("storage");
        setErrorDetail(diagnostic(error));
      }
      return false;
    }
  };

  const baseRecord = (nextManifest: RepairManifest, phase: RepairProgressPhase): RepairProgressRecord => ({
    version: 1,
    reviewId: identity?.reviewId ?? item.id,
    checkId: identity?.checkId ?? checkId ?? "",
    occurrenceDigest: identity?.occurrenceDigest ?? payload?.occurrence_digest ?? "",
    ownerIds: [...(identity?.ownerIds ?? item.sourceIds)],
    phase,
    manifest: nextManifest,
  });

  const setWorkflowError = (kind: ErrorKind, detail: unknown) => {
    setFlow("error");
    setError(kind, detail, false);
  };

  const prepare = async () => {
    // Check all local prerequisites before taking the host navigation lock.
    if (!identity || !payload || !supported || !target || !beginRequest()) return;
    setFlow("preparing");
    setErrorKind(null);
    setErrorDetail(null);
    try {
      const lintScope = scopeFor(targetSpace(target));
      let choice: CurrentRepairChoice;
      if (checkId === CHECK_CLASSIFICATION) {
        if (!currentMemory || !selectedType || selectedType === currentType) {
          throw new Error("choose a different memory type");
        }
        choice = { kind: "reclassify_memory", review_id: item.id, memory_id: currentMemory.source_id, after_memory_type: selectedType };
      } else if (checkId === CHECK_DUPLICATE_TITLES) {
        if (!currentPage || !newTitle.trim() || newTitle.trim() === currentPage.title.trim()) {
          throw new Error("enter a new page title");
        }
        choice = {
          kind: "rename_page_title",
          review_id: item.id,
          page_id: currentPage.id,
          before_title: currentPage.title,
          after_title: newTitle.trim(),
        };
      } else if (checkId === CHECK_ENRICHMENT) {
        if (!currentMemory) throw new Error("memory target is unavailable");
        const entityIds = [...selectedEntities]
          .filter((id) => entities.some((entity) => entity.id === id))
          .sort();
        choice = {
          kind: "complete_entity_extraction",
          review_id: item.id,
          memory_id: currentMemory.source_id,
          // The daemon's contract permits an empty, explicit selection.
          entity_ids: entityIds,
        };
      } else {
        throw new Error("unsupported source repair check");
      }
      const prepared = await repairPrepareCurrent({
        lint_scope: lintScope,
        choice,
      });
      const binding = validateRepairManifestBinding(prepared, identity);
      if (!binding.ok) {
        setWorkflowError("stale", binding.reason);
        return;
      }
      const record = baseRecord(prepared, "prepared");
      if (!writeProgress(record)) {
        setWorkflowError("storage", "local repair progress could not be saved");
        return;
      }
      if (!mountedRef.current) return;
      setManifest(prepared);
      setApplyReceipt(null);
      setVerificationReceipt(null);
      setFlow("prepared");
      finishRequest(false);
    } catch (error) {
      if (!mountedRef.current) return;
      const code = daemonErrorMessage(error);
      const noCurrentFinding = code === "repair_current_finding_missing" ||
        code === "repair_target_stale" || code === "unsupported_repair_finding";
      setWorkflowError(noCurrentFinding ? "stale" : code === "repair_current_check_unavailable" ? "check_unavailable" : "prepare", error);
    }
  };

  const verify = async (nextManifest: RepairManifest, receipt: RepairApplyReceipt) => {
    if (!identity || !payload) return;
    if (!validateRepairApplyReceipt(receipt, nextManifest)) {
      setWorkflowError("verify", "the apply receipt did not match the prepared change");
      finishRequest(true);
      return;
    }
    setFlow("verifying");
    try {
      // Recovery may outlive the source detail, so use the scope frozen in
      // the manifest rather than re-deriving it from a current target.
      const lintScope = nextManifest.source.lint_scope;
      const general = await repairLint(lintQueryForScope("general", lintScope));
      const deep = needsDeepVerification(nextManifest, checkId)
        ? await repairLint(lintQueryForScope("deep", lintScope))
        : undefined;
      const requiredDeepCheckIds = nextManifest.post_assertions?.verification_policy?.kind === "applicable_checks"
        ? nextManifest.post_assertions.verification_policy.required_deep_check_ids
        : deep ? [CHECK_CLASSIFICATION] : [];
      if (!reportReady(general, "general") ||
          (deep && (!hasApplicableCheck(deep, requiredDeepCheckIds[0] ?? CHECK_CLASSIFICATION) ||
            requiredDeepCheckIds.some((required) => !hasApplicableCheck(deep, required))))) {
        throw new Error("the fresh lint report is incomplete");
      }
      const verified = await repairVerify({
        manifest_id: nextManifest.manifest_id,
        manifest_digest: nextManifest.manifest_digest,
        apply_receipt_digest: receipt.receipt_digest,
        general_report: general,
        ...(deep ? { deep_report: deep } : {}),
      });
      if (!validateRepairVerificationReceipt(verified, nextManifest, receipt)) {
        throw new Error("the verification receipt did not match the applied change");
      }
      const record = baseRecord(nextManifest, "verified");
      record.applyReceipt = receipt;
      record.verificationReceipt = verified;
      if (!writeProgress(record)) {
        setWorkflowError("storage", "the verification receipt could not be saved");
        finishRequest(true);
        return;
      }
      if (!mountedRef.current) return;
      setVerificationReceipt(verified);
      setFlow("verified");
      finishRequest(false);
      if (!reportedVerifiedRef.current) {
        reportedVerifiedRef.current = true;
        onVerified();
      }
    } catch (error) {
      if (!mountedRef.current) return;
      setFlow("applied_unverified");
      setErrorKind("verify");
      setErrorDetail(diagnostic(error));
      finishRequest(true);
    }
  };

  const apply = async () => {
    if (!manifest || !identity) return;
    if (flow === "prepared" && previewEntitiesMissing) return;
    // Reserve the approval click synchronously before the asynchronous digest
    // check so two same-tick clicks cannot both reach the daemon.
    if (approvalPreflightRef.current) return;
    approvalPreflightRef.current = true;
    // Lock the host before the asynchronous digest check. A dialog
    // navigation during this final local preflight must not race the apply.
    setBusyState(true);
    // Re-check the binding and signed payload at the final approval boundary.
    // A persisted or mutated preview must never be turned into a different
    // daemon request by the UI.
    const binding = validateRepairManifestBinding(manifest, identity);
    if (!binding.ok) {
      approvalPreflightRef.current = false;
      setWorkflowError("stale", binding.reason);
      return;
    }
    let digestValid = false;
    try {
      digestValid = await validateRepairManifestDigest(manifest);
    } catch (error) {
      approvalPreflightRef.current = false;
      setWorkflowError("stale", error);
      return;
    }
    if (!digestValid) {
      approvalPreflightRef.current = false;
      setWorkflowError("stale", "the prepared manifest digest is invalid");
      return;
    }
    if (!mountedRef.current) {
      approvalPreflightRef.current = false;
      return;
    }
    if (!beginRequest()) {
      approvalPreflightRef.current = false;
      return;
    }
    approvalPreflightRef.current = false;
    setErrorKind(null);
    setErrorDetail(null);
    setFlow("applying");
    const applyingRecord = baseRecord(manifest, "applying");
    if (!writeProgress(applyingRecord)) {
      setFlow("prepared");
      setErrorKind("storage");
      finishRequest(false);
      return;
    }
    try {
      const receipt = await repairApply(applyRequestForManifest(manifest));
      setApplyReceipt(receipt);
      if (!validateRepairApplyReceipt(receipt, manifest)) {
        // The endpoint may have committed before returning a malformed or
        // mismatched receipt. Preserve the same applying record and require
        // explicit retry of that exact manifest.
        setApplyReceipt(null);
        const uncertainRecord = baseRecord(manifest, "applying");
        setFlow("uncertain_apply");
        if (!writeProgress(uncertainRecord)) {
          setErrorKind("storage");
          setErrorDetail("the apply receipt did not match the prepared change and recovery could not be saved");
        } else {
          setErrorKind("unknown_apply");
          setErrorDetail("the apply receipt did not match the prepared change");
        }
        finishRequest(true);
        return;
      }
      const appliedRecord = baseRecord(manifest, "applied_unverified");
      appliedRecord.applyReceipt = receipt;
      if (!writeProgress(appliedRecord)) {
        setFlow("applied_unverified");
        finishRequest(true);
        return;
      }
      await verify(manifest, receipt);
    } catch (error) {
      if (!mountedRef.current) return;
      const kind = classifyApplyFailure(error);
      if (kind === "pre_apply") {
        // The daemon explicitly refused before mutation; make the same
        // manifest retryable and keep the proposal pending.
        const retryRecord = baseRecord(manifest, "prepared");
        if (writeProgress(retryRecord)) setFlow("prepared");
        else setFlow("error");
        setErrorKind("pre_apply");
        setErrorDetail(diagnostic(error));
        finishRequest(false);
      } else {
        // Applying began, so this manifest is retained forever for explicit
        // recovery. Never prepare a replacement after this point.
        setFlow("uncertain_apply");
        setErrorKind("unknown_apply");
        setErrorDetail(diagnostic(error));
        finishRequest(true);
      }
    }
  };

  const recover = async () => {
    if (!manifest) return;
    if (flow === "uncertain_apply") {
      await apply();
      return;
    }
    if (flow === "applied_unverified" && applyReceipt) {
      if (!beginRequest()) return;
      await verify(manifest, applyReceipt);
    }
  };

  const retryRecoveryRead = () => {
    if (busy || !mountedRef.current) return;
    // Recovery failures keep the host locked. Reserve the next read
    // synchronously so a repeated click cannot start two reads.
    setBusyState(true);
    setRecoveryAttempt((attempt) => attempt + 1);
  };

  const finishVerified = () => {
    if (!manifest || !applyReceipt || !verificationReceipt || reportedVerifiedRef.current) return;
    if (!validateRepairVerificationReceipt(verificationReceipt, manifest, applyReceipt)) return;
    reportedVerifiedRef.current = true;
    onVerified();
  };

  const mutationPreview = manifest?.mutation;
  const manifestBeforeMemoryType = mutationPreview?.kind === "reclassify_memory"
    ? mutationPreview.before_memory_type
    : undefined;
  const sourceMemoryType = manifestBeforeMemoryType !== undefined ? manifestBeforeMemoryType : currentType;
  const boundApplyReceipt = !!(manifest && applyReceipt && validateRepairApplyReceipt(applyReceipt, manifest));
  const uncategorizedLabel = t("sourceRepair.uncategorized");
  const editable = supported && (flow === "ready" ||
    (flow === "error" && (errorKind === "prepare" || errorKind === "pre_apply" || errorKind === "check_unavailable")));
  const previewEntities = mutationPreview?.kind === "complete_entity_extraction"
    ? mutationPreview.entity_ids.map((id) => entities.find((entity) => entity.id === id)?.name ?? null).filter((name): name is string => name != null)
    : [];
  const previewEntitiesMissing = mutationPreview?.kind === "complete_entity_extraction" &&
    previewEntities.length !== mutationPreview.entity_ids.length;

  const actions = <>
      {flow === "uncertain_apply" && <button type="button" style={buttonStyle} disabled={busy} onClick={() => void recover()}>{t("sourceRepair.recoverApply")}</button>}
      {flow === "applied_unverified" && applyReceipt && <button type="button" style={buttonStyle} disabled={busy} onClick={() => void recover()}>{t("sourceRepair.retryVerification")}</button>}
      {flow === "verified" && <button type="button" style={buttonStyle} onClick={finishVerified}>{t("sourceRepair.continueVerified")}</button>}
      {flow === "prepared" && <button type="button" style={{ ...buttonStyle, backgroundColor: "var(--mem-accent-indigo)", color: "var(--mem-bg)", fontWeight: 600 }} disabled={busy || errorKind === "storage" || previewEntitiesMissing} onClick={() => void apply()}>{t("sourceRepair.applyChange")}</button>}
      {editable && <button type="button" style={{ ...buttonStyle, backgroundColor: "var(--mem-accent-indigo)", borderColor: "var(--mem-accent-indigo)", color: "var(--mem-bg)" }} disabled={busy} onClick={() => void prepare()}>{t("sourceRepair.prepareChange")}</button>}
  </>;

  const diagnosticDetails = (
    <details className="source-repair-details">
      <summary>{t("sourceRepair.showDetails")}</summary>
      {errorDetail && <pre>{errorDetail}</pre>}
      {manifest && <pre>{JSON.stringify({ manifest_id: manifest.manifest_id, manifest_digest: manifest.manifest_digest, check_id: manifest.source.check_id, target: manifest.target }, null, 2)}</pre>}
      {technicalDetails}
    </details>
  );

  const recoveryRetryAction = (
    <button type="button" style={buttonStyle} disabled={busy} onClick={retryRecoveryRead}>
      {t("sourceRepair.retryRecovery")}
    </button>
  );

  if ((!supported && !readOnlyPageCheck) || !payload || !identity) {
    return <div className="source-repair"><p role="status" style={paneStyle}>{t("sourceRepair.unsupportedPending")}</p>{diagnosticDetails}</div>;
  }

  if (flow === "inspecting") {
    return <div role="status" style={paneStyle}>{t("sourceRepair.inspecting")}</div>;
  }

  if (flow === "error" && errorKind === "target") {
    return <div role="alert" className="source-repair-notice" style={paneStyle}>
      <p>{t("sourceRepair.targetMissing")}</p>
      {diagnosticDetails}
    </div>;
  }

  if (flow === "error" && errorKind === "recovery") {
    return <div role="alert" className="source-repair-notice" style={paneStyle}>
      <p>{t("sourceRepair.recoveryFailed")}</p>
      {actionHost ? createPortal(recoveryRetryAction, actionHost) : recoveryRetryAction}
      {diagnosticDetails}
    </div>;
  }

  return (
    <div className="source-repair" style={paneStyle}>
      {readOnlyPageCheck && (
        <div role="status" className="source-repair-notice" style={paneStyle}>
          <p>{t("sourceRepair.unsupportedPending")}</p>
        </div>
      )}
      {errorKind === "target" && (flow === "uncertain_apply" || flow === "applied_unverified") && (
        <div role="alert" className="source-repair-notice" style={paneStyle}>
          <p>{t("sourceRepair.targetMissing")}</p>
        </div>
      )}
      {errorKind === "stale" && (
        <div role="alert" className="source-repair-notice" style={paneStyle}>
          <p>{t("sourceRepair.staleProposal")}</p>
          <p>{t("sourceRepair.pendingUntilReviewed")}</p>
        </div>
      )}
      {errorKind === "storage" && (
        <div role="alert" className="source-repair-notice" style={paneStyle}>
          <p>{t("sourceRepair.storageFailed")}</p>
        </div>
      )}
      {errorKind === "prepare" && (
        <div role="alert" className="source-repair-notice" style={paneStyle}>
          <p>{t("sourceRepair.prepareFailed")}</p>
        </div>
      )}
      {errorKind === "check_unavailable" && (
        <div role="alert" className="source-repair-notice" style={paneStyle}>
          <p>{t("sourceRepair.checkUnavailable")}</p>
        </div>
      )}
      {errorKind === "pre_apply" && (
        <div role="alert" className="source-repair-notice" style={paneStyle}>
          <p>{t("sourceRepair.notApplied")}</p>
        </div>
      )}
      {errorKind === "unknown_apply" && (
        <div role="alert" className="source-repair-notice" style={paneStyle}>
          <p>{t("sourceRepair.unknownApply")}</p>
          <p>{t("sourceRepair.recoverSameChange")}</p>
        </div>
      )}
      {boundApplyReceipt && (flow === "verifying" || flow === "applied_unverified") && (
        <div role={errorKind === "verify" && flow === "applied_unverified" ? "alert" : "status"} className="source-repair-notice" style={paneStyle}>
          <p>{t(errorKind === "verify" && flow === "applied_unverified"
            ? "sourceRepair.appliedVerificationFailed"
            : "sourceRepair.verificationPending")}</p>
        </div>
      )}
      {errorKind === "verify" && !boundApplyReceipt && (
        <div role="alert" className="source-repair-notice" style={paneStyle}>
          <p>{t("sourceRepair.verifyFailed")}</p>
        </div>
      )}

      {currentMemory && (
        <div className="source-repair-source">
          <p style={{ margin: "0 0 6px", fontWeight: 600 }}>{currentMemory.title || t("sourceRepair.untitledMemory")}</p>
          <p className="source-repair-content">{currentMemory.content}</p>
          {checkId === CHECK_CLASSIFICATION && <p style={{ margin: "10px 0 0", color: "var(--mem-text-secondary)" }}>
            {t(manifestBeforeMemoryType !== undefined ? "sourceRepair.originalMemoryType" : "sourceRepair.currentMemoryType", {
              type: isMemoryType(sourceMemoryType)
                ? t(`importView.typeLabels.${sourceMemoryType}`)
                : t("sourceRepair.unclassified"),
            })}
          </p>}
        </div>
      )}
      {currentPage && (
        <div className="source-repair-source">
          <p style={{ margin: "0 0 6px", fontWeight: 600 }}>{currentPage.title}</p>
          <p className="source-repair-content">{currentPage.content}</p>
        </div>
      )}

      {editable && checkId === CHECK_CLASSIFICATION && (
        <label style={labelStyle}>
          {t("sourceRepair.newMemoryType")}
          <select aria-label={t("sourceRepair.newMemoryType")} value={selectedType} onChange={(event) => setSelectedType(event.target.value as MemoryType | "")} disabled={busy}>
            <option value="">{t("sourceRepair.chooseMemoryType")}</option>
            {MEMORY_TYPES.map((type) => <option key={type} value={type}>{t(`importView.typeLabels.${type}`)}</option>)}
          </select>
        </label>
      )}
      {editable && checkId === CHECK_DUPLICATE_TITLES && (
        <label style={labelStyle}>
          {t("sourceRepair.newPageTitle")}
          <input aria-label={t("sourceRepair.newPageTitle")} value={newTitle} onChange={(event) => setNewTitle(event.target.value)} disabled={busy} />
        </label>
      )}
      {editable && checkId === CHECK_ENRICHMENT && (
        <fieldset className="source-repair-entities" disabled={busy}>
          <legend>{t("sourceRepair.existingEntities")}</legend>
          <p style={{ margin: 0, color: "var(--mem-text-secondary)" }}>{t("sourceRepair.sameScopeEntities", { scope: currentSpace ?? uncategorizedLabel })}</p>
          {entities.length === 0 ? <p style={{ margin: 0 }}>{t("sourceRepair.noEntities")}</p> : entities.map((entity) => (
            <label key={entity.id} className="source-repair-entity">
              <input
                type="checkbox"
                checked={selectedEntities.has(entity.id)}
                onChange={(event) => setSelectedEntities((old) => {
                  const next = new Set(old);
                  if (event.target.checked) next.add(entity.id); else next.delete(entity.id);
                  return next;
                })}
              />
              <span>{entity.name} <span style={{ color: "var(--mem-text-secondary)" }}>({t(`atlas.entityType.${entity.entity_type}`, { defaultValue: entity.entity_type })})</span></span>
            </label>
          ))}
          <p style={{ margin: 0, color: "var(--mem-text-secondary)" }}>{t("sourceRepair.emptySelectionAllowed")}</p>
        </fieldset>
      )}

      {manifest && (flow === "prepared" || flow === "applying" || flow === "uncertain_apply" || flow === "verifying" || flow === "applied_unverified" || flow === "verified") && (
        <div className="source-repair-preview">
          <p style={{ margin: "0 0 8px", fontWeight: 600 }}>{t("sourceRepair.previewTitle")}</p>
          {mutationPreview?.kind === "reclassify_memory" && <p style={{ margin: 0 }}>{t("sourceRepair.typePreview", { before: isMemoryType(mutationPreview.before_memory_type) ? t(`importView.typeLabels.${mutationPreview.before_memory_type}`) : t("sourceRepair.unclassified"), after: t(`importView.typeLabels.${mutationPreview.after_memory_type}`) })}</p>}
          {mutationPreview?.kind === "rename_page_title" && <p style={{ margin: 0 }}>{t("sourceRepair.titlePreview", { before: mutationPreview.before_title, after: mutationPreview.after_title })}</p>}
          {mutationPreview?.kind === "complete_entity_extraction" && <p style={{ margin: 0 }}>{previewEntitiesMissing ? t("sourceRepair.selectedEntitiesUnavailable") : previewEntities.length > 0 ? t("sourceRepair.entityPreview", { entities: previewEntities.join(", ") }) : t("sourceRepair.entityPreviewEmpty")}</p>}
        </div>
      )}

      {actionHost ? createPortal(actions, actionHost) : actions}
      {flow === "preparing" && <p role="status" style={{ margin: 0 }}>{t("sourceRepair.preparing")}</p>}
      {flow === "applying" && <p role="status" style={{ margin: 0 }}>{t("sourceRepair.applying")}</p>}
      {flow === "verifying" && <p role="status" style={{ margin: 0 }}>{t("sourceRepair.verifying")}</p>}
      {diagnosticDetails}
    </div>
  );
}
