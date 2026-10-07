// SPDX-License-Identifier: AGPL-3.0-only
import { SourceImportProgress } from "./SourceImportProgress";
import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { FileText, Folder, Globe, MagnifyingGlass, Plus, CaretRight } from "@phosphor-icons/react";
import { listIndexedFiles, listRegisteredSources, type IndexedFileInfo } from "../../../lib/tauri";
import "./SourceLibrary.css";

export type SourceLibraryFilter = "all" | "files" | "links" | "folders";

export interface SourceLibraryState {
  search: string;
  filter: SourceLibraryFilter;
}

export interface SourceLibraryProps {
  onAdd: () => void;
  onManageSources: () => void;
  onBrowseFolder: (sourceId: string) => void;
  onOpenDocument: (file: IndexedFileInfo) => void;
  /** Provide both props to retain search and filter in the owning route. */
  state?: SourceLibraryState;
  onStateChange?: (state: SourceLibraryState) => void;
}

const FILTERS: SourceLibraryFilter[] = ["all", "files", "links", "folders"];

function normalizedPath(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/, "");
}

function pathName(path: string): string {
  return normalizedPath(path).split("/").pop() || path;
}

function documentPath(file: IndexedFileInfo): string {
  const separator = file.source_id.indexOf("::");
  return separator >= 0 ? file.source_id.slice(separator + 2) : file.source_id;
}

/** Only imported document sources belong here, even when a memory has a URL. */
export function isLibraryDocument(file: IndexedFileInfo): boolean {
  if (file.memory_type === "recap") return false;
  return ["file", "webpage", "directory", "obsidian"].includes(file.source) ||
    (file.source === "memory" && ["folder", "obsidian"].includes(file.source_agent ?? ""));
}

function documentUrl(file: IndexedFileInfo): URL | null {
  for (const candidate of [file.source === "webpage" ? file.source_id : file.url]) {
    if (!candidate) continue;
    try {
      const url = new URL(candidate);
      if (url.protocol === "http:" || url.protocol === "https:") return url;
    } catch { /* Source IDs and local paths are not URLs. */ }
  }
  return null;
}

function documentFormat(file: IndexedFileInfo): string | null {
  const name = pathName(documentPath(file));
  const match = /\.([a-z0-9]+)$/i.exec(name) ?? /\.([a-z0-9]+)$/i.exec(file.title);
  if (!match) return null;
  return ["md", "markdown", "mdx"].includes(match[1].toLowerCase()) ? "Markdown" : match[1].toUpperCase();
}

