// SPDX-License-Identifier: AGPL-3.0-only
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Button } from "../settings/primitives";
import { distillReview, listRefinements, rejectRefinement } from "../../../lib/tauri";
import { DISTILL_REVIEW_SESSION_QUERY_KEY, pageCandidateItems } from "../pages/pageReviewSignals";
import { isActionableReviewItem } from "../reviewDisposition";
import { useReviewItemSummary } from "../ReviewDialog";
import { REVIEW_QUEUE_LIMIT, type ReviewItem } from "../useReviewQueue";
import { reviewSuppressKey, useSuppressedReviewItems } from "../reviewSuppression";
import "./activityDiscoveries.css";

function Finding({ item, onDismiss, dismissPending }: {
  item: ReviewItem & { kind: "refinement" };
  onDismiss: (item: ReviewItem & { kind: "refinement" }) => void;
  dismissPending: boolean;
}) {
  const { t } = useTranslation();
  const summary = useReviewItemSummary(item);
  const crossSpace = item.action === "cross_space_discovery";
  const crossSpacePayload = item.payload?.action === "cross_space_discovery" ? item.payload : null;
  return <li>
    <h4>{crossSpace ? t("activityDiscoveries.crossSpace") : summary.title}</h4>
    <p className="mem-activity-discovery-hint">{crossSpacePayload
      ? t("review.crossSpaceSpaces", { spaces: crossSpacePayload.spaces.join(" · "), count: crossSpacePayload.memory_count })
      : t("activityDiscoveries.unavailableHint")}</p>
    {!crossSpace && summary.reason && <p className="mem-activity-discovery-hint">{summary.reason}</p>}
    <Button type="button" variant="secondary" size="sm" disabled={dismissPending} onClick={() => onDismiss(item)}>{t("review.dismiss")}</Button>
  </li>;
}

/** Read-only suggestions are shown on Activity, outside the decision queue. */
export default function ActivityDiscoveries() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { hiddenKeys, hiddenEntries, hide, restore } = useSuppressedReviewItems();
  const discoveries = useQuery({
    queryKey: DISTILL_REVIEW_SESSION_QUERY_KEY,
    queryFn: distillReview,
    staleTime: 30_000,
    gcTime: Infinity,
    retry: false,
  });
  const refinements = useQuery({
    queryKey: ["refinement-proposals"],
    queryFn: () => listRefinements(REVIEW_QUEUE_LIMIT),
    staleTime: 30_000,
    retry: false,
  });
  const dismiss = useMutation({
    mutationFn: (item: ReviewItem & { kind: "refinement" }) => rejectRefinement(item.id),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["refinement-proposals"] }),
  });
  const findings = (refinements.data?.proposals ?? []).map((proposal): ReviewItem & { kind: "refinement" } => ({
    kind: "refinement", id: proposal.id, action: proposal.action,
    sourceIds: proposal.source_ids, payload: proposal.payload ?? null,
    confidence: proposal.confidence, timestampMs: null,
  })).filter(item => !item.id.startsWith("example:") && !isActionableReviewItem(item));
  const candidates = pageCandidateItems(discoveries.data, t("review.untitledCluster"))
    .filter((item): item is Extract<ReviewItem, { kind: "page_candidate" }> =>
      item.kind === "page_candidate" && !hiddenKeys.has(reviewSuppressKey(item) ?? ""));
  const topics = (discoveries.data?.orphan_topics ?? []).map((topic): ReviewItem & { kind: "topic" } => ({
    kind: "topic", id: topic.label, label: topic.label, count: topic.count, timestampMs: null,
  })).filter((item): item is Extract<ReviewItem, { kind: "topic" }> =>
    !hiddenKeys.has(reviewSuppressKey(item) ?? ""));
  const pending = discoveries.isPending || refinements.isPending;
  const failed = discoveries.isError || refinements.isError;
  const partial = (refinements.data?.proposals.length ?? 0) >= REVIEW_QUEUE_LIMIT;
  const activityHiddenEntries = hiddenEntries.filter((entry) => entry.kind === "page_candidate" || entry.kind === "topic");
  const hasResults = candidates.length + topics.length + findings.length > 0;
  return <section className="mem-activity-discoveries" aria-labelledby="mem-activity-suggestions-title">
    <h2 id="mem-activity-suggestions-title" className="mem-activity-section-title">{t("activityDiscoveries.title")}</h2>
    <div className="mem-activity-discoveries-body">
      <p className="mem-activity-discovery-hint">{t("activityDiscoveries.hint")}</p>
      {pending && <p role="status">{t("activityDiscoveries.loading")}</p>}
      {failed && <div role="alert"><p>{t("activityDiscoveries.failed")}</p><Button type="button" variant="secondary" size="sm" onClick={() => { void discoveries.refetch(); void refinements.refetch(); }}>{t("activityDiscoveries.retry")}</Button></div>}
      {dismiss.error && <p role="alert">{dismiss.error instanceof Error ? dismiss.error.message : String(dismiss.error)}</p>}
      {partial && <p>{t("activityDiscoveries.partial")}</p>}
      {!pending && !failed && !partial && !hasResults && <p>{t("activityDiscoveries.empty")}</p>}
      {candidates.length > 0 && <section>
        <h3>{t("activityDiscoveries.candidates")}</h3>
        <ul>{candidates.map((item) => <li key={item.id}>
          <h4>{item.title}</h4>
          <p className="mem-activity-discovery-hint">{t("activityDiscoveries.notGenerated")} · {t("activityDiscoveries.sourceCount", { count: item.cluster.source_ids.length })}</p>
          {item.cluster.contents.length > 0 && <details className="mem-activity-source-preview"><summary>{t("knowledgeContext.sources")}</summary><ul>{item.cluster.contents.map((content, i) => <li key={i} className="mem-activity-discovery-source">{content}</li>)}</ul></details>}
          <Button type="button" variant="secondary" size="sm" onClick={() => hide(item)}>{t("review.hide")}</Button>
        </li>)}</ul>
      </section>}
      {findings.length > 0 && <section><h3>{t("activityDiscoveries.proposals")}</h3><ul>{findings.map(item => <Finding key={item.id} item={item} onDismiss={dismiss.mutate} dismissPending={dismiss.isPending} />)}</ul></section>}
      {topics.length > 0 && <section><h3>{t("activityDiscoveries.topics")}</h3><ul>{topics.map(topic => <li key={topic.label}><h4>{topic.label}</h4><p className="mem-activity-discovery-hint">{t("activityDiscoveries.topicCount", { count: topic.count })}</p><Button type="button" variant="secondary" size="sm" onClick={() => hide(topic)}>{t("review.hide")}</Button></li>)}</ul></section>}
      {activityHiddenEntries.length > 0 && <details className="mem-activity-hidden-results">
        <summary>{t("review.hiddenSectionTitle")}</summary>
        <ul>{activityHiddenEntries.map((entry) => <li key={entry.key}>
          <span>{entry.label}</span> <Button type="button" variant="secondary" size="sm" onClick={() => restore(entry.key)}>{t("review.restore")}</Button>
        </li>)}</ul>
      </details>}
    </div>
  </section>;
}
