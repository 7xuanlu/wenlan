import { PageIcon } from "../assets/PageIcon";
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
import { GettingStarted } from "../../onboarding/GettingStarted";

interface PagesOverviewProps {
  readonly inventoryScope?: WikiInventoryScope;
  readonly onBrowseAll?: () => void;
  readonly onBrowseFolder?: (scope: WikiInventoryScope) => void;
  readonly onCreatePage: (space: string | null, folderPath?: string) => void;
  readonly onSelectDraft: (draftId: string, space: string | null) => void;
  readonly onSelectPage: (pageId: string) => void;
  readonly onSelectSpace: (spaceName: string) => void;
  /** Getting-started destinations. The checklist renders only when all three
   *  are wired, so no step ever names an action with nothing behind it. */
  readonly onConnectTool?: () => void;
  readonly onAddSources?: () => void;
  readonly onSeeExample?: () => void;
}

export function PagesOverview({
  onCreatePage,
  onSelectPage,
  onConnectTool,
  onAddSources,
  onSeeExample,
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
  const gettingStarted = onConnectTool && onAddSources && onSeeExample ? (
    <GettingStarted
      pages={pages}
      pagesReady={activePagesQuery.isSuccess && draftPagesQuery.isSuccess}
      onWriteNote={() => onCreatePage(null)}
      onConnectTool={onConnectTool}
      onAddSources={onAddSources}
      onSeeExample={onSeeExample}
    />
  ) : null;

  return (
    <section
      className="wiki-overview wiki-reading-start mx-auto w-full max-w-[1130px] pb-16"
      data-library-empty={activePagesQuery.isSuccess && draftPagesQuery.isSuccess && pages.length === 0 ? "true" : undefined}
    >
      <FirstPageMilestone pages={pages} onSelectPage={onSelectPage} />
      <div className="wiki-reading-start-panel">
        <PageIcon className="wiki-reading-start-icon" size={32} />
        <h1>{t("pages.overview.readingStartTitle")}</h1>
        <p>{t("pages.overview.readingStartDescription")}</p>
      </div>
      {gettingStarted}
    </section>
  );
}
