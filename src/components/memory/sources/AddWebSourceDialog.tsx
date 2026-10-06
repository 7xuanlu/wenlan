// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useRef, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ingestWebpage, listIndexedFiles, type IngestWebpageRequest } from "../../../lib/tauri";
import SourceDialog from "./SourceDialog";

export default function AddWebSourceDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [url, setUrl] = useState("");
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [validation, setValidation] = useState("");
  const [replacement, setReplacement] = useState<IngestWebpageRequest | null>(null);
  const incarnationRef = useRef({ active: true });
  useEffect(() => {
    // StrictMode may replay effect setup/cleanup. Each setup owns its token;
    // cleanup never reactivates requests captured by an earlier setup.
    const incarnation = { active: true };
    incarnationRef.current = incarnation;
    return () => { incarnation.active = false; };
  }, []);
  const close = useCallback(() => {
    if (!incarnationRef.current.active) return;
    // All dismiss paths invalidate synchronously, before parent unmount.
    incarnationRef.current.active = false;
    onClose();
  }, [onClose]);
  const save = useMutation({
    mutationFn: async ({ request, replace, incarnation }: {
      request: IngestWebpageRequest;
      replace: boolean;
      incarnation: { active: boolean };
    }) => {
      if (!incarnation.active) return { kind: "cancelled" as const };
      if (!replace) {
        // Fetch the current inventory; a stale list must not authorize replacing an excerpt.
        const files = await listIndexedFiles();
        if (!incarnation.active) return { kind: "cancelled" as const };
        if (files.some(file => file.source === "webpage" && file.source_id === request.url)) {
          return { kind: "confirmation" as const, request };
        }
      }
      try {
        await ingestWebpage({ ...request, create_only: !replace });
      } catch (error) {
        // The inventory is only a hint: another writer may save this URL
        // before our transaction starts. Only explicit consent may replace it.
        if (!replace && (error instanceof Error ? error.message : error) === "WEBPAGE_ALREADY_EXISTS") {
          return { kind: "confirmation" as const, request };
        }
        throw error;
      }
      return { kind: "saved" as const };
    },
    onSuccess: (result, { incarnation }) => {
      // Issued writes continue after dismissal. Refresh inventory on success,
      // but their completion cannot control a later dialog or its draft.
      if (result.kind === "saved") qc.invalidateQueries({ queryKey: ["indexedFiles"] });
      if (!incarnation.active || result.kind === "cancelled") return;
      if (result.kind === "confirmation") { setReplacement(result.request); return; }
      toast(t("sourceAccess.excerptSaved"));
      close();
    },
  });
  const replacementMatches = replacement?.url === url.trim()
    && replacement?.content === content
    && replacement?.title === (title.trim() || (() => { try { return new URL(url).hostname; } catch { return ""; } })());

  return <SourceDialog title={t("sourceAccess.webTitle")} onClose={close}>
    <form onSubmit={event => {
      event.preventDefault();
      if (save.isPending || !incarnationRef.current.active) return;
      let address: URL;
      try {
        address = new URL(url.trim());
        if (!["http:", "https:"].includes(address.protocol) || address.username || address.password) throw new Error();
      } catch { setValidation(t("sourceAccess.urlInvalid")); return; }
      if (!content.trim()) { setValidation(t("sourceAccess.contentRequired")); return; }
      setValidation("");
      save.mutate({ request: { url: url.trim(), title: title.trim() || address.hostname, content }, replace: replacementMatches, incarnation: incarnationRef.current });
    }}>
      <label>{t("sourceAccess.url")}<input type="url" required value={url} onChange={event => { setUrl(event.target.value); setReplacement(null); }} autoComplete="url" placeholder={t("sourceAccess.urlPlaceholder")} /></label>
      <label>{t("sourceAccess.title")}<input value={title} onChange={event => { setTitle(event.target.value); setReplacement(null); }} /></label>
      <label>{t("sourceAccess.content")}<textarea required value={content} onChange={event => { setContent(event.target.value); setReplacement(null); }} aria-describedby="source-excerpt-hint" /></label>
      <p id="source-excerpt-hint">{t("sourceAccess.contentHint")}</p>
      {replacementMatches && <p role="alert">{t("sourceAccess.replaceWarning")}</p>}
      {(validation || save.isError) && <div role="alert" className="source-dialog-error">{validation || t("sourceAccess.addFailed")}{save.isError && <p>{String(save.error)}</p>}</div>}
      <div className="source-dialog-actions">
        <button type="button" className="page-editor-action" onClick={close}>{t("sourceAccess.cancel")}</button>
        <button type="submit" className="page-editor-action" disabled={save.isPending}>{t(save.isPending ? "sourceAccess.adding" : replacementMatches ? "sourceAccess.replaceExcerpt" : "sourceAccess.saveExcerpt")}</button>
      </div>
    </form>
  </SourceDialog>;
}
