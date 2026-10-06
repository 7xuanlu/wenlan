// SPDX-License-Identifier: AGPL-3.0-only
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Entity, Page } from "../../../lib/tauri";
import type { SpaceDetailCopy } from "./copy";
import {
  KEY_ENTITY_LIMIT,
  PAGE_DISPLAY_STEP,
  PAGE_FETCH_LIMIT,
  pageCountLabel,
  pagesNeedingReview,
  reviewReasonName,
  sortedKeyEntities,
  sortedSpacePages,
} from "./model";

type SpaceDossierNavigation = {
  readonly onEntityClick: (entityId: string) => void;
  readonly onReviewAll?: () => void;
  readonly onSelectPage: (pageId: string) => void;
};

type SpaceDossierContentProps = {
  readonly copy: SpaceDetailCopy;
  readonly entities: readonly Entity[];
  readonly navigation: SpaceDossierNavigation;
  readonly pages: readonly Page[];
};

function PageIcon() {
  return (
    <svg aria-hidden="true" viewBox="0 0 24 24">
      <path d="M12 2 2 7l10 5 10-5-10-5ZM2 12l10 5 10-5M2 17l10 5 10-5" />
    </svg>
  );
}

export function SpaceDossierContent({ copy, entities, navigation, pages }: SpaceDossierContentProps) {
  const { t, i18n } = useTranslation();
  const [visiblePageCount, setVisiblePageCount] = useState(PAGE_DISPLAY_STEP);
  const [showAllEntities, setShowAllEntities] = useState(false);
  const sortedPages = useMemo(() => sortedSpacePages(pages), [pages]);
  const reviewPages = useMemo(() => pagesNeedingReview(pages), [pages]);
  const keyEntities = useMemo(() => sortedKeyEntities(entities), [entities]);
  const visiblePages = sortedPages.slice(0, visiblePageCount);
  const visibleEntities = showAllEntities ? keyEntities : keyEntities.slice(0, KEY_ENTITY_LIMIT);

  return (
    <div className="space-dossier-content">
      <section aria-label={copy.metrics.pages} className="space-dossier-pages">
        <h2>{copy.metrics.pages}</h2>
        {sortedPages.length === 0 ? (
          <p className="space-dossier-empty">{copy.noPages}</p>
        ) : (
          <div className="space-dossier-page-list">
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

      <div className="space-dossier-secondary">
        <section aria-label={copy.keyEntities} className="space-dossier-entities">
          <details className="space-dossier-disclosure">
            <summary>{copy.keyEntities}</summary>
            {visibleEntities.length === 0 ? (
              <p className="space-dossier-empty">{copy.noEntities}</p>
            ) : (
              <div className="space-dossier-entity-list">
                {visibleEntities.map((entity) => (
                  <button key={entity.id} onClick={() => navigation.onEntityClick(entity.id)} type="button">{entity.name}</button>
                ))}
              </div>
            )}
            {keyEntities.length > KEY_ENTITY_LIMIT && (
              <button className="space-dossier-text-action" onClick={() => setShowAllEntities((current) => !current)} type="button">
                {showAllEntities ? copy.showLess : copy.viewAllEntities(keyEntities.length)}
              </button>
            )}
          </details>
        </section>

        <section aria-label={copy.needsReview} className="space-dossier-review">
          <details className="space-dossier-disclosure">
            <summary>{copy.needsReview}</summary>
            {reviewPages.length === 0 ? (
              <p className="space-dossier-empty">{copy.noReview}</p>
            ) : (
              <div className="space-dossier-review-list">
                {reviewPages.map((page) => (
                  <button key={page.id} onClick={() => navigation.onSelectPage(page.id)} type="button">
                    <PageIcon />
                    <span>{page.title}</span>
                    <small>{copy.reasons[reviewReasonName(page)]}</small>
                  </button>
                ))}
              </div>
            )}
            {navigation.onReviewAll && (
              <button className="space-dossier-text-action space-dossier-text-action-review" onClick={navigation.onReviewAll} type="button">{copy.reviewAll}</button>
            )}
          </details>
        </section>
      </div>
    </div>
  );
}
