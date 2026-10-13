// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useId, useMemo, useRef, useState, type ReactNode } from "react";
import { CaretDown, Folder, FolderPlus } from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import type { Page } from "../../../lib/tauri";
import { readWikiInventoryMode, writeWikiInventoryMode, type WikiInventoryMode } from "../../../lib/wikiNotesPreferences";
import { PageInventoryPanel } from "./PageInventoryPanel";
import { RecentNotesPanel } from "./RecentNotesPanel";
import { WikiFolderCreationContext } from "./WikiFolderCreationContext";
import type { WikiInventoryScope } from "./pageInventory";
import { NoteGroupFrame } from "./NoteGroupFrame";
import "./wikiWorkspace.css";

/** The docked list's border-box width plus its inset; see wikiWorkspace.css. */
const NOTE_LIST_WIDTH = 248;
/** With Info open, the list stays docked only while the note keeps this much room. */
const MIN_NOTE_LANE = 680;
/** Matches the wiki-main container query that turns the list into a dropdown. */
const NARROW_WORKSPACE_WIDTH = 900;

export interface WikiWorkspaceProps {
  readonly children: ReactNode;
  readonly tabs?: ReactNode;
  readonly onGlobalToolsHost?: (host: HTMLDivElement | null) => void;
  readonly onGroupFocus?: () => void;
  readonly secondaryOpen?: boolean;
  readonly onSecondaryHost?: (host: HTMLDivElement | null) => void;
  readonly inventoryScope?: WikiInventoryScope;
  readonly currentPageId?: string | null;
  readonly currentFolderPath?: string | null;
  readonly inventoryMode?: WikiInventoryMode;
  readonly onInventoryModeChange?: (mode: WikiInventoryMode) => void;
  readonly recentRevision?: number;
  readonly onOpenRecentPage?: (id: string) => void;
  readonly browsing?: boolean;
  readonly fullBleed?: boolean;
  readonly filter?: string;
  readonly onFilterChange?: (filter: string) => void;
  readonly onBrowse: (scope: WikiInventoryScope) => void;
  readonly onCreatePage: (folderPath?: string) => void;
  readonly onCreationFolderPathChange?: (path: string | null) => void;
  readonly onOpenDraft: (draftId: string, space: string | null) => void;
  readonly onOpenPage: (page: Page) => void;
  /** Increments to bring back a list that gave way to Info, closing Info. */
  readonly noteListRestoreRequest?: number;
  readonly onNoteListYieldedChange?: (yielded: boolean) => void;
}

