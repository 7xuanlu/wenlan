// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useRef, useState } from "react";
import type { SpaceIdentityValue } from "./SpaceDossierHeader";

const SAVE_DELAY_MS = 750;
const equal = (a: SpaceIdentityValue, b: SpaceIdentityValue) => a.name === b.name && a.description === b.description;
const normalize = (value: SpaceIdentityValue) => ({ name: value.name.trim(), description: value.description.trim() });

/** Serializes identity writes and preserves newer typing or a failed draft. */
export function useSpaceIdentityAutosave(remote: SpaceIdentityValue, save: (value: SpaceIdentityValue) => Promise<void> | void) {
  const [draft, setDraft] = useState(remote);
  const [status, setStatus] = useState<"idle" | "saving" | "error">("idle");
  const draftRef = useRef(draft);
  const baseline = useRef(remote);
  const saveRef = useRef(save);
  saveRef.current = save;
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inFlight = useRef<Promise<boolean> | null>(null);
  const failed = useRef(false);
  const composing = useRef(false);
  const focused = useRef(false);
  const mounted = useRef(true);

  const clearTimer = useCallback(() => {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
  }, []);

  const flush = useCallback(async (explicitRetry = false): Promise<boolean> => {
    clearTimer();
    if (composing.current) return false;

    let retrying = explicitRetry;
    while (true) {
      if (inFlight.current) {
        if (!await inFlight.current) return false;
        continue;
      }
      if (failed.current && !retrying) return false;
      const input = draftRef.current;
      const value = normalize(input);
      if (!value.name) {
        failed.current = true;
        if (mounted.current) setStatus("error");
        return false;
      }
      if (equal(value, baseline.current)) return true;

      retrying = false;
      failed.current = false;
      if (mounted.current) setStatus("saving");
      const operation = (async (): Promise<boolean> => {
        try {
          await saveRef.current(value);
          baseline.current = value;
          if (!focused.current && equal(draftRef.current, input)) {
            draftRef.current = value;
            if (mounted.current) setDraft(value);
          }
          if (mounted.current) setStatus("idle");
          return true;
        } catch {
          failed.current = true;
          if (mounted.current) setStatus("error");
          return false;
        }
      })();
      inFlight.current = operation;
      const saved = await operation;
      if (inFlight.current === operation) inFlight.current = null;
      if (!saved) return false;
      if (composing.current) return false;
      if (equal(normalize(draftRef.current), baseline.current)) return true;
      // A newer edit arrived during the write. Drain it once in order; a
      // failure in this follow-up is reported without an automatic retry.
    }
  }, [clearTimer]);

  useEffect(() => {
    // Polling may refresh the old server value while an unsaved draft is visible.
    if (focused.current || inFlight.current || !equal(normalize(draftRef.current), baseline.current)) return;
    baseline.current = remote;
    draftRef.current = remote;
    setDraft(remote);
  }, [remote.name, remote.description]);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      clearTimer();
    };
  }, [clearTimer]);

  function change(value: SpaceIdentityValue) {
    draftRef.current = value;
    setDraft(value);
    failed.current = false;
    clearTimer();
    if (!composing.current) timer.current = setTimeout(() => { void flush(); }, SAVE_DELAY_MS);
  }

  function compositionStart() { composing.current = true; clearTimer(); }
  function compositionEnd() {
    composing.current = false;
    clearTimer();
    timer.current = setTimeout(() => { void flush(); }, SAVE_DELAY_MS);
  }

  function focus() { focused.current = true; }
  function blur() { focused.current = false; void flush(); }

  return { draft, status, change, flush, focus, blur, compositionStart, compositionEnd };
}
