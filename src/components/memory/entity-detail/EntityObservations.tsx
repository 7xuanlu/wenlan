// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { useMutation } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { addObservation, deleteObservation, updateObservation, type Observation } from "../../../lib/tauri";
import { formatConfidence } from "./formatEntityMetadata";
import "./EntityObservations.css";

type EntityObservationsProps = {
  readonly entityId: string;
  readonly entityName: string;
  readonly observations: readonly Observation[];
  readonly onInvalidate: () => void;
  /** Archived topics are readable but do not accept edits. */
  readonly readOnly?: boolean;
  /** A topic-level lifecycle operation is in progress. */
  readonly blocked?: boolean;
  /** Registers the flush used by navigation and quit guards. */
  readonly onRegisterFlush?: (flush: (() => Promise<boolean>) | null) => void;
};

type EditedEntry = {
  readonly key: string;
  readonly observationId: string | null;
  readonly originalContent: string;
};

type DeleteState = { readonly id: string; readonly status: "pending" | "error" };

const SAVE_DELAY_MS = 600;
const MIN_NEW_OBSERVATION_CODE_POINTS = 5;

function unicodeLength(content: string) {
  return Array.from(content).length;
}

export function EntityObservations({
  entityId,
  entityName,
  observations: rawObservations,
  onInvalidate,
  readOnly = false,
  blocked = false,
  onRegisterFlush,
}: EntityObservationsProps) {
  const { t } = useTranslation();
  const [editor, setEditor] = useState<EditedEntry | null>(null);
  const [editContent, setEditContent] = useState("");
  const [isComposing, setIsComposing] = useState(false);
  const [status, setStatus] = useState<"saving" | "error" | "tooShort" | null>(null);
  const [deleteState, setDeleteState] = useState<DeleteState | null>(null);
  const [acknowledgedContent, setAcknowledgedContent] = useState<Record<string, string>>({});
  const [pendingAddedObservation, setPendingAddedObservation] = useState<Observation | null>(null);
  const [deletedObservationIds, setDeletedObservationIds] = useState<ReadonlySet<string>>(() => new Set());
  const editorRef = useRef<EditedEntry | null>(null);
  const draftRef = useRef("");
  const savedDraftRef = useRef("");
  const composingRef = useRef(false);
  const failedRef = useRef(false);
  const deleteStateRef = useRef<DeleteState | null>(null);
  const deletePromiseRef = useRef<Promise<boolean> | null>(null);
  const deletedObservationIdsRef = useRef(new Set<string>());
  const deleteFocusOriginRef = useRef<HTMLElement | null>(null);
  const sectionRef = useRef<HTMLElement | null>(null);
  const blockedRef = useRef(blocked);
  const readOnlyRef = useRef(readOnly);
  const flushPromiseRef = useRef<Promise<boolean> | null>(null);
  const flushCallbackRef = useRef<(explicitLeave?: boolean) => Promise<boolean>>(() => Promise.resolve(true));
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const debounceTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const switchTargetRef = useRef<string | null>(null);
  const addObservationIdRef = useRef<string | null>(null);
  const deleteOnLeaveRef = useRef<string | null>(null);
  blockedRef.current = blocked;
  readOnlyRef.current = readOnly;

  // Keep backend rows independent. Only rows deleted locally wait for refetch.
  const observations = useMemo(() => {
    if (!pendingAddedObservation || rawObservations.some((item) => item.id === pendingAddedObservation.id)) return rawObservations;
    return [...rawObservations, pendingAddedObservation];
  }, [rawObservations, pendingAddedObservation]);
  const visibleObservations = observations.filter((observation) =>
    !deletedObservationIds.has(observation.id) &&
    !(editor?.key === "new-observation" && editor.observationId === observation.id) &&
    (observation.content.trim().length > 0 || editor?.observationId === observation.id));
  const updateMutation = useMutation({
    mutationKey: ["entity-observations", entityId],
    mutationFn: ({ id, content }: { id: string; content: string }) => updateObservation(id, content),
    onSuccess: onInvalidate,
  });
  const addMutation = useMutation({
    mutationKey: ["entity-observations", entityId],
    mutationFn: (content: string) => addObservation(entityId, content, "human", 1),
    onSuccess: onInvalidate,
  });
  const deleteMutation = useMutation({
    mutationKey: ["entity-observations", entityId],
    mutationFn: (id: string) => deleteObservation(id),
    onSuccess: onInvalidate,
  });
  const updateMutateRef = useRef(updateMutation.mutateAsync);
  const addMutateRef = useRef(addMutation.mutateAsync);
  const deleteMutateRef = useRef(deleteMutation.mutateAsync);
  updateMutateRef.current = updateMutation.mutateAsync;
  addMutateRef.current = addMutation.mutateAsync;
  deleteMutateRef.current = deleteMutation.mutateAsync;

  const setCurrentDeleteState = (next: DeleteState | null) => {
    deleteStateRef.current = next;
    setDeleteState(next);
  };

  const restoreDeleteFocus = (observationId: string, retry: boolean) => {
    requestAnimationFrame(() => {
      const section = sectionRef.current;
      const active = document.activeElement as HTMLElement | null;
      const deletedRow = Array.from(section?.querySelectorAll<HTMLElement>("[data-observation-row-id]") ?? [])
        .find((row) => row.dataset.observationRowId === observationId);
      if (active && active !== document.body && active !== deleteFocusOriginRef.current && !deletedRow?.contains(active)) return;
      if (retry) {
        section?.querySelector<HTMLButtonElement>(".entity-obs-delete-retry")?.focus();
        return;
      }
      if (editorRef.current && editorRef.current.observationId !== observationId) {
        inputRef.current?.focus();
        return;
      }
      section?.querySelector<HTMLButtonElement>(".entity-obs-content:not(:disabled), .entity-obs-empty:not(:disabled), .entity-obs-add-trigger:not(:disabled)")?.focus();
    });
  };

  const setCurrentEditor = (next: EditedEntry | null) => {
    editorRef.current = next;
    setEditor(next);
  };

  const beginEdit = (entry: EditedEntry, content: string) => {
    if (readOnlyRef.current || blockedRef.current) return;
    editorRef.current = entry;
    draftRef.current = content;
    savedDraftRef.current = content;
    addObservationIdRef.current = entry.observationId;
    deleteOnLeaveRef.current = null;
    setEditor(entry);
    setEditContent(content);
    failedRef.current = false;
    setStatus(null);
  };

  const closeEditor = () => {
    setCurrentEditor(null);
    setStatus(null);
    setIsComposing(false);
    composingRef.current = false;
    addObservationIdRef.current = null;
  };

  const deleteEntry = (observationId: string): Promise<boolean> => {
    if (deletedObservationIdsRef.current.has(observationId)) return Promise.resolve(true);
    if (deletePromiseRef.current) return deletePromiseRef.current;
    if (readOnlyRef.current || blockedRef.current) return Promise.resolve(false);

    setCurrentDeleteState({ id: observationId, status: "pending" });
    const operation = (async () => {
      try {
        await deleteMutateRef.current(observationId);
        deletedObservationIdsRef.current = new Set([...deletedObservationIdsRef.current, observationId]);
        setDeletedObservationIds(deletedObservationIdsRef.current);
        setAcknowledgedContent((current) => {
          const next = { ...current };
          delete next[observationId];
          return next;
        });
        setPendingAddedObservation((current) => current?.id === observationId ? null : current);
        if (editorRef.current?.observationId === observationId) closeEditor();
        setCurrentDeleteState(null);
        restoreDeleteFocus(observationId, false);
        return true;
      } catch {
        setCurrentDeleteState({ id: observationId, status: "error" });
        restoreDeleteFocus(observationId, true);
        return false;
      } finally {
        deletePromiseRef.current = null;
      }
    })();
    deletePromiseRef.current = operation;
    return operation;
  };

  const flushCurrent = useCallback((explicitLeave = false): Promise<boolean> => {
    const deletion = deleteStateRef.current;
    if (deletion?.status === "pending") return deletePromiseRef.current ?? Promise.resolve(false);
    if (deletion?.status === "error") return Promise.resolve(false);
    const current = editorRef.current;
    if (!current) return Promise.resolve(true);
    if (readOnlyRef.current || blockedRef.current) return Promise.resolve(false);
    if (composingRef.current) return Promise.resolve(false);
    if (explicitLeave && current.observationId !== null && draftRef.current.trim().length === 0 &&
        (current.originalContent.trim().length > 0 || savedDraftRef.current.trim().length > 0)) {
      if (flushPromiseRef.current) {
        deleteOnLeaveRef.current = current.key;
        return flushPromiseRef.current;
      }
      return deleteEntry(current.observationId);
    }
    if (explicitLeave && current.observationId === null && addObservationIdRef.current === null &&
        draftRef.current.trim().length === 0) {
      if (flushPromiseRef.current) {
        deleteOnLeaveRef.current = current.key;
        return flushPromiseRef.current;
      }
      closeEditor();
      return Promise.resolve(true);
    }
    if (explicitLeave && draftRef.current.trim().length === 0) deleteOnLeaveRef.current = current.key;
    if (flushPromiseRef.current) return flushPromiseRef.current;
    // A failed write is retried only by an explicit retry or a changed draft.
    // Blur and navigation can fire separately during the same pointer gesture.
    if (failedRef.current) return Promise.resolve(false);

    const firstDraft = draftRef.current;
    if (current.observationId === null && firstDraft.trim().length === 0) {
      closeEditor();
      return Promise.resolve(true);
    }
    if (current.observationId === null && unicodeLength(firstDraft.trim()) < MIN_NEW_OBSERVATION_CODE_POINTS) {
      setStatus("tooShort");
      return Promise.resolve(false);
    }
    if (current.observationId !== null && firstDraft.trim().length === 0 &&
        (current.originalContent.trim().length > 0 || savedDraftRef.current.trim().length > 0)) {
      return explicitLeave || deleteOnLeaveRef.current === current.key
        ? deleteEntry(current.observationId)
        : Promise.resolve(true);
    }
    if (firstDraft === savedDraftRef.current) return Promise.resolve(true);

    const save = async (): Promise<boolean> => {
      setStatus("saving");
      let entryId = addObservationIdRef.current;
      try {
        while (true) {
          if (readOnlyRef.current || blockedRef.current) return false;
          if (composingRef.current) return false;
          const nextDraft = draftRef.current;
          if (current.observationId === null && !entryId && nextDraft.trim().length === 0) {
            savedDraftRef.current = "";
            break;
          }
          if (!entryId && unicodeLength(nextDraft.trim()) < MIN_NEW_OBSERVATION_CODE_POINTS) {
            setStatus("tooShort");
            return false;
          }
          if (entryId && nextDraft.trim().length === 0 &&
              (current.originalContent.trim().length > 0 || savedDraftRef.current.trim().length > 0)) {
            if (deleteOnLeaveRef.current === current.key) return await deleteEntry(entryId);
            break;
          }
          if (entryId && nextDraft === savedDraftRef.current) break;

          if (!entryId) {
            entryId = await addMutateRef.current(nextDraft);
            addObservationIdRef.current = entryId;
            setPendingAddedObservation({
              id: entryId, entity_id: entityId, content: nextDraft, source_agent: "human",
              confidence: 1, confirmed: false, created_at: Date.now() / 1000,
            });
            const active = editorRef.current;
            if (active?.key === current.key) {
              const persistedEntry = { ...active, observationId: entryId };
              editorRef.current = persistedEntry;
              setEditor(persistedEntry);
            }
          } else {
            await updateMutateRef.current({ id: entryId, content: nextDraft });
            setAcknowledgedContent((currentContent) => ({ ...currentContent, [entryId!]: nextDraft }));
          }
          savedDraftRef.current = nextDraft;
          if (draftRef.current === savedDraftRef.current) break;
        }
        setStatus(null);
        return true;
      } catch {
        failedRef.current = true;
        setStatus("error");
        return false;
      }
    };

    const promise = save().finally(() => {
      if (flushPromiseRef.current === promise) flushPromiseRef.current = null;
      setStatus((currentStatus) => currentStatus === "saving" ? null : currentStatus);
    });
    flushPromiseRef.current = promise;
    return promise;
  }, [entityId]);
  flushCallbackRef.current = flushCurrent;

  useEffect(() => {
    if (!onRegisterFlush) return;
    const flush = () => flushCallbackRef.current(true);
    onRegisterFlush(flush);
    return () => onRegisterFlush(null);
  }, [onRegisterFlush, entityId]);

  useEffect(() => {
    if (debounceTimerRef.current) clearTimeout(debounceTimerRef.current);
    if (!editor || readOnly || blocked || isComposing) return;
    if (editor.observationId !== null && editContent.trim().length === 0 &&
        (editor.originalContent.trim().length > 0 || savedDraftRef.current.trim().length > 0)) return;
    if (deleteState?.status === "pending" && editor.observationId === deleteState.id) return;
    if (editContent === savedDraftRef.current) return;
    debounceTimerRef.current = setTimeout(() => { void flushCallbackRef.current(); }, SAVE_DELAY_MS);
    return () => {
      if (debounceTimerRef.current) clearTimeout(debounceTimerRef.current);
    };
  }, [editor, editContent, isComposing, readOnly, blocked, deleteState]);

  useEffect(() => {
    setAcknowledgedContent((current) => {
      let changed = false;
      const next = { ...current };
      for (const observation of observations) {
        if (next[observation.id] === observation.content) {
          delete next[observation.id];
          changed = true;
        }
      }
      return changed ? next : current;
    });
    setPendingAddedObservation((current) => current && rawObservations.some((item) => item.id === current.id) ? null : current);
    const confirmedDeleted = [...deletedObservationIdsRef.current].filter((id) => !rawObservations.some((item) => item.id === id));
    if (confirmedDeleted.length > 0) {
      const next = new Set(deletedObservationIdsRef.current);
      confirmedDeleted.forEach((id) => next.delete(id));
      deletedObservationIdsRef.current = next;
      setDeletedObservationIds(next);
    }
  }, [observations, rawObservations]);

  useEffect(() => {
    if (!editor) return;
    const input = inputRef.current;
    if (!input) return;
    const resize = () => {
      input.style.height = "auto";
      input.style.height = `${input.scrollHeight}px`;
    };
    resize();
    let width = input.clientWidth;
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => {
      if (input.clientWidth === width) return;
      width = input.clientWidth;
      resize();
    });
    observer?.observe(input);
    return () => observer?.disconnect();
  }, [editor?.key]);

  const focusInput = () => requestAnimationFrame(() => inputRef.current?.focus());

  const leaveOnBlur = async (target: EventTarget | null) => {
    const targetElement = target instanceof HTMLElement ? target : null;
    if (deleteStateRef.current?.status === "pending" || targetElement?.dataset.observationRetry === "true" || targetElement?.dataset.observationDelete === "true") return;
    const targetId = targetElement?.dataset.observationId;
    const targetObservation = targetId ? observations.find((item) => item.id === targetId) : undefined;
    if (targetObservation) {
      requestRowSwitch(targetObservation, acknowledgedContent[targetObservation.id] ?? targetObservation.content);
      return;
    }
    const saved = await flushCallbackRef.current(true);
    if (!saved) {
      focusInput();
      return;
    }
    if (editorRef.current?.key === editor?.key && document.activeElement !== inputRef.current) closeEditor();
  };

  const requestRowSwitch = (observation: Observation, content: string) => {
    if (switchTargetRef.current === observation.id) return;
    switchTargetRef.current = observation.id;
    void flushCallbackRef.current(true).then((saved) => {
      if (saved && !readOnlyRef.current && !blockedRef.current) {
        beginEdit({ key: observation.id, observationId: observation.id, originalContent: content }, content);
      } else {
        focusInput();
      }
    }).finally(() => {
      if (switchTargetRef.current === observation.id) switchTargetRef.current = null;
    });
  };

  const onEntryClick = (observation: Observation, content: string) => {
    if (readOnly || blocked) return;
    if (!editorRef.current) {
      beginEdit({ key: observation.id, observationId: observation.id, originalContent: content }, content);
    } else if (editorRef.current.key !== observation.id) {
      requestRowSwitch(observation, content);
    }
  };

  const onNewEntryClick = () => {
    if (readOnly || blocked) return;
    if (!editorRef.current) beginEdit({ key: "new-observation", observationId: null, originalContent: "" }, "");
  };

  const onDraftChange = (value: string) => {
    failedRef.current = false;
    if (value.trim().length > 0) deleteOnLeaveRef.current = null;
    draftRef.current = value;
    setEditContent(value);
    if (status === "error" || status === "tooShort") setStatus(null);
    const editorEntry = editorRef.current;
    if (editorEntry?.key === "new-observation" && editorEntry.observationId === null && addObservationIdRef.current) {
      const persistedEntry = { ...editorEntry, observationId: addObservationIdRef.current };
      editorRef.current = persistedEntry;
      setEditor(persistedEntry);
    }
  };

  const onEscape = async (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.nativeEvent.isComposing || composingRef.current) return;
    if (event.key !== "Escape") return;
    event.preventDefault();
    event.stopPropagation();
    const saved = await flushCallbackRef.current(true);
    if (saved) closeEditor();
    else focusInput();
  };

  const requestDelete = async (observationId: string) => {
    if (readOnlyRef.current || blockedRef.current) return;
    const activeEditor = editorRef.current;
    if (activeEditor?.observationId === observationId) {
      if (debounceTimerRef.current) {
        clearTimeout(debounceTimerRef.current);
        debounceTimerRef.current = null;
      }
      // Deleting this same row is an explicit discard of its unsaved draft.
      // Wait for an in-flight write to settle, but a failed write must not
      // suppress the user's delete action.
      if (flushPromiseRef.current) await flushPromiseRef.current;
    } else if (activeEditor) {
      const saved = await flushCallbackRef.current(true);
      if (!saved) return;
    } else if (flushPromiseRef.current && !await flushPromiseRef.current) {
      return;
    }
    if (!deletedObservationIdsRef.current.has(observationId)) await deleteEntry(observationId);
  };

  const keepInformation = () => {
    const deletion = deleteStateRef.current;
    if (deletion?.status !== "error") return;
    setCurrentDeleteState(null);
    failedRef.current = false;
    if (editorRef.current?.observationId === deletion.id && draftRef.current.trim().length === 0) {
      const restored = savedDraftRef.current.trim().length > 0
        ? savedDraftRef.current
        : editorRef.current.originalContent;
      draftRef.current = restored;
      savedDraftRef.current = restored;
      setEditContent(restored);
      setStatus(null);
      focusInput();
    }
  };

  const hasError = deleteState === null && (status === "error" || status === "tooShort");
  const placeholder = t("entityDetail.notePlaceholder", { name: entityName });

  return (
    <section ref={sectionRef} className="memory-detail-card entity-about" aria-labelledby="entity-about-title">
      <div className="memory-detail-card-header">
        <h2 id="entity-about-title" className="memory-detail-section-title">{t("entityDetail.aboutTitle")}</h2>
      </div>
      <div className="memory-detail-card-body">
        {hasError && <p className="entity-error" role="alert">{t(status === "tooShort" ? "entityDetail.observationTooShort" : "entityDetail.saveError")}</p>}
        {deleteState?.status === "error" && <p className="entity-error" role="alert">{t("entityDetail.deleteError")}</p>}
        {visibleObservations.length === 0 && !editor && (
          readOnly ? <p className="entity-empty">{t("entityDetail.emptyAbout")}</p> :
            <button type="button" className="entity-obs-empty entity-obs-add-trigger" disabled={blocked || deleteState !== null} onClick={onNewEntryClick}>
              {t("entityDetail.addNote")}
            </button>
        )}
        <ul className="entity-obs-list" role="list">
          {visibleObservations.map((observation) => {
            const confidence = formatConfidence(observation.confidence, 1);
            const active = editor?.key === observation.id;
            const content = acknowledgedContent[observation.id] ?? observation.content;
            return (
              <li key={observation.id} className="entity-obs-row" data-observation-row-id={observation.id}>
                <div className="entity-obs-entry">
                  {active ? (
                    <textarea
                      ref={inputRef}
                      className="entity-obs-input"
                      rows={1}
                      value={editContent}
                      autoFocus
                      disabled={blocked || deleteState !== null}
                      aria-label={t("entityDetail.editNote")}
                      placeholder={placeholder}
                      onChange={(event) => {
                        onDraftChange(event.target.value);
                        event.currentTarget.style.height = "auto";
                        event.currentTarget.style.height = `${event.currentTarget.scrollHeight}px`;
                      }}
                      onBlur={(event) => { void leaveOnBlur(event.relatedTarget); }}
                      onCompositionStart={() => {
                        composingRef.current = true;
                        setIsComposing(true);
                        if (debounceTimerRef.current) clearTimeout(debounceTimerRef.current);
                      }}
                      onCompositionEnd={() => {
                        composingRef.current = false;
                        setIsComposing(false);
                      }}
                      onKeyDown={(event) => { void onEscape(event); }}
                    />
                  ) : readOnly ? (
                  <p className="entity-obs-content" title={confidence ? `${t("entityDetail.confidenceLabel")}: ${confidence}` : undefined}>{content || placeholder}</p>
                  ) : (
                  <button
                    type="button"
                    className="entity-obs-content"
                    data-observation-id={observation.id}
                    aria-label={`${t("entityDetail.editNote")} — ${content || placeholder}`}
                    disabled={blocked || deleteState !== null}
                    onPointerDown={(event) => { if (editorRef.current && editorRef.current.key !== observation.id) event.preventDefault(); }}
                    onClick={() => onEntryClick(observation, content)}
                    title={confidence ? `${t("entityDetail.confidenceLabel")}: ${confidence}` : undefined}
                  >{content || placeholder}</button>
                  )}
                  {!readOnly && (
                    <button
                      type="button"
                      className="entity-obs-delete"
                      aria-label={`${t("entityDetail.deleteNote")} — ${content || placeholder}`}
                      title={t("entityDetail.deleteNote")}
                      data-observation-delete="true"
                      data-observation-delete-id={observation.id}
                      disabled={blocked || deleteState !== null}
                      onPointerDown={(event) => { if (editorRef.current) event.preventDefault(); }}
                      onClick={() => { deleteFocusOriginRef.current = document.activeElement as HTMLElement | null; void requestDelete(observation.id); }}
                    >
                      <svg aria-hidden="true" viewBox="0 0 20 20" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                        <path d="M3.5 5.5h13M7 5.5V4h6v1.5m-8 0 .7 10.5h8.6L15 5.5M8 8.5v5m4-5v5" />
                      </svg>
                    </button>
                  )}
                </div>
              </li>
            );
          })}
          {editor?.key === "new-observation" && (
            <li className="entity-obs-row">
              <div className="entity-obs-entry entity-obs-editor">
                <textarea
                  ref={inputRef}
                  className="entity-obs-input"
                  rows={1}
                  value={editContent}
                  autoFocus
                  disabled={blocked || deleteState !== null}
                  aria-label={t("entityDetail.addNote")}
                  placeholder={placeholder}
                  onChange={(event) => {
                    onDraftChange(event.target.value);
                    event.currentTarget.style.height = "auto";
                    event.currentTarget.style.height = `${event.currentTarget.scrollHeight}px`;
                  }}
                  onBlur={(event) => { void leaveOnBlur(event.relatedTarget); }}
                  onCompositionStart={() => {
                    composingRef.current = true;
                    setIsComposing(true);
                    if (debounceTimerRef.current) clearTimeout(debounceTimerRef.current);
                  }}
                  onCompositionEnd={() => {
                    composingRef.current = false;
                    setIsComposing(false);
                  }}
                  onKeyDown={(event) => { void onEscape(event); }}
                />
                {editor.observationId && (
                  <button
                    type="button"
                    className="entity-obs-delete"
                    aria-label={`${t("entityDetail.deleteNote")} — ${editContent || placeholder}`}
                    title={t("entityDetail.deleteNote")}
                    data-observation-delete="true"
                    data-observation-delete-id={editor.observationId}
                    disabled={blocked || deleteState !== null}
                    onPointerDown={(event) => event.preventDefault()}
                    onClick={() => { deleteFocusOriginRef.current = document.activeElement as HTMLElement | null; void requestDelete(editor.observationId!); }}
                  >
                    <svg aria-hidden="true" viewBox="0 0 20 20" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                      <path d="M3.5 5.5h13M7 5.5V4h6v1.5m-8 0 .7 10.5h8.6L15 5.5M8 8.5v5m4-5v5" />
                    </svg>
                  </button>
                )}
              </div>
            </li>
          )}
        </ul>
        {!readOnly && !editor && visibleObservations.length > 0 && (
          <button type="button" className="memory-detail-text-button entity-obs-add-trigger" disabled={blocked || deleteState !== null} onClick={onNewEntryClick}>
            <span aria-hidden="true">＋ </span>{t("entityDetail.addNote")}
          </button>
        )}
        {status === "saving" && deleteState === null && <span className="entity-obs-saving" role="status">{t("entityDetail.savingObservation")}</span>}
        {status === "error" && deleteState === null && (
          <button type="button" className="entity-obs-retry" data-observation-retry="true" onPointerDown={(event) => event.preventDefault()} onClick={() => { failedRef.current = false; void flushCallbackRef.current(true).then(() => { focusInput(); }); }}>
            {t("entityDetail.retryObservationSave")}
          </button>
        )}
        {deleteState?.status === "error" && (
          <div className="entity-obs-delete-recovery">
            <button type="button" className="entity-obs-retry entity-obs-delete-retry" data-observation-retry="true" onPointerDown={(event) => event.preventDefault()} onClick={() => { deleteFocusOriginRef.current = document.activeElement as HTMLElement | null; void deleteEntry(deleteState.id); }}>
              {t("entityDetail.retryDelete")}
            </button>
            <button type="button" className="entity-obs-retry entity-obs-keep" data-observation-retry="true" onPointerDown={(event) => event.preventDefault()} onClick={keepInformation}>
              {t("entityDetail.keepInformation")}
            </button>
          </div>
        )}
      </div>
    </section>
  );
}
