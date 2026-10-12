// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useId, useRef, useState } from "react";
import type { ReactNode } from "react";
import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { getSourceAnnotation, listPages, updateSourceAnnotation, type Page, type SourceAnnotation } from "../../../lib/tauri";
import { useReferenceNavigation } from "../links/ReferenceNavigationContext";
import { PageIcon } from "../assets/PageIcon";
import { MarkdownEditor } from "../editor/MarkdownEditor";
import "./SourceAnnotations.css";

type SaveState = "saved" | "saving" | "error" | "conflict";
const isConflict = (error: unknown) => /409|conflict|stale/i.test(String(error));
const RECOVERY_LIMIT_BYTES = 64 * 1024;

type RecoveredNoteDraft = { draft: string; baseNotes: string; baseVersion: number };

export function sourceAnnotationRecoveryKey(sourceId: string, space?: string): string {
  return `wenlan:source-annotation-draft:v1:${JSON.stringify(["webpage", sourceId, space ?? null])}`;
}

function utf8Bytes(value: string): number {
  return new TextEncoder().encode(value).byteLength;
}

function readRecoveryDraft(key: string): RecoveredNoteDraft | null {
  try {
    const raw = window.sessionStorage.getItem(key);
    if (!raw || utf8Bytes(raw) > RECOVERY_LIMIT_BYTES) return null;
    const value = JSON.parse(raw) as Partial<RecoveredNoteDraft>;
    if (typeof value.draft !== "string" || typeof value.baseNotes !== "string" || !Number.isInteger(value.baseVersion)) return null;
    if (utf8Bytes(value.draft) > RECOVERY_LIMIT_BYTES || utf8Bytes(value.baseNotes) > RECOVERY_LIMIT_BYTES) return null;
    return { draft: value.draft, baseNotes: value.baseNotes, baseVersion: value.baseVersion as number };
  } catch {
    return null;
  }
}

function writeRecoveryDraft(key: string, value: RecoveredNoteDraft): boolean {
  try {
    const serialized = JSON.stringify(value);
    if (utf8Bytes(value.draft) > RECOVERY_LIMIT_BYTES || utf8Bytes(value.baseNotes) > RECOVERY_LIMIT_BYTES || utf8Bytes(serialized) > RECOVERY_LIMIT_BYTES) return false;
    window.sessionStorage.setItem(key, serialized);
    return true;
  } catch {
    return false;
  }
}

function clearRecoveryDraft(key: string): void {
  try { window.sessionStorage.removeItem(key); } catch { /* Recovery storage is best-effort. */ }
}

