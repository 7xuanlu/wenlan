// SPDX-License-Identifier: AGPL-3.0-only
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Page } from "../../../lib/tauri";
import type { AssetLens } from "../../../lib/assetLens";
import type { SpaceDetailCopy } from "./copy";
import {
  PAGE_DISPLAY_STEP,
  PAGE_FETCH_LIMIT,
  pageCountLabel,
  sortedSpacePages,
} from "./model";

type SpaceDossierNavigation = {
  readonly onSelectPage: (pageId: string) => void;
};

type SpaceDossierContentProps = {
  readonly copy: SpaceDetailCopy;
  readonly navigation: SpaceDossierNavigation;
  readonly pages: readonly Page[];
  readonly lens: AssetLens;
};

function PageIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24">
      <path d="M12 2 2 7l10 5 10-5-10-5ZM2 12l10 5 10-5M2 17l10 5 10-5" />
    </svg>
  );
}

export function SpaceDossierContent({ copy, navigation, pages, lens }: SpaceDossierContentProps) {
  const { t, i18n } = useTranslation();
  const [visiblePageCount, setVisiblePageCount] = useState(PAGE_DISPLAY_STEP);
  const sortedPages = useMemo(() => sortedSpacePages(pages), [pages]);
  const visiblePages = sortedPages.slice(0, visiblePageCount);

  return (
    <div className="space-dossier-content">
      <section aria-label={copy.metrics.pages} className="space-dossier-pages">
        {sortedPages.length === 0 ? (
          <p className="space-dossier-empty">{copy.noPages}</p>
        ) : (
          <div className="space-dossier-page-list" data-lens={lens}>
            {visiblePages.map((page) => (
              <button key={page.id} onClick={() => navigation.onSelectPage(page.id)} type="button">
                <PageIcon />
                <span className="space-dossier-page-text">
                  <span className="space-dossier-page-title">{page.title}</span>{" "}
                  {page.summary?.trim() && <span className="space-dossier-page-summary">{page.summary}</span>}
                </span>
              </button>
            ))}
          </div>
        )}
        {visiblePageCount < sortedPages.length && (
          <button className="space-dossier-text-action" onClick={() => setVisiblePageCount((current) => current + PAGE_DISPLAY_STEP)} type="button">
            {t("spaceDetail.showMore")}
          </button>
        )}
        {pages.length >= PAGE_FETCH_LIMIT && (
          <p className="space-dossier-empty" role="status">
            {t("spaceDetail.pageLimit", { count: pageCountLabel(pages.length, i18n.language), limit: new Intl.NumberFormat(i18n.language).format(PAGE_FETCH_LIMIT) })}
          </p>
        )}
      </section>

    </div>
  );
}
