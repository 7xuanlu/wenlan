// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useId, useMemo, useState } from "react";
import { CaretRight, FileText, Folder, FolderPlus, MagnifyingGlass, Plus, Stack } from "@phosphor-icons/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { knowledgeFolderCreate, type Page, type KnowledgeFolder } from "../../../lib/tauri";
import { listAllActivePages, listAllDraftPages, listAllActivePagesExplicitBrowse, listAllDraftPagesExplicitBrowse, EXPLICIT_BROWSE_QUERY_POLICY } from "./listAllPages";
import { pageSpaceContext } from "./pagePresentation";
import { collectPageInventory, filterPageInventory, folderScope, inventoryFolderPath, pageFolderPath, pageMatchesInventoryScope, type WikiInventoryScope } from "./pageInventory";
import { KNOWLEDGE_FOLDERS_QUERY_KEY, useKnowledgeFolders } from "./useKnowledgeFolders";
import "./pageInventory.css";

type PageInventoryPanelProps = {
  readonly currentPageId?: string | null;
  readonly inventoryScope?: WikiInventoryScope;
  readonly browsing?: boolean;
  readonly onBrowse?: (scope: WikiInventoryScope) => void;
  readonly onCreatePage?: (folderPath?: string) => void;
  readonly onOpenDraft?: (draftId: string, space: string | null) => void;
  readonly onOpenPage?: (page: Page) => void;
};