export default function SourceAnnotations({ sourceId, space, onReady, onRegisterFlush, renderRelatedPages }: {
  sourceId: string;
  space?: string;
  onReady?: () => void;
  onRegisterFlush?: (flush: (() => Promise<boolean>) | null) => void;
  renderRelatedPages?: (content: ReactNode) => ReactNode;
}) {
  const { t } = useTranslation();
  const navigation = useReferenceNavigation();
  const queryClient = useQueryClient();
  const placeholderId = useId();
  const annotationKey = ["sourceAnnotation", "webpage", sourceId, space ?? null] as const;
  const recoveryKey = sourceAnnotationRecoveryKey(sourceId, space);
  const annotationQuery = useQuery({
    queryKey: annotationKey,
    queryFn: () => getSourceAnnotation("webpage", sourceId, space),
    retry: false,
  });
  useEffect(() => {
    if (!annotationQuery.isPending) onReady?.();
  }, [annotationQuery.isPending, onReady]);
  const [draft, setDraft] = useState<string | null>(null);
  const [editorRevision, setEditorRevision] = useState(0);
  const [saveState, setSaveState] = useState<SaveState>("saved");
  const [noteConflict, setNoteConflict] = useState(false);
  const [pageSaving, setPageSaving] = useState(false);
  const [composing, setComposing] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [pageFilter, setPageFilter] = useState("");
  const [pageSaveFailed, setPageSaveFailed] = useState(false);
  const [pageRetry, setPageRetry] = useState<{ pageId: string; shouldAttach: boolean } | null>(null);
  const [recoveryWarning, setRecoveryWarning] = useState<"storage" | "size" | null>(null);
  const baseline = useRef<SourceAnnotation | null>(null);
  const savedNotes = useRef<string | null>(null);
  const noteConflictRef = useRef(false);
  const draftRef = useRef<string | null>(null);
  const busy = useRef(false);
  const busyWaiters = useRef<Array<() => void>>([]);
  const activeWriteResult = useRef<boolean | null>(null);
  const composingRef = useRef(false);
  const pageWriteFailed = useRef(false);
  const mounted = useRef(true);
  const recoveryInitialized = useRef(false);
  const flushRef = useRef<(notify?: boolean) => Promise<boolean>>(async () => true);
  const registeredFlush = useCallback(() => flushRef.current(), []);

  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);

  useEffect(() => {
    onRegisterFlush?.(registeredFlush);
    return () => onRegisterFlush?.(null);
  }, [onRegisterFlush, registeredFlush]);

  useEffect(() => {
    const latest = annotationQuery.data;
    if (!latest) return;
    if (!recoveryInitialized.current) {
      recoveryInitialized.current = true;
      baseline.current = latest;
      savedNotes.current = latest.notes;
      const recovered = readRecoveryDraft(recoveryKey);
      if (recovered) {
        draftRef.current = recovered.draft;
        setDraft(recovered.draft);
        if (latest.notes === recovered.baseNotes) {
          setSaveState("error"); // A restored draft is retried only after an explicit click.
        } else {
          noteConflictRef.current = true;
          setNoteConflict(true);
          setSaveState("conflict");
        }
        return;
      }
      clearRecoveryDraft(recoveryKey);
      draftRef.current = latest.notes;
      setDraft(latest.notes);
      return;
    }
    // Do not let a background refetch replace a draft the user is still editing.
    if (draftRef.current !== null && savedNotes.current !== null && draftRef.current !== savedNotes.current) return;
    const adoptsDifferentCleanText = savedNotes.current !== null
      && latest.notes !== savedNotes.current
      && (draftRef.current === null || draftRef.current === savedNotes.current);
    baseline.current = latest;
    savedNotes.current = latest.notes;
    draftRef.current = latest.notes;
    setDraft(latest.notes);
    if (adoptsDifferentCleanText) setEditorRevision((revision) => revision + 1);
  }, [annotationQuery.data, recoveryKey]);

  const persistDraft = useCallback((value: string, baseNotes = savedNotes.current ?? "", baseVersion = baseline.current?.version ?? 0) => {
    if (value === baseNotes) {
      clearRecoveryDraft(recoveryKey);
      setRecoveryWarning(null);
      return;
    }
    if (utf8Bytes(value) > RECOVERY_LIMIT_BYTES) {
      clearRecoveryDraft(recoveryKey);
      setRecoveryWarning("size");
      return;
    }
    const didSave = writeRecoveryDraft(recoveryKey, { draft: value, baseNotes, baseVersion });
    if (!didSave) clearRecoveryDraft(recoveryKey);
    setRecoveryWarning(didSave ? null : "storage");
  }, [recoveryKey]);

  const saveNotes = useCallback(async (notify = true): Promise<boolean> => {
    let rebased = false;
    while (true) {
      if (composingRef.current || noteConflictRef.current || pageWriteFailed.current) return false;
      if (busy.current) {
        await new Promise<void>((resolve) => busyWaiters.current.push(resolve));
        if (activeWriteResult.current === false) return false;
        continue;
      }
      const value = draftRef.current;
      const current = baseline.current;
      if (value === null) return true;
      if (!current) return false;
      if (value === savedNotes.current) return true;
      if (utf8Bytes(value) > RECOVERY_LIMIT_BYTES) return false;
      busy.current = true;
      activeWriteResult.current = null;
      if (notify && mounted.current) setSaveState("saving");
      let writeSucceeded = false;
      try {
        const saved = await updateSourceAnnotation({
          source: "webpage", source_id: sourceId, ...(space === undefined ? {} : { space }),
          expected_version: current.version, notes: value,
        });
        baseline.current = saved;
        savedNotes.current = saved.notes;
        queryClient.setQueryData(annotationKey, saved);
        if (draftRef.current === value) {
          const storedDraft = readRecoveryDraft(recoveryKey);
          if (storedDraft?.draft === value) clearRecoveryDraft(recoveryKey);
          if (mounted.current && (!storedDraft || storedDraft.draft === value)) setRecoveryWarning(null);
        } else if (draftRef.current !== null && mounted.current) {
          persistDraft(draftRef.current, saved.notes, saved.version);
        }
        if (mounted.current) {
          setSaveState("saved");
          if (draftRef.current === value) setDraft(saved.notes);
        }
        writeSucceeded = true;
      } catch (error) {
        if (isConflict(error)) {
          try {
            const latest = await getSourceAnnotation("webpage", sourceId, space);
            if (latest.notes === current.notes && !rebased) {
              // Another annotation field changed. Rebase once, then drain the
              // still-current description through a fresh versioned write.
              rebased = true;
              baseline.current = latest;
              persistDraft(value, latest.notes, latest.version);
              queryClient.setQueryData(annotationKey, latest);
              if (mounted.current) setSaveState("saved");
              writeSucceeded = true;
            } else if (latest.notes !== current.notes) {
              baseline.current = latest;
              savedNotes.current = latest.notes;
              noteConflictRef.current = true;
              queryClient.setQueryData(annotationKey, latest);
              if (mounted.current) {
                setNoteConflict(true);
                setSaveState("conflict");
              }
            } else if (mounted.current) setSaveState("error");
          } catch {
            if (mounted.current) setSaveState("error");
          }
        } else if (mounted.current) setSaveState("error");
      } finally {
        activeWriteResult.current = writeSucceeded;
        busy.current = false;
        for (const resolve of busyWaiters.current.splice(0)) resolve();
      }
      if (!writeSucceeded) return false;
    }
  }, [sourceId, space, queryClient, recoveryKey, persistDraft]);
  flushRef.current = saveNotes;

  useEffect(() => {
    if (draft === null || !baseline.current || draft === savedNotes.current || composing || noteConflict || saveState === "error") return;
    const timer = window.setTimeout(() => { void flushRef.current(); }, 750);
    return () => window.clearTimeout(timer);
  }, [draft, composing, noteConflict, saveState, annotationQuery.data]);

  useEffect(() => {
    const onBeforeUnload = (event: BeforeUnloadEvent) => {
      if (draftRef.current !== null && savedNotes.current !== null && draftRef.current !== savedNotes.current) {
        event.preventDefault();
        event.returnValue = "";
      }
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  }, []);

  const pages = annotationQuery.data?.pages ?? [];
  const selectedIds = annotationQuery.data?.page_ids ?? [];
  const pageQuery = useInfiniteQuery({
    queryKey: ["sourceAnnotationPages", space ?? null],
    queryFn: ({ pageParam }) => listPages("active", space, 50, pageParam),
    initialPageParam: 0,
    getNextPageParam: (lastPage, allPages) => lastPage.length === 50 ? allPages.length * 50 : undefined,
    enabled: pickerOpen,
    retry: false,
  });
  const candidates: Page[] = (pageQuery.data?.pages.flat() ?? []).filter((page) => page.status === "active" && !page.entity_id);
  const visibleCandidates = candidates.filter((page) => page.title.toLocaleLowerCase().includes(pageFilter.trim().toLocaleLowerCase()));

  const togglePage = async (pageId: string, desiredMembership?: boolean) => {
    const current = baseline.current;
    if (!current || busy.current) return;
    busy.current = true;
    activeWriteResult.current = null;
    pageWriteFailed.current = false;
    setPageSaving(true);
    setPageSaveFailed(false);
    setPageRetry(null);
    const shouldAttach = desiredMembership ?? !current.page_ids.includes(pageId);
    const withDesiredMembership = (ids: string[]) => shouldAttach
      ? ids.includes(pageId) ? ids : [...ids, pageId]
      : ids.filter((id) => id !== pageId);
    try {
      if (current.page_ids.includes(pageId) === shouldAttach) {
        activeWriteResult.current = true;
        return;
      }
      const saved = await updateSourceAnnotation({
        source: "webpage", source_id: sourceId, ...(space === undefined ? {} : { space }),
        expected_version: current.version, page_ids: withDesiredMembership(current.page_ids),
      });
      baseline.current = saved;
      if (draftRef.current !== null && draftRef.current !== savedNotes.current && !noteConflictRef.current) {
        persistDraft(draftRef.current, savedNotes.current ?? "", saved.version);
      }
      queryClient.setQueryData(annotationKey, saved);
      pageWriteFailed.current = false;
      activeWriteResult.current = true;
      setPageSaveFailed(false);
      setPageRetry(null);
    } catch (error) {
      if (isConflict(error)) {
        // Preserve the requested attached/detached state, even if another
        // writer has already made the same change while this click was pending.
        try {
          const latest = await getSourceAnnotation("webpage", sourceId, space);
          const hasDirtyNotes = draftRef.current !== null && draftRef.current !== savedNotes.current;
          if (latest.notes !== current.notes) {
            baseline.current = latest;
            queryClient.setQueryData(annotationKey, latest);
            if (hasDirtyNotes) {
              savedNotes.current = latest.notes;
              noteConflictRef.current = true;
              setNoteConflict(true);
              setSaveState("conflict");
            } else {
              const adoptsDifferentCleanText = latest.notes !== savedNotes.current;
              savedNotes.current = latest.notes;
              draftRef.current = latest.notes;
              setDraft(latest.notes);
              if (adoptsDifferentCleanText) setEditorRevision((revision) => revision + 1);
            }
          } else {
            baseline.current = latest;
            if (hasDirtyNotes && !noteConflictRef.current) persistDraft(draftRef.current!, savedNotes.current ?? "", latest.version);
          }
          const latestIds = withDesiredMembership(latest.page_ids);
          if (latestIds.length === latest.page_ids.length && latestIds.every((id) => latest.page_ids.includes(id))) {
            baseline.current = latest;
            queryClient.setQueryData(annotationKey, latest);
          } else {
            const saved = await updateSourceAnnotation({
              source: "webpage", source_id: sourceId, ...(space === undefined ? {} : { space }),
              expected_version: latest.version, page_ids: latestIds,
            });
            baseline.current = saved;
            queryClient.setQueryData(annotationKey, saved);
          }
          setPageSaveFailed(false);
          setPageRetry(null);
          activeWriteResult.current = true;
        } catch {
          setPageSaveFailed(true);
          setPageRetry({ pageId, shouldAttach });
          pageWriteFailed.current = true;
          activeWriteResult.current = false;
          void annotationQuery.refetch();
        }
      } else {
        setPageSaveFailed(true);
        setPageRetry({ pageId, shouldAttach });
        pageWriteFailed.current = true;
        activeWriteResult.current = false;
      }
    } finally {
      if (activeWriteResult.current === null) activeWriteResult.current = false;
      busy.current = false;
      setPageSaving(false);
      for (const resolve of busyWaiters.current.splice(0)) resolve();
      if (draftRef.current !== savedNotes.current && !noteConflictRef.current) void flushRef.current();
    }
  };

  const resolveConflict = async (keepMine: boolean) => {
    try {
      if (!keepMine) {
        const latest = await getSourceAnnotation("webpage", sourceId, space);
        baseline.current = latest;
        savedNotes.current = latest.notes;
        noteConflictRef.current = false;
        setNoteConflict(false);
        queryClient.setQueryData(annotationKey, latest);
        draftRef.current = latest.notes;
        setDraft(latest.notes);
        setEditorRevision((revision) => revision + 1);
        clearRecoveryDraft(recoveryKey);
        setRecoveryWarning(null);
        setSaveState("saved");
        return;
      }
      const latest = await getSourceAnnotation("webpage", sourceId, space);
      baseline.current = latest;
      savedNotes.current = latest.notes;
      persistDraft(draftRef.current ?? "", latest.notes, latest.version);
      noteConflictRef.current = false;
      setNoteConflict(false);
      queryClient.setQueryData(annotationKey, latest);
      setSaveState("saved");
      await saveNotes();
    } catch {
      setSaveState("error");
    }
  };

  const statusPages = (loading: boolean) => (
    <section className="source-annotations-pages" aria-label={t("sourceAccess.relatedPages")}>
      <h3>{t("sourceAccess.relatedPages")}</h3>
      <p role={loading ? "status" : "alert"} className="source-annotations-muted">
        {loading ? t("sourceAccess.pagesLoading") : t("sourceAccess.pagesFailed")}
      </p>
    </section>
  );
  if (annotationQuery.isPending) return <>
    <section className="source-annotations" aria-label={t("sourceAccess.description")}><h2>{t("sourceAccess.description")}</h2><p role="status" className="source-annotations-muted">{t("sourceAccess.notesLoading")}</p></section>
    {renderRelatedPages ? renderRelatedPages(statusPages(true)) : statusPages(true)}
  </>;
  if (!annotationQuery.data) return <>
    <section className="source-annotations" aria-label={t("sourceAccess.description")}><h2>{t("sourceAccess.description")}</h2><p role="alert" className="source-annotations-muted">{t("sourceAccess.notesFailed")}</p><button type="button" className="source-annotations-text-button" onClick={() => void annotationQuery.refetch()}>{t("sourceAccess.retry")}</button></section>
    {renderRelatedPages ? renderRelatedPages(statusPages(false)) : statusPages(false)}
  </>;

  const descriptionDirty = draft !== null && savedNotes.current !== null && draft !== savedNotes.current;
  const saveMessage = saveState === "saving" ? t("sourceAccess.saving")
    : saveState === "error" ? t("sourceAccess.saveFailed")
      : noteConflict ? t("sourceAccess.noteConflict")
        : descriptionDirty ? t("sourceAccess.unsaved") : null;
  const relatedPages = (
    <section className="source-annotations-pages" aria-label={t("sourceAccess.relatedPages")}>
      <div className="source-annotations-heading-row">
        <h3>{t("sourceAccess.relatedPages")}</h3>
        <button type="button" className="source-annotations-text-button" disabled={pageSaving} onClick={() => setPickerOpen((open) => !open)}>{pickerOpen ? t("sourceAccess.closePicker") : t("sourceAccess.linkPage")}</button>
      </div>
      {pageSaveFailed && <p role="alert" className="source-annotations-page-error">{t("sourceAccess.pageSaveFailed")} {pageRetry && <button type="button" className="source-annotations-text-button" disabled={pageSaving} onClick={() => void togglePage(pageRetry.pageId, pageRetry.shouldAttach)}>{t("sourceAccess.retrySave")}</button>}</p>}
      {pages.length > 0 && <ul className="source-annotations-page-list">{pages.map((page) => <li key={page.id}><button type="button" onClick={() => navigation.onOpenPage?.(page.id)} disabled={!navigation.onOpenPage}><PageIcon size={16} /><span>{page.title}</span></button><button type="button" aria-label={t("sourceAccess.unlinkPage", { title: page.title })} disabled={pageSaving} onClick={() => void togglePage(page.id)}>{t("sourceAccess.removePage")}</button></li>)}</ul>}
      {pickerOpen && <div className="source-annotations-page-picker"><label><span>{t("sourceAccess.searchPages")}</span><input value={pageFilter} onChange={(event) => setPageFilter(event.target.value)} /></label>{pageQuery.isPending ? <p role="status">{t("sourceAccess.pagesLoading")}</p> : pageQuery.isError ? <p role="alert">{t("sourceAccess.pagesFailed")}</p> : visibleCandidates.length ? <ul>{visibleCandidates.map((page) => <li key={page.id}><label><input type="checkbox" checked={selectedIds.includes(page.id)} onChange={() => void togglePage(page.id)} disabled={pageSaving} /><PageIcon size={16} /><span>{page.title}</span></label></li>)}</ul> : <p>{t("sourceAccess.noPages")}</p>}{pageQuery.hasNextPage && <button type="button" className="source-annotations-text-button" disabled={pageQuery.isFetchingNextPage} onClick={() => void pageQuery.fetchNextPage()}>{pageQuery.isFetchingNextPage ? t("sourceAccess.pagesLoading") : t("sourceAccess.morePages")}</button>}</div>}
    </section>
  );

  return (
    <>
      <section className="source-annotations" aria-labelledby={`${placeholderId}-heading`}>
        <div className="source-annotations-heading-row">
          <h2 id={`${placeholderId}-heading`}>{t("sourceAccess.description")}</h2>
          {saveMessage ? <span className="source-annotations-save-state" role="status" aria-live="polite">{saveMessage}</span>
            : <span className="sr-only" role="status">{t("sourceAccess.saved")}</span>}
        </div>
        <div className="source-annotations-editor-wrap" onBlurCapture={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null) && !composing) void flushRef.current();
        }}>
          {!draft && <span id={`${placeholderId}-hint`} className="source-annotations-placeholder" aria-hidden="true">{t("sourceAccess.descriptionPlaceholder")}</span>}
          {draft !== null && <MarkdownEditor
            initialDocument={draft}
            sessionId={`${JSON.stringify([sourceId, space ?? null])}:${editorRevision}`}
            seamless
            disabled={false}
            ariaLabel={t("sourceAccess.descriptionLabel")}
            describedBy={!draft ? `${placeholderId}-hint` : undefined}
            onDocumentChange={(value) => {
              draftRef.current = value;
              setDraft(value);
              persistDraft(value);
              if (saveState === "error") setSaveState("saved");
            }}
            onStatusChange={(status) => {
              composingRef.current = status.compositionActive;
              setComposing(status.compositionActive);
            }}
            onSave={(value) => {
              draftRef.current = value;
              setDraft(value);
              persistDraft(value);
              void flushRef.current();
            }}
            onCancel={() => { void flushRef.current(); }}
            onFallback={() => {}}
          />}
        </div>
        {annotationQuery.isError && <p role="alert" className="source-annotations-muted">{t("sourceAccess.notesFailed")} <button type="button" className="source-annotations-text-button" onClick={() => void annotationQuery.refetch()}>{t("sourceAccess.retry")}</button></p>}
        {recoveryWarning && <p role="alert" className="source-annotations-muted">{recoveryWarning === "size" ? t("sourceAccess.recoveryTooLarge") : t("sourceAccess.recoveryUnavailable")}</p>}
        {saveState === "error" && <button type="button" className="source-annotations-text-button" onClick={() => void flushRef.current()}>{t("sourceAccess.retrySave")}</button>}
        {noteConflict && <div className="source-annotations-conflict" role="alert"><span>{t("sourceAccess.noteConflictBody")}</span><button type="button" onClick={() => void resolveConflict(true)}>{t("sourceAccess.keepMine")}</button><button type="button" onClick={() => void resolveConflict(false)}>{t("sourceAccess.useSaved")}</button></div>}
      </section>
      {renderRelatedPages ? renderRelatedPages(relatedPages) : relatedPages}
    </>
  );
}
