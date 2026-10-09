// SPDX-License-Identifier: AGPL-3.0-only
import type { PageLinkOutbound } from "../../../lib/tauri";
import { FileText } from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import "./NoteInfo.css";

interface RelatedPagesProps {
  outbound: PageLinkOutbound[];
  onPageClick?: (pageId: string) => void;
}

export default function RelatedPages({ outbound, onPageClick }: RelatedPagesProps) {
  const { t } = useTranslation();
  // An empty "Related pages" header is noise, not information.
  if (outbound.length === 0) return null;

  return (
    <section aria-label={t("knowledgeContext.linkedPages")} className="memory-detail-rail-section note-info-related-pages">
      <h3 className="note-info-section-heading">
        {t("knowledgeContext.linkedPages")}
      </h3>
      <div className="note-info-link-list">
        {outbound.map((link, idx) => {
          const key = `${link.label}-${link.target_page_id ?? idx}`;
          const inner = (
            <span className="note-info-link-copy">
              <FileText size={16} weight="regular" aria-hidden="true" />
              <span className="note-info-link-label">{link.target_page_id ? link.target_title || link.label : link.label}</span>
            </span>
          );
          const targetPageId = link.target_page_id;
          if (!targetPageId || !onPageClick) {
            return (
              <div key={key} className="note-info-link-row is-unresolved" title={t("pageInfo.unresolvedLink")}>
                {inner}
                <span className="note-info-link-status">{t("pageInfo.unresolvedLink")}</span>
              </div>
            );
          }
          return (
            <button
              key={key}
              type="button"
              onClick={() => onPageClick(targetPageId)}
              className="note-info-link-row"
            >
              {inner}
            </button>
          );
        })}
      </div>
    </section>
  );
}
