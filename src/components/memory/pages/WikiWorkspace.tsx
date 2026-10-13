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
/** Info's default width beside the note. */
const INFO_WIDTH = 320;
/** The note keeps at least this much room beside the list and Info. */
const MIN_NOTE_LANE = 600;
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
}: WikiWorkspaceProps) {
  const { t } = useTranslation();
  const id = useId();
  const [folderOpen, setFolderOpen] = useState(false);
  // When the window is too narrow for the list, Info and a comfortable note
  // side by side, the list starts hidden and a Notes button at the start of
  // the tab strip docks it on request. Opening or closing Info never changes it.
  const [listCrowded, setListCrowded] = useState(false);
  const [listRequested, setListRequested] = useState(false);
  const workspaceRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const root = workspaceRef.current;
    if (!root) return;
    const measure = () => {
      const width = root.getBoundingClientRect().width;
      const crowded = width > NARROW_WORKSPACE_WIDTH && width - NOTE_LIST_WIDTH - INFO_WIDTH < MIN_NOTE_LANE;
      setListCrowded(crowded);
      if (!crowded) setListRequested(false);
    };
    measure();
    const resize = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    resize?.observe(root);
    return () => resize?.disconnect();
  }, []);
  // Two note groups side by side were chosen narrower; the list stays put.
  const listHidden = listCrowded && !listRequested && !secondaryOpen;
  const listToggle = listCrowded && !secondaryOpen ? (
    <button
      type="button"
      className="wiki-workspace-list-toggle"
      aria-controls={`${id}-directory`}
      aria-expanded={!listHidden}
      aria-label={t("sidebar.notes")}
      title={t("sidebar.notes")}
      onClick={() => setListRequested(requested => !requested)}
    >
      <Folder aria-hidden="true" size={16} />
    </button>
  ) : null;
  const [localInventoryMode, setLocalInventoryMode] = useState<WikiInventoryMode>(readWikiInventoryMode);
  const inventoryMode = controlledInventoryMode ?? localInventoryMode;
  const [creationFolderPath, reportCreationFolderPath] = useState<string | null>(null);
  useEffect(() => {
    onCreationFolderPathChange?.(inventoryMode === "folders" ? creationFolderPath : null);
  }, [creationFolderPath, inventoryMode, onCreationFolderPathChange]);
  const [canCreateFolder, setCanCreateFolder] = useState(false);
  const createFormRef = useRef<(() => void) | null>(null);
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
      <div className={`wiki-workspace-body${folderOpen ? " is-folder-open" : ""}${listHidden ? " is-list-yielded" : ""}`}>
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
            <NoteGroupFrame id="primary" label={t("pages.groups.central")} tabs={listToggle ? <>{listToggle}{tabs}</> : tabs} onGlobalToolsHost={onGlobalToolsHost} contentId="wiki-note-content" onFocus={onGroupFocus}>
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
