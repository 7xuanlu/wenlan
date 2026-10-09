// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it, vi } from "vitest";
import type { RepairApplyReceipt, RepairLintReport, RepairManifest, RepairPlanEntry, RepairPlanSummary, RepairVerificationReceipt } from "./repairTypes";
import { isAutomaticRepair, knowledgeEntryKey, KnowledgeCheckController } from "./knowledgeCheckWorkflow";

const digest = (n: string) => n.repeat(64);
function report(complete = true): RepairLintReport {
  return {
    report_schema_version: 1, check_catalog_version: 1, profile: "general", scope: { kind: "global" },
    capability_context: "daemon_operator_endpoint_unauthenticated_unverified", config_fingerprint: digest("a"),
    snapshots: { db: { mode: "transactional_read_only", analysis_digest: "b".repeat(16), post_run_digest: "b".repeat(16) }, pages: { mode: "best_effort", before_scan_digest: "d".repeat(16), after_scan_digest: "d".repeat(16) } },
    producer_receipt: { runtime_commit: null }, checks: [], totals: { checks: 0, passed: 0, findings: 0, actionable_findings: 0, advisory_findings: 0, incomplete: 0 }, complete,
  };
}
function manifest(id: string, checkId = "memory.source_agent_blank"): RepairManifest {
  const owner = { kind: "memory" as const, source_id: id, scope: { kind: "registered" as const, space: "space-1" } };
  return {
    manifest_schema_version: 1, manifest_id: `m-${id}`, prepared_at: 1,
    source: { report_schema_version: 1, check_catalog_version: 1, lint_scope: { kind: "global" }, report_scope: { kind: "global" }, check_id: checkId, finding: null, general_snapshots: {}, general_producer_receipt: { runtime_commit: null } },
    target: owner, expected_state: { canonical_receipt: digest("f") }, writer: "normalize_memory_source_agent",
    mutation: { kind: "normalize_memory_source_agent", before_source_agent: "   " },
    allowed_effects: { owner, fields: ["source_agent"] }, rollback: { format_version: 1, relative_path: "rollback", digest: digest("1") },
    post_assertions: { target_check_id: checkId, target_evidence_id: digest("2"), general_baseline: [], deep_baseline: [], verification_policy: { kind: "general_only" }, require_complete_general: true, reject_new_actionable: true, reject_new_incomplete: true, allowed_non_target_check_deltas: [] }, manifest_digest: digest("3"),
  };
}
function entry(id: string): RepairPlanEntry {
  return { check_id: "identity.memory_state_integrity", occurrence_digest: digest(id), affected_records: [{ kind: "memory", durable_id: id }], resolution: { disposition: "ready", manifest: manifest(id, "identity.memory_state_integrity") } };
}
function receipts(m: RepairManifest): [RepairApplyReceipt, RepairVerificationReceipt] {
  const apply: RepairApplyReceipt = { receipt_schema_version: 1, manifest_id: m.manifest_id, manifest_digest: m.manifest_digest, applied_at: 2, before_target_receipt: digest("4"), after_target_receipt: digest("5"), non_target_before: digest("6"), non_target_after: digest("7"), actual_effects: m.allowed_effects, writer: m.writer, receipt_digest: digest("8") };
  const verify: RepairVerificationReceipt = { receipt_schema_version: 1, manifest_id: m.manifest_id, manifest_digest: m.manifest_digest, apply_receipt_digest: apply.receipt_digest, verified_at: 3, general_snapshots: {}, receipt_digest: digest("9") };
  return [apply, verify];
}
function planSummary(count: number): RepairPlanSummary { return { plan_id: "plan", plan_digest: digest("a"), scope: { kind: "global" }, entry_count: count, deterministic_complete: true, semantic_complete: false }; }
function manifestById(id: string): RepairManifest { return entriesFixture().find((candidate) => candidate.manifest_id === id) ?? manifest("fallback", "identity.memory_state_integrity"); }
function entriesFixture(): RepairManifest[] { return [manifest("1", "identity.memory_state_integrity"), manifest("2", "identity.memory_state_integrity")]; }
function harness(overrides: Record<string, unknown> = {}) {
  const stored = new Map<string, string>();
  const storage = { getItem: vi.fn((key: string) => stored.get(key) ?? null), setItem: vi.fn((key: string, value: string) => { stored.set(key, value); }), removeItem: vi.fn((key: string) => { stored.delete(key); }) } as unknown as Storage;
  const calls: string[] = [];
  const locks = { request: vi.fn(async (_name: string, _opts: unknown, fn: () => Promise<unknown>) => fn()) };
  const entries = [entry("1"), entry("2")];
  const deps = {
    storage, locks,
    repairLint: vi.fn(async () => report()), repairPlan: vi.fn(async () => planSummary(entries.length)),
    repairPlanEntries: vi.fn(async ({ offset }: { offset: number }) => ({ plan_id: "plan", plan_digest: digest("a"), scope: { kind: "global" as const }, offset, next_offset: offset + PAGE >= entries.length ? null : offset + PAGE, total_entries: entries.length, entries: entries.slice(offset, offset + PAGE) })),
    repairValidateManifest: vi.fn(async () => true), repairApply: vi.fn(async ({ manifest_id }: { manifest_id: string }) => { calls.push(`apply:${manifest_id}`); return receipts(manifestById(manifest_id))[0]; }),
    repairOperationStatus: vi.fn(async () => { throw new Error("not set"); }), repairCancel: vi.fn(async () => { throw new Error("not set"); }),
    repairVerify: vi.fn(async ({ manifest_id }: { manifest_id: string }) => { calls.push(`verify:${manifest_id}`); return receipts(manifestById(manifest_id))[1]; }),
    repairResumeRuntime: vi.fn(async ({ apply: a }: { apply: { manifest_id: string } }) => { calls.push(`resume:${a.manifest_id}`); }), getActivity: vi.fn(async () => ({ state: { status: "ok" } })),
    ...overrides,
  };
  return { controller: new KnowledgeCheckController(deps as never), deps, entries, calls, storage, locks };
}
const PAGE = 100;