export function PageInventoryPanel({ currentPageId = null, inventoryScope = "all", browsing = false, onBrowse, onCreatePage, onOpenDraft, onOpenPage }: PageInventoryPanelProps) {
  const { i18n, t } = useTranslation();
  const id = useId();
  const queryClient = useQueryClient();
  const [filter, setFilter] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set([""]));
  const [creating, setCreating] = useState(false);
  const [name, setName] = useState("");
  const [createPending, setCreatePending] = useState(false);
  const [createError, setCreateError] = useState(false);
  const active = useQuery({ queryKey: browsing ? ["pages", "active"] : ["pages", "inventory", "passive", "active"], queryFn: browsing ? listAllActivePagesExplicitBrowse : listAllActivePages, ...(browsing ? EXPLICIT_BROWSE_QUERY_POLICY : { staleTime: 30_000 }) });
  const drafts = useQuery({ queryKey: browsing ? ["pages", "draft"] : ["pages", "inventory", "passive", "draft"], queryFn: browsing ? listAllDraftPagesExplicitBrowse : listAllDraftPages, ...(browsing ? EXPLICIT_BROWSE_QUERY_POLICY : { staleTime: 30_000 }) });
  const folders = useKnowledgeFolders();
  const pages = useMemo(() => collectPageInventory(active.data ?? [], drafts.data ?? []), [active.data, drafts.data]);
  const selectedPage = pages.find(page => page.id === currentPageId);
  const selectedPath = selectedPage ? pageFolderPath(selectedPage) : inventoryFolderPath(inventoryScope);
  useEffect(() => {
    if (selectedPath === null) return;
    setExpanded(previous => {
      const next = new Set(previous); next.add("");
      const parts = selectedPath.split("/");
      for (let index = 1; index <= parts.length; index++) next.add(parts.slice(0, index).join("/"));
      return next;
    });
  }, [selectedPath]);
  const hasFilter = !!filter.trim();
  const canCreate = !!folders.data && !folders.data.truncated;
  const folderUnavailable = folders.isError || !!folders.data?.truncated;
  const renderPage = (page: Page) => {
    const draft = page.status === "draft";
    const title = draft && !page.title.trim() ? t("pages.overview.untitledDraft") : page.title;
    return <li key={page.id}><button type="button" className="notes-page-button notes-inventory-page" aria-label={t("pages.overview.openPage", { title })} aria-current={currentPageId === page.id ? "page" : undefined} data-active={currentPageId === page.id ? "true" : undefined} title={title} disabled={draft ? !onOpenDraft : !onOpenPage} onClick={() => draft ? onOpenDraft?.(page.id, pageSpaceContext(page) ?? null) : onOpenPage?.(page)}><FileText aria-hidden="true" size={14}/><span className="notes-page-title">{title}</span></button></li>;
  };
  const renderFolder = (folder: KnowledgeFolder, depth: number): React.ReactNode => {
    const children = (folders.data?.folders ?? []).filter(item => item.path !== folder.path && item.parent_path === folder.path);
    const noteChildren = !browsing ? pages.filter(page => pageMatchesInventoryScope(page, folderScope(folder.path))) : [];
    const open = expanded.has(folder.path);
    const listId = `${id}-${folder.path}`;
    return <li key={folder.path}>
      <div className="notes-inventory-collection-row" style={{ paddingLeft: depth * 14 }}>
        <button type="button" className="notes-inventory-disclosure" aria-label={t(open ? "pages.inventory.collapse" : "pages.inventory.expand", { name: folder.name })} aria-expanded={open} aria-controls={listId} disabled={!children.length && !noteChildren.length} onClick={() => setExpanded(previous => { const next = new Set(previous); if (next.has(folder.path)) next.delete(folder.path); else next.add(folder.path); return next; })}><CaretRight aria-hidden="true" size={12}/></button>
        <button type="button" className="notes-inventory-scope" data-active={selectedPath === folder.path ? "true" : undefined} aria-current={selectedPath === folder.path ? "page" : undefined} onClick={() => { setFilter(""); onBrowse?.(folderScope(folder.path)); }} disabled={!onBrowse}><Folder aria-hidden="true" size={16}/><span>{folder.name}</span></button>
      </div>
      {open && <ul id={listId} className="notes-inventory-collections">{children.map(child => renderFolder(child, depth + 1))}{noteChildren.length > 0 && <li><ul className="notes-page-list notes-inventory-pages" style={{ paddingLeft: depth * 14 }}>{noteChildren.map(renderPage)}</ul></li>}</ul>}
    </li>;
  };
  const create = async (event: React.FormEvent) => {
    event.preventDefault(); if (!name.trim() || createPending || !canCreate) return;
    setCreatePending(true); setCreateError(false);
    try {
      const parentPath = selectedPath ?? "";
      const result = await knowledgeFolderCreate(parentPath, name.trim());
      queryClient.setQueryData(KNOWLEDGE_FOLDERS_QUERY_KEY, { folders: [...(folders.data?.folders ?? []), { path: result.path, parent_path: parentPath, name: name.trim() }], truncated: false });
      setCreating(false); setName(""); onBrowse?.(folderScope(result.path));
      void queryClient.invalidateQueries({ queryKey: KNOWLEDGE_FOLDERS_QUERY_KEY });
    } catch { setCreateError(true); } finally { setCreatePending(false); }
  };
  return <section aria-label={t("sidebar.notes")} className="notes-list-panel notes-inventory-panel">
    <div className="notes-list-header"><h2>{t("sidebar.notes")}</h2><div className="flex items-center gap-1">
      {onCreatePage && <button type="button" className="notes-list-create" aria-label={t("sidebar.newNote")} title={t("sidebar.newNote")} onClick={() => { if (selectedPath === null) onCreatePage(); else onCreatePage(selectedPath); }}><Plus aria-hidden="true" size={16}/></button>}
      <button type="button" className="notes-list-create" aria-label={t("pages.folders.newFolder")} title={t("pages.folders.newFolder")} disabled={!canCreate} onClick={() => { setCreating(value => !value); setCreateError(false); }}><FolderPlus aria-hidden="true" size={16}/></button>
    </div></div>
    {creating && <form className="notes-folder-create" onSubmit={create}><label htmlFor={`${id}-name`}>{t("pages.folders.folderName")}</label><input id={`${id}-name`} autoFocus value={name} disabled={createPending} maxLength={100} onChange={event => setName(event.target.value)} /><span className="notes-folder-parent">{t("pages.folders.createIn", { name: selectedPath || t("pages.folders.root") })}</span><div><button type="submit" disabled={createPending || !name.trim()}>{t("pages.folders.create")}</button><button type="button" onClick={() => setCreating(false)} disabled={createPending}>{t("pages.folders.cancel")}</button></div>{createError && <p role="alert">{t("pages.folders.createError")}</p>}</form>}
    <label className="notes-list-filter"><span className="sr-only">{t("pages.folders.searchAll")}</span><MagnifyingGlass aria-hidden="true" size={14}/><input aria-label={t("pages.folders.searchAll")} placeholder={t("pages.folders.searchAll")} onChange={event => setFilter(event.target.value)} type="search" value={filter}/></label>
    <div className="notes-list-scroll">
      {active.isPending || drafts.isPending ? <p className="notes-list-state" role="status">{t("pages.overview.loading")}</p> : active.isError || drafts.isError ? <div className="notes-list-state" role="alert"><p>{t("pages.overview.error")}</p><button type="button" onClick={() => { void active.refetch(); void drafts.refetch(); }}>{t("pageDetail.retry")}</button></div> : <>
        <button type="button" className="notes-inventory-scope notes-inventory-all" aria-label={t("pages.inventory.all")} data-active={selectedPath === null ? "true" : undefined} aria-current={selectedPath === null ? "page" : undefined} onClick={() => { setFilter(""); onBrowse?.("all"); }} disabled={!onBrowse}><Stack aria-hidden="true" size={16}/><span>{t("pages.inventory.all")}</span></button>
        {hasFilter ? <><p className="notes-folder-parent">{t("pages.folders.searchAll")}</p><ul className="notes-page-list">{filterPageInventory(pages, filter, i18n.language).map(renderPage)}</ul>{!filterPageInventory(pages, filter, i18n.language).length && <p className="notes-list-state">{t("pages.overview.noMatches")}</p>}</> : <>
          {folders.data && !folders.data.truncated && <ul className="notes-inventory-collections">{renderFolder({ path: "", parent_path: "", name: t("pages.folders.root") }, 0)}</ul>}
          {!browsing && selectedPath === null && <ul className="notes-page-list">{pages.filter(page => pageFolderPath(page) === null).map(renderPage)}</ul>}
        </>}
        {folderUnavailable && <p className="notes-list-state" role="status">{t("pages.folders.unavailable")}</p>}
      </>}
    </div>
  </section>;
}
