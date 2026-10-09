import { FileText } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  EXPLICIT_BROWSE_QUERY_POLICY,
  listAllActivePagesExplicitBrowse,
  listAllDraftPagesExplicitBrowse,
} from "./listAllPages";
import { collectPageInventory, type WikiInventoryScope } from "./pageInventory";
import "./wikiInventoryOverview.css";
import { FirstPageMilestone } from "../../onboarding/FirstPageMilestone";

interface PagesOverviewProps {
  readonly inventoryScope?: WikiInventoryScope;
  readonly onBrowseAll?: () => void;
  readonly onBrowseFolder?: (scope: WikiInventoryScope) => void;
  readonly onCreatePage: (space: string | null, folderPath?: string) => void;
  readonly onSelectDraft: (draftId: string, space: string | null) => void;
  readonly onSelectPage: (pageId: string) => void;
  readonly onSelectSpace: (spaceName: string) => void;
}

export function PagesOverview({
  onSelectPage,
}: PagesOverviewProps) {
  const { t } = useTranslation();
  const activePagesQuery = useQuery({
    queryKey: ["pages", "active"],
    queryFn: listAllActivePagesExplicitBrowse,
    ...EXPLICIT_BROWSE_QUERY_POLICY,
  });
  const draftPagesQuery = useQuery({
    queryKey: ["pages", "draft"],
    queryFn: listAllDraftPagesExplicitBrowse,
    ...EXPLICIT_BROWSE_QUERY_POLICY,
  });
  const pages = collectPageInventory(activePagesQuery.data ?? [], draftPagesQuery.data ?? []);

  return (
    <section className="wiki-overview wiki-reading-start mx-auto w-full max-w-[1130px] pb-16">
      <FirstPageMilestone pages={pages} onSelectPage={onSelectPage} />
      <div className="wiki-reading-start-panel">
        <FileText aria-hidden="true" className="wiki-reading-start-icon" size={32} weight="regular" />
        <h1>{t("pages.overview.readingStartTitle")}</h1>
        <p>{t("pages.overview.readingStartDescription")}</p>
      </div>
    </section>
  );
}
