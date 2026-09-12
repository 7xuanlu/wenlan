// SPDX-License-Identifier: AGPL-3.0-only
import { useTranslation } from "react-i18next";
import type { AssetLens } from "../../../lib/assetLens";

interface AssetLensToggleProps {
  readonly value: AssetLens;
  readonly onChange: (lens: AssetLens) => void;
}

/**
 * Icon-only rows/cards lens switch. Two buttons in one bordered group;
 * the pressed state is exposed through `aria-pressed` (styled via the
 * attribute selector, no class toggling).
 */
export function AssetLensToggle({ value, onChange }: AssetLensToggleProps) {
  const { t } = useTranslation();
  const rowsLabel = t("assets.lens.rows");
  const cardsLabel = t("assets.lens.cards");

  return (
    <div
      aria-label={t("assets.lens.viewLabel")}
      className="asset-lens-toggle"
      data-testid="asset-lens-toggle"
      role="group"
    >
      <button
        aria-label={rowsLabel}
        aria-pressed={value === "rows"}
        className="asset-lens-button"
        data-testid="asset-lens-rows"
        onClick={() => onChange("rows")}
        title={rowsLabel}
        type="button"
      >
        <svg
          aria-hidden="true"
          fill="none"
          stroke="currentColor"
          strokeLinecap="round"
          strokeWidth="2"
          viewBox="0 0 24 24"
        >
          <path d="M3 6h18M3 12h18M3 18h18" />
        </svg>
      </button>
      <button
        aria-label={cardsLabel}
        aria-pressed={value === "cards"}
        className="asset-lens-button"
        data-testid="asset-lens-cards"
        onClick={() => onChange("cards")}
        title={cardsLabel}
        type="button"
      >
        <svg
          aria-hidden="true"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          viewBox="0 0 24 24"
        >
          <rect height="7" rx="1" width="7" x="3" y="3" />
          <rect height="7" rx="1" width="7" x="14" y="3" />
          <rect height="7" rx="1" width="7" x="3" y="14" />
          <rect height="7" rx="1" width="7" x="14" y="14" />
        </svg>
      </button>
    </div>
  );
}