describe("knowledge check workflow", () => {
  it("checks general only, validates complete pagination, and never applies during check", async () => {
    const h = harness();
    await h.controller.check();
    expect(h.deps.repairLint).toHaveBeenCalledWith({ profile: "general" });
    expect(h.deps.repairPlan).toHaveBeenCalledWith({ scope: { kind: "global" }, general_report: expect.any(Object) });
    expect(h.controller.getSnapshot().entries).toHaveLength(2);
    expect(h.deps.repairApply).not.toHaveBeenCalled();
  });

  it("rejects incomplete reports and a mismatched/truncated plan page", async () => {
    const incomplete = harness({ repairLint: vi.fn(async () => report(false)) });
    await incomplete.controller.check();
    expect(incomplete.deps.repairPlan).not.toHaveBeenCalled();
    expect(incomplete.controller.getSnapshot().phase).toBe("error");
    expect(incomplete.controller.getSnapshot().report?.complete).toBe(false);
    const truncated = harness({ repairPlanEntries: vi.fn(async () => ({ plan_id: "wrong", plan_digest: digest("a"), scope: { kind: "global" }, offset: 0, next_offset: null, total_entries: 2, entries: [entry("1")] })) });
    await truncated.controller.check();
    expect(truncated.controller.getSnapshot().entries).toHaveLength(0);
    expect(truncated.controller.getSnapshot().phase).toBe("error");
    const malformed = harness({ repairPlanEntries: vi.fn(async () => ({ plan_id: "plan", plan_digest: digest("a"), scope: { kind: "global" }, offset: 0, next_offset: null, total_entries: 2, entries: [{ check_id: "x", occurrence_digest: digest("1"), affected_records: null, resolution: null }] })) });
    await malformed.controller.check();
    expect(malformed.controller.getSnapshot().entries).toHaveLength(0);
    expect(malformed.controller.getSnapshot().phase).toBe("error");
  });

  it("follows short plan pages by their validated next offset", async () => {
    const many = Array.from({ length: 105 }, (_, i) => {
      const value = i.toString(16).padStart(64, "0");
      const base = entry(value);
      base.occurrence_digest = value;
      return base;
    });
    const offsets: number[] = [];
    const h = harness({
      repairPlan: vi.fn(async () => planSummary(many.length)),
      repairPlanEntries: vi.fn(async ({ offset }: { offset: number }) => {
        offsets.push(offset);
        const pageEntries = many.slice(offset, offset + 48);
        const next = offset + pageEntries.length < many.length ? offset + pageEntries.length : null;
        return { plan_id: "plan", plan_digest: digest("a"), scope: { kind: "global" as const }, offset, next_offset: next, total_entries: many.length, entries: pageEntries };
      }),
    });
    await h.controller.check();
    expect(offsets).toEqual([0, 48, 96]);
    expect(h.controller.getSnapshot().entries).toHaveLength(105);
  });

  it("allows only exact low-impact writer and field combinations", () => {
    expect(isAutomaticRepair(entry("1"))).toBe(true);
    const unsafe = entry("2");
    if (unsafe.resolution.disposition === "ready") unsafe.resolution.manifest.writer = "archive_empty_source_page";
    expect(isAutomaticRepair(unsafe)).toBe(false);
    const malformed = entry("3");
    if (malformed.resolution.disposition === "ready") malformed.resolution.manifest.allowed_effects.fields = ["page_status"];
    expect(isAutomaticRepair(malformed)).toBe(false);
  });

  it("accepts the backend primary check for self-supersession and rejects other references", () => {
    for (const checkId of ["identity.memory_state_integrity", "memories.supersession_integrity"]) {
      const candidate = entry("1");
      if (candidate.resolution.disposition !== "ready") throw new Error("fixture is not ready");
      candidate.check_id = checkId;
      const m = candidate.resolution.manifest;
      m.source.check_id = checkId;
      m.post_assertions.target_check_id = checkId;
      m.writer = "clear_memory_supersedes";
      m.mutation = { kind: "clear_memory_supersedes", before_supersedes: m.target.kind === "memory" ? m.target.source_id : "invalid" };
      m.allowed_effects.fields = ["supersedes"];
      expect(isAutomaticRepair(candidate)).toBe(true);
      m.mutation.before_supersedes = "different-memory";
      expect(isAutomaticRepair(candidate)).toBe(false);
    }
  });

  it("finishes each selected manifest through verify, runtime resume, and probe before the next", async () => {
    const h = harness();
    await h.controller.check();
    await h.controller.repair(h.entries.map(knowledgeEntryKey));
    expect(h.calls).toEqual(["apply:m-1", "verify:m-1", "resume:m-1", "apply:m-2", "verify:m-2", "resume:m-2"]);
    expect(h.controller.getSnapshot().results[knowledgeEntryKey(h.entries[1])]).toBe("verified");
    expect(h.controller.getSnapshot().phase).toBe("complete");
    await h.controller.check();
    expect(h.controller.getSnapshot().phase).toBe("ready");
  });

  it("reopens a completed batch without requiring recovery", async () => {
    const h = harness();
    await h.controller.check();
    await h.controller.repair(h.entries.map(knowledgeEntryKey));
    const reopened = new KnowledgeCheckController({ ...h.deps } as never);
    await reopened.initialize();
    expect(reopened.getSnapshot().phase).toBe("complete");
    expect(reopened.getSnapshot().recoveryRequired).toBe(false);
    await reopened.check();
    expect(reopened.getSnapshot().phase).toBe("ready");
  });

  it("requires a fresh check after a crash between selected entries", async () => {
    const h = harness();
    const first = knowledgeEntryKey(h.entries[0]);
    const second = knowledgeEntryKey(h.entries[1]);
    const partial = { version: 1, scope: "global", planId: "plan", planDigest: digest("a"), items: [{ key: first, entry: h.entries[0] }, { key: second, entry: h.entries[1] }], results: { [first]: "verified", [second]: "not_started" }, currentKey: null, stage: "prepared" };
    (h.storage.setItem as unknown as (key: string, value: string) => void)("wenlan.knowledge-check.progress.v1", JSON.stringify(partial));
    const reopened = new KnowledgeCheckController({ ...h.deps } as never);
    await reopened.initialize();
    expect(reopened.getSnapshot().phase).toBe("paused");
    expect(reopened.getSnapshot().recoveryRequired).toBe(false);
    await reopened.repair([second]);
    expect(h.deps.repairApply).not.toHaveBeenCalled();
    await reopened.recover();
    expect(reopened.getSnapshot().phase).toBe("paused");
    await reopened.check();
    expect(reopened.getSnapshot().phase).toBe("ready");
  });

  it("captures a selection once, so a second click cannot duplicate sends", async () => {
    let release!: (value: RepairApplyReceipt) => void;
    const h = harness({ repairApply: vi.fn(() => new Promise<RepairApplyReceipt>((resolve) => { release = resolve; })) });
    await h.controller.check();
    const key = knowledgeEntryKey(h.entries[0]);
    const first = h.controller.repair([key]);
    await Promise.resolve(); await Promise.resolve();
    await h.controller.repair([key]);
    release(receipts(manifest("1", "identity.memory_state_integrity"))[0]);
    await first;
    expect(h.deps.repairApply).toHaveBeenCalledTimes(1);
  });

  it("blocks writes when progress storage fails", async () => {
    const h = harness();
    await h.controller.check();
    (h.storage.setItem as unknown as ReturnType<typeof vi.fn>).mockImplementation(() => { throw new Error("storage full"); });
    await h.controller.repair([knowledgeEntryKey(h.entries[0])]);
    expect(h.deps.repairApply).not.toHaveBeenCalled();
    expect(h.controller.getSnapshot().error).toContain("storage full");
  });

  it("stops after verification failure and leaves the applied item visibly unverified", async () => {
    const h = harness({ repairVerify: vi.fn(async () => { throw new Error("verification failed"); }) });
    await h.controller.check();
    await h.controller.repair(h.entries.map(knowledgeEntryKey));
    expect(h.deps.repairApply).toHaveBeenCalledTimes(1);
    expect(h.controller.getSnapshot().results[knowledgeEntryKey(h.entries[0])]).toBe("applied_unverified");
    expect(h.controller.getSnapshot().results[knowledgeEntryKey(h.entries[1])]).toBe("not_started");
  });

  it("recovers an unknown apply from operation status without resending it", async () => {
    const h = harness({
      repairApply: vi.fn(async () => { throw new Error("reply lost"); }),
      repairOperationStatus: vi.fn(async () => { const [apply] = receipts(manifest("1")); return { manifest_id: apply.manifest_id, manifest_digest: apply.manifest_digest, state: { phase: "applied_unverified" as const, apply_receipt: apply } }; }),
    });
    await h.controller.check();
    const key = knowledgeEntryKey(h.entries[0]);
    await h.controller.repair([key]);
    expect(h.controller.getSnapshot().results[key]).toBe("applied_unverified");
    await h.controller.recover();
    expect(h.deps.repairApply).toHaveBeenCalledTimes(1);
    expect(h.deps.repairVerify).toHaveBeenCalledTimes(1);
    expect(h.controller.getSnapshot().results[key]).toBe("verified");
    await h.controller.check();
    expect(h.controller.getSnapshot().phase).toBe("ready");
  });

  it("cancels a prepared preflight failure during explicit recovery, then allows a fresh check", async () => {
    const h = harness({
      repairValidateManifest: vi.fn().mockResolvedValueOnce(false).mockResolvedValue(true),
      repairOperationStatus: vi.fn(async () => ({ manifest_id: "m-1", manifest_digest: digest("3"), state: { phase: "prepared" as const } })),
      repairCancel: vi.fn(async () => ({ manifest_id: "m-1", manifest_digest: digest("3"), state: { phase: "cancelled" as const, cancelled_at: 8 } })),
    });
    await h.controller.check();
    await h.controller.repair([knowledgeEntryKey(h.entries[0])]);
    expect(h.controller.getSnapshot().recoveryRequired).toBe(true);
    await h.controller.recover();
    expect(h.deps.repairApply).not.toHaveBeenCalled();
    expect(h.controller.getSnapshot().recoveryRequired).toBe(false);
    await h.controller.check();
    expect(h.controller.getSnapshot().phase).toBe("ready");
  });

  it("keeps a verified manifest recoverable when runtime resume fails", async () => {
    let resumeCount = 0;
    const h = harness({
      repairResumeRuntime: vi.fn(async () => { resumeCount += 1; if (resumeCount === 1) throw new Error("resume interrupted"); }),
      repairOperationStatus: vi.fn(async () => { const [apply, verification] = receipts(manifest("1", "identity.memory_state_integrity")); return { manifest_id: apply.manifest_id, manifest_digest: apply.manifest_digest, state: { phase: "verified" as const, apply_receipt: apply, verification_receipt: verification } }; }),
    });
    await h.controller.check();
    const key = knowledgeEntryKey(h.entries[0]);
    await h.controller.repair([key]);
    expect(h.controller.getSnapshot().recoveryRequired).toBe(true);
    const reopened = new KnowledgeCheckController({ ...h.deps } as never);
    await reopened.initialize();
    expect(reopened.getSnapshot().recoveryRequired).toBe(true);
    await reopened.recover();
    expect(reopened.getSnapshot().results[key]).toBe("verified");
    expect(reopened.getSnapshot().recoveryRequired).toBe(false);
    await reopened.check();
    expect(reopened.getSnapshot().phase).toBe("ready");
  });

  it("fails closed if browser locks or signed-manifest validation are unavailable", async () => {
    const noLock = harness({ locks: undefined });
    noLock.controller = new KnowledgeCheckController({ ...noLock.deps, locks: undefined } as never);
    await noLock.controller.check();
    await noLock.controller.repair([knowledgeEntryKey(noLock.entries[0])]);
    expect(noLock.deps.repairApply).not.toHaveBeenCalled();
    expect(noLock.storage.setItem).not.toHaveBeenCalled();
    const badDigest = harness({ repairValidateManifest: vi.fn(async () => false) });
    await badDigest.controller.check();
    await badDigest.controller.repair([knowledgeEntryKey(badDigest.entries[0])]);
    expect(badDigest.deps.repairApply).not.toHaveBeenCalled();
  });

  it("does not overwrite another window's persisted outstanding batch", async () => {
    const h = harness();
    await h.controller.check();
    const key = knowledgeEntryKey(h.entries[0]);
    const foreign = { version: 1, scope: "global", planId: "foreign-plan", planDigest: digest("b"), items: [{ key, entry: h.entries[0] }], results: { [key]: "applying" }, currentKey: key, stage: "sent" };
    const raw = JSON.stringify(foreign);
    (h.storage.setItem as unknown as (key: string, value: string) => void)("wenlan.knowledge-check.progress.v1", raw);
    (h.storage.setItem as unknown as ReturnType<typeof vi.fn>).mockClear();
    await h.controller.repair([key]);
    expect(h.deps.repairApply).not.toHaveBeenCalled();
    expect(h.storage.setItem).not.toHaveBeenCalled();
    expect(h.storage.getItem("wenlan.knowledge-check.progress.v1")).toBe(raw);
  });

  it("persists failure state before releasing the lock to a waiting window", async () => {
    const h = harness({ repairApply: vi.fn(async () => { throw new Error("apply reply lost"); }) });
    await h.controller.check();
    const key = knowledgeEntryKey(h.entries[0]);
    const sentinel = "next-window-owned-progress";
    const locks = {
      request: vi.fn(async (_name: string, _options: unknown, callback: (lock: unknown) => Promise<unknown>) => {
        try { return await callback({}); }
        catch (error) {
          // The queued owner acquires after this callback rejects and writes its own record.
          (h.storage.setItem as unknown as (key: string, value: string) => void)("wenlan.knowledge-check.progress.v1", sentinel);
          throw error;
        }
      }),
    };
    const racing = new KnowledgeCheckController({ ...h.deps, locks } as never);
    await racing.initialize();
    // Restore the freshly checked UI snapshot after initialize observed no saved batch.
    await racing.check();
    await racing.repair([key]);
    expect(h.deps.repairApply).toHaveBeenCalledTimes(1);
    expect(h.storage.getItem("wenlan.knowledge-check.progress.v1")).toBe(sentinel);
  });
});