export function WikiWorkspace({
  children,
  tabs,
  onGroupFocus,
  onGlobalToolsHost,
  secondaryOpen,
  onSecondaryHost,
  inventoryScope = "all",
  currentPageId = null,
  currentFolderPath,
  inventoryMode: controlledInventoryMode,
  onInventoryModeChange,
  recentRevision = 0,
  onOpenRecentPage,
  browsing = false,
  fullBleed = false,
  filter,
  onFilterChange,
  onBrowse,
  onCreatePage,
  onCreationFolderPathChange,
  onOpenDraft,
  onOpenPage,
  noteListRestoreRequest = 0,
  onNoteListYieldedChange,
}: WikiWorkspaceProps) {
  const { t } = useTranslation();
  const [folderOpen, setFolderOpen] = useState(false);
  // The list and Info share the room beside the note. When both would leave
  // the note narrower than MIN_NOTE_LANE, Info (the one just opened) keeps its
  // place and the list gives way until Info closes or the window has room.
  const [listYielded, setListYielded] = useState(false);
  const listYieldedRef = useRef(listYielded);
  listYieldedRef.current = listYielded;
  const workspaceRef = useRef<HTMLDivElement>(null);
  const inspectorCloser = useRef<(() => void) | null>(null);
  useEffect(() => {
    const root = workspaceRef.current;
    if (!root) return;
    let frame = 0;
    const measure = () => {
      frame = 0;
      const width = root.getBoundingClientRect().width;
      const primary = root.querySelector<HTMLElement>(".wiki-primary-note-group");
      const lane = primary?.querySelector<HTMLElement>(".note-group-content")?.getBoundingClientRect().width ?? 0;
      const infoOpen = Boolean(primary?.querySelector(".page-info-drawer-group-overlay"));
      // Two note groups side by side were chosen narrower; the list stays put.
      const split = Boolean(root.querySelector(".wiki-workspace-reading.is-split"));
      const besideList = listYieldedRef.current ? lane - NOTE_LIST_WIDTH : lane;
      const yielded = width > NARROW_WORKSPACE_WIDTH && !split && infoOpen && lane > 0 && besideList < MIN_NOTE_LANE;
      setListYielded(yielded);
    };
    const schedule = () => { if (!frame) frame = requestAnimationFrame(measure); };
    measure();
    const resize = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(schedule);
    resize?.observe(root);
    // Info opens, closes and resizes inside the note group, not on the root.
    const mutations = typeof MutationObserver === "undefined" ? null : new MutationObserver(schedule);
    mutations?.observe(root, { childList: true, subtree: true, attributes: true, attributeFilter: ["class", "style"] });
    return () => {
      if (frame) cancelAnimationFrame(frame);
      resize?.disconnect();
      mutations?.disconnect();
    };
  }, []);
  useEffect(() => { onNoteListYieldedChange?.(listYielded); }, [listYielded, onNoteListYieldedChange]);
  const handledRestoreRef = useRef(noteListRestoreRequest);
  useEffect(() => {
    if (handledRestoreRef.current === noteListRestoreRequest) return;
    handledRestoreRef.current = noteListRestoreRequest;
    inspectorCloser.current?.();
  }, [noteListRestoreRequest]);
  const [localInventoryMode, setLocalInventoryMode] = useState<WikiInventoryMode>(readWikiInventoryMode);
  const inventoryMode = controlledInventoryMode ?? localInventoryMode;
  const [creationFolderPath, reportCreationFolderPath] = useState<string | null>(null);
  useEffect(() => {
    onCreationFolderPathChange?.(inventoryMode === "folders" ? creationFolderPath : null);
  }, [creationFolderPath, inventoryMode, onCreationFolderPathChange]);
  const [canCreateFolder, setCanCreateFolder] = useState(false);
  const createFormRef = useRef<(() => void) | null>(null);
  const id = useId();
  const reportCanCreateFolder = useCallback((available: boolean) => {
    setCanCreateFolder(current => current === available ? current : available);
  }, []);
  const registerCreateFolderForm = useCallback((openForm: () => void) => {
    createFormRef.current = openForm;
    return () => {
      if (createFormRef.current !== openForm) return;
      createFormRef.current = null;
      reportCanCreateFolder(false);
    };
  }, [reportCanCreateFolder]);
  const requestCreateFolder = useCallback(() => {
    if (inventoryMode !== "folders" || !canCreateFolder || !createFormRef.current) return false;
    setFolderOpen(true);
    createFormRef.current();
    return true;
  }, [canCreateFolder, inventoryMode]);
  const folderCreationContext = useMemo(() => ({
    creationFolderPath: inventoryMode === "folders" ? creationFolderPath : null,
    reportCreationFolderPath,
    canCreateFolder: inventoryMode === "folders" && canCreateFolder,
    reportCanCreateFolder,
    registerCreateFolderForm,
    requestCreateFolder,
  }), [creationFolderPath, inventoryMode, canCreateFolder, reportCanCreateFolder, registerCreateFolderForm, requestCreateFolder]);
  const isManualOrder = inventoryMode === "custom";
  const changeInventoryMode = (mode: WikiInventoryMode) => {
    writeWikiInventoryMode(mode);
    setLocalInventoryMode(mode);
    onInventoryModeChange?.(mode);
  };
  const closeForNavigation = <Arguments extends readonly unknown[]>(callback: (...args: Arguments) => void) => (...args: Arguments) => {
    setFolderOpen(false);
    callback(...args);
  };

  return (
    <WikiFolderCreationContext.Provider value={folderCreationContext}>
    <div className="wiki-workspace" ref={workspaceRef}>
      <button
        type="button"
        className="wiki-workspace-folders-toggle"
        aria-expanded={folderOpen}
        aria-controls={`${id}-directory`}
        onClick={() => setFolderOpen(open => !open)}
      >
        <Folder aria-hidden="true" size={16} />
        <span>{t("sidebar.notes")}</span>
      </button>
      <div className={`wiki-workspace-body${folderOpen ? " is-folder-open" : ""}${listYielded ? " is-list-yielded" : ""}`}>
        <aside id={`${id}-directory`} className="wiki-workspace-directory">
          <div className="wiki-workspace-mode-row">
            <label className="wiki-workspace-mode">
              <span>{t("pages.recent.modeLabel")}</span>
              <select
                aria-label={t("pages.recent.modeLabel")}
                title={isManualOrder ? t("pages.recent.customHint") : undefined}
                value={inventoryMode}
                onChange={event => changeInventoryMode(event.target.value as WikiInventoryMode)}
              >
                <option value="recent">{t("pages.recent.title")}</option>
                <option value="custom">{t("pages.recent.custom")}</option>
                <option value="folders">{t("pages.folders.title")}</option>
              </select>
              <CaretDown aria-hidden="true" size={16} />
            </label>
            {inventoryMode === "folders" && (
              <button
                type="button"
                className="wiki-workspace-new-folder"
                aria-label={t("pages.folders.newFolder")}
                title={t("pages.folders.newFolder")}
                disabled={!canCreateFolder}
                onClick={event => { event.currentTarget.focus(); requestCreateFolder(); }}
              >
                <FolderPlus aria-hidden="true" size={16} />
              </button>
            )}
          </div>
          {inventoryMode !== "folders" ? (
            <RecentNotesPanel
              mode={inventoryMode}
              currentPageId={currentPageId}
              revision={recentRevision}
              onOpenPage={onOpenRecentPage ? closeForNavigation(onOpenRecentPage) : undefined}
            />
          ) : (
            <PageInventoryPanel
              inventoryScope={inventoryScope}
              currentPageId={currentPageId}
              currentFolderPath={creationFolderPath ?? currentFolderPath}
              fileTree
              browsing={browsing}
              filter={filter}
              onFilterChange={onFilterChange}
              onBrowse={closeForNavigation(onBrowse)}
              onCreatePage={closeForNavigation(onCreatePage)}
              onOpenDraft={closeForNavigation(onOpenDraft)}
              onOpenPage={closeForNavigation(onOpenPage)}
            />
          )}
        </aside>
        <div className={`wiki-workspace-reading${secondaryOpen ? " is-split" : ""}`}>
          <div className={`wiki-primary-note-group${fullBleed ? " is-full-bleed" : ""}`}>
            <NoteGroupFrame id="primary" label={t("pages.groups.central")} tabs={tabs} onGlobalToolsHost={onGlobalToolsHost} contentId="wiki-note-content" onFocus={onGroupFocus} inspectorCloser={inspectorCloser}>
              {children}
            </NoteGroupFrame>
          </div>
          <div className="wiki-secondary-note-group" ref={onSecondaryHost} hidden={!secondaryOpen} />
        </div>
      </div>
    </div>
    </WikiFolderCreationContext.Provider>
  );
}
