// SPDX-License-Identifier: AGPL-3.0-only
import { reviewApproveBlocked } from "./ReviewDialog";
import type { ReviewItem } from "./useReviewQueue";

/** Whether an item can receive a real decision through the existing dialog. */
export function isActionableReviewItem(item: ReviewItem): boolean {
  if (item.id.startsWith("example:")) return false;
  if (item.kind === "capture" || item.kind === "page_candidate" || item.kind === "topic") {
    return false;
  }
  // Repair proposals use their dedicated SourceRepairReview choice flow. The
  // daemon action tag must match before this item can enter that flow.
  if (item.kind === "refinement" && item.action === "lint_repair_review") {
    return item.payload?.action === "lint_repair_review";
  }
  return !reviewApproveBlocked(item);
}
