// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { publishPageDraft } from "../../../lib/tauri";
import type { PageProjectionIssue } from "./pageInventory";
import "./wikiInventoryOverview.css";

/** A real publish outcome carried through Main navigation, never inferred from
 * missing files or an unconfigured provider. Content is already persisted. */
export function PageProjectionNotice({ pageId, issue, disabled, onResolved }: {
  readonly pageId: string;
  readonly issue: PageProjectionIssue;
  readonly disabled: boolean;
  readonly onResolved?: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [pending, setPending] = useState(false);
  const [failed, setFailed] = useState(false);
  const [resolved, setResolved] = useState(false);
  const mountedRef = useRef(true);
  useEffect(() => { mountedRef.current = true; return () => { mountedRef.current = false; }; }, []);
  if (resolved) return null;
  const retry = async () => {
    if (pending || disabled) return;
    setPending(true); setFailed(false);
    try {
      // Publishing an already-active expectedVersion+1 Page replays only the
      // file projection; the daemon rejects intervening edits instead of
      // overwriting them or publishing a duplicate.
      const result = await publishPageDraft({ id: pageId, expectedVersion: issue.expectedVersion });
      if (result.projection_status === "pending") {
        if (mountedRef.current) setFailed(true);
        return;
      }
      queryClient.setQueryData(["page", pageId], result);
      await Promise.all([queryClient.invalidateQueries({ queryKey: ["pages"] }), queryClient.invalidateQueries({ queryKey: ["page", pageId] })]);
      if (mountedRef.current) { setResolved(true); onResolved?.(); }
    } catch { if (mountedRef.current) setFailed(true); } finally { if (mountedRef.current) setPending(false); }
  };
  return <div role="status" aria-live="polite" className="wiki-projection-notice">
    <span>{t(failed ? "pages.folders.retryLocationError" : "pages.folders.locationPending")}</span>
    <button type="button" disabled={disabled || pending} onClick={() => void retry()}>{t("pages.folders.retryLocation")}</button>
  </div>;
}
