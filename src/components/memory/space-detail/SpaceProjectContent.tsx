// SPDX-License-Identifier: AGPL-3.0-only
import { useId, useRef, useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import type { Page } from "../../../lib/tauri";
import type { AssetLens } from "../../../lib/assetLens";
import { AssetLensToggle } from "../assets/AssetLensToggle";
import "../assets/assetCards.css";
import "../assets/collectionToolbar.css";
import type { SpaceDetailCopy } from "./copy";
import { SpaceDossierContent } from "./SpaceDossierContent";
import { SpaceSources } from "./SpaceSources";

type Props = {
  copy: SpaceDetailCopy;
  spaceId: string;
  spaceName: string;
  pages: readonly Page[];
  pagesPending: boolean;
  pagesError: boolean;
  onRetryPages: () => void;
  onSelectPage: (id: string) => void;
};

export function SpaceProjectContent({ copy, spaceId, spaceName, pages, pagesPending, pagesError, onRetryPages, onSelectPage }: Props) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<"notes" | "sources">("notes");
  const [lenses, setLenses] = useState<Readonly<Record<string, AssetLens>>>({});
  const lens = lenses[spaceId] ?? "rows";
  const setLens = (next: AssetLens) => setLenses((current) => ({ ...current, [spaceId]: next }));
  const id = useId();
  const notesRef = useRef<HTMLButtonElement>(null);
  const sourcesRef = useRef<HTMLButtonElement>(null);
  function changeTab(event: KeyboardEvent<HTMLDivElement>) {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    event.preventDefault();
    const next = event.key === "Home" ? "notes" : event.key === "End" ? "sources" : tab === "notes" ? "sources" : "notes";
    setTab(next);
    (next === "notes" ? notesRef : sourcesRef).current?.focus();
  }
  return <>
    <div className="space-project-controls collection-toolbar">
      <div className="space-project-tabs" role="tablist" aria-label={t("spaceDetail.projectContent")} onKeyDown={changeTab}>
        <button ref={notesRef} id={`${id}-notes`} aria-controls={`${id}-panel`} role="tab" aria-selected={tab === "notes"} tabIndex={tab === "notes" ? 0 : -1} type="button" onClick={() => setTab("notes")}>{t("spaceDetail.notesTab")}</button>
        <button ref={sourcesRef} id={`${id}-sources`} aria-controls={`${id}-panel`} role="tab" aria-selected={tab === "sources"} tabIndex={tab === "sources" ? 0 : -1} type="button" onClick={() => setTab("sources")}>{t("spaceDetail.sourcesTab")}</button>
      </div>
      <AssetLensToggle value={lens} onChange={setLens} />
    </div>
    <div id={`${id}-panel`} role="tabpanel" aria-labelledby={`${id}-${tab}`}>
      {tab === "sources" ? <SpaceSources key={`${spaceId}:${spaceName}`} spaceId={spaceId} spaceName={spaceName} lens={lens} /> : pagesPending ? <p role="status" className="space-dossier-empty">{copy.loading}</p> : pagesError ? <div role="alert"><p className="space-dossier-empty">{copy.relatedLoadError}</p><button type="button" className="page-editor-action" onClick={onRetryPages}>{t("sourceAccess.retry")}</button></div> : <SpaceDossierContent copy={copy} pages={pages} lens={lens} navigation={{ onSelectPage }} />}
    </div>
  </>;
}
