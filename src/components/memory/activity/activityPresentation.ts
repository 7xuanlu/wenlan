// SPDX-License-Identifier: AGPL-3.0-only
import type { KnownActivityAssetKind } from "../../../lib/activitySentence";

/** Stable ordering for background activity rows. */
export const ACTIVITY_ASSET_ORDER: readonly KnownActivityAssetKind[] = [
  "memories",
  "entities",
  "pages",
];