export default function SourceLibrary({ onAdd, onManageSources, onBrowseFolder, onOpenDocument, state, onStateChange }: SourceLibraryProps) {
  const { t } = useTranslation();
  const [localState, setLocalState] = useState<SourceLibraryState>({ search: "", filter: "all" });
  const { search, filter } = state ?? localState;
  const foldersQuery = useQuery({ queryKey: ["registeredSources"], queryFn: listRegisteredSources });
  const filesQuery = useQuery({ queryKey: ["indexedFiles"], queryFn: () => listIndexedFiles(), refetchInterval: 5000 });

  const folders = useMemo(() => (foldersQuery.data ?? [])
    .filter((source) => !/(?:^|\/)\.wenlan\/sources$/.test(normalizedPath(source.path)))
    .sort((a, b) => pathName(a.path).localeCompare(pathName(b.path))), [foldersQuery.data]);
  const documents = useMemo(() => (filesQuery.data ?? []).filter(isLibraryDocument)
    .sort((a, b) => a.title.localeCompare(b.title)), [filesQuery.data]);
  const needle = search.trim().toLocaleLowerCase();
  const visibleFolders = filter === "files" || filter === "links" ? [] : folders.filter((source) =>
    `${pathName(source.path)} ${source.path}`.toLocaleLowerCase().includes(needle));
  const visibleDocuments = filter === "folders" ? [] : documents.filter((file) => {
    const link = file.source === "webpage" || documentUrl(file) !== null;
    return (filter === "all" || (filter === "links" ? link : !link)) &&
      `${file.title} ${documentPath(file)} ${file.url ?? ""}`.toLocaleLowerCase().includes(needle);
  });
  const bothLoaded = foldersQuery.isSuccess && filesQuery.isSuccess;
  const realEmpty = bothLoaded && folders.length === 0 && documents.length === 0;
  const filteredEmpty = bothLoaded && !realEmpty && visibleFolders.length === 0 && visibleDocuments.length === 0;

  function updateState(next: SourceLibraryState) {
    if (state === undefined) setLocalState(next);
    onStateChange?.(next);
  }

  function clearFilters() { updateState({ search: "", filter: "all" }); }

  return (
    <section className="source-library" aria-labelledby="source-library-title">
      <header className="source-library-header">
        <div>
          <h1 id="source-library-title">{t("sourceLibrary.title")}</h1>
          <p>{t("sourceLibrary.description")}</p>
        </div>
        <div className="source-library-actions">
          <button className="source-library-manage" onClick={onManageSources}>{t("sourceLibrary.manage")}</button>
          <button className="source-library-add" onClick={onAdd}><Plus size={16} aria-hidden="true" />{t("sourceLibrary.add")}</button>
        </div>
      </header>

      <SourceImportProgress />

      <div className="source-library-toolbar">
        <div className="source-library-filters" role="group" aria-label={t("sourceLibrary.filterLabel")}>
          {FILTERS.map((value) => <button key={value} aria-pressed={filter === value} onClick={() => updateState({ search, filter: value })}>{t(`sourceLibrary.filters.${value}`)}</button>)}
        </div>
        <label className="source-library-search">
          <MagnifyingGlass size={17} aria-hidden="true" />
          <input type="search" value={search} onChange={(event) => updateState({ search: event.target.value, filter })} placeholder={t("sourceLibrary.search")} aria-label={t("sourceLibrary.search")} />
        </label>
      </div>

      <div className="source-library-notices">
        {foldersQuery.isPending && <p role="status">{t("sourceLibrary.loadingFolders")}</p>}
        {filesQuery.isPending && <p role="status">{t("sourceLibrary.loadingDocuments")}</p>}
        {foldersQuery.isError && <div role="alert"><span>{t("sourceLibrary.folderError")}</span><button onClick={() => void foldersQuery.refetch()}>{t("sourceLibrary.retryFolders")}</button></div>}
        {filesQuery.isError && <div role="alert"><span>{t("sourceLibrary.documentError")}</span><button onClick={() => void filesQuery.refetch()}>{t("sourceLibrary.retryDocuments")}</button></div>}
      </div>

      {(visibleFolders.length > 0 || visibleDocuments.length > 0) && <ul className="source-library-list" aria-label={t("sourceLibrary.listLabel")}>
        {visibleFolders.map((source) => <li key={`folder:${source.id}`}>
          <button className="source-library-row" onClick={() => onBrowseFolder(source.id)}>
            <span className="source-library-icon source-library-icon-folder"><Folder size={23} aria-hidden="true" /></span>
            <span className="source-library-row-content"><span className="source-library-row-title">{pathName(source.path)}</span><span className="source-library-row-meta">{t(source.source_type === "obsidian" ? "sourceLibrary.obsidian" : "sourceLibrary.folder")}<span aria-hidden="true"> · </span>{source.path}</span></span>
            <CaretRight className="source-library-row-arrow" size={16} aria-hidden="true" />
          </button>
        </li>)}
        {visibleDocuments.map((file) => {
          const url = documentUrl(file);
          const link = file.source === "webpage" || url !== null;
          const path = normalizedPath(documentPath(file));
          const parent = path.includes("/") ? pathName(path.slice(0, path.lastIndexOf("/"))) : "";
          const metadata = link ? url?.hostname ?? t("sourceLibrary.webExcerpt") : [documentFormat(file) ?? t("sourceLibrary.document"), parent].filter(Boolean).join(" · ");
          return <li key={`${file.source}:${file.source_id}`}>
            <button className="source-library-row" onClick={() => onOpenDocument(file)}>
              <span className={`source-library-icon${link ? " source-library-icon-link" : ""}`}>{link ? <Globe size={23} aria-hidden="true" /> : <FileText size={23} aria-hidden="true" />}</span>
              <span className="source-library-row-content"><span className="source-library-row-title">{file.title || pathName(path)}</span><span className="source-library-row-meta">{metadata}</span></span>
              <CaretRight className="source-library-row-arrow" size={16} aria-hidden="true" />
            </button>
          </li>;
        })}
      </ul>}

      {realEmpty && <div className="source-library-empty"><FileText size={32} aria-hidden="true" /><h2>{t("sourceLibrary.emptyTitle")}</h2><p>{t("sourceLibrary.emptyBody")}</p><p className="source-library-support">{t("sourceLibrary.supportedFiles")}</p><button className="source-library-add" onClick={onAdd}><Plus size={16} aria-hidden="true" />{t("sourceLibrary.add")}</button></div>}
      {filteredEmpty && <div className="source-library-empty"><MagnifyingGlass size={28} aria-hidden="true" /><h2>{t("sourceLibrary.noMatches")}</h2><p>{t("sourceLibrary.noMatchesBody")}</p><button className="source-library-manage" onClick={clearFilters}>{t("sourceLibrary.clearFilters")}</button></div>}
    </section>
  );
}
