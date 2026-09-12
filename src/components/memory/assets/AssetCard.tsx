// SPDX-License-Identifier: AGPL-3.0-only
import type { ReactNode } from "react";

interface AssetCardProps {
  readonly title: string;
  readonly context?: string | null;
  readonly footer: ReactNode;
  readonly onOpen: () => void;
  readonly openLabel: string;
  readonly testId?: string;
  readonly children?: ReactNode;
}

/**
 * Flat presentational asset card. The root is an `<article>` (not a button)
 * because the footer carries its own interactive control (the space chip) —
 * nesting it inside a button would break keyboard and screen-reader
 * semantics. The title button is the card's open control; the surface picks
 * up hover/focus styling through `:hover`/`:has(:focus-visible)`.
 */
export function AssetCard({
  title,
  context,
  footer,
  onOpen,
  openLabel,
  testId,
  children,
}: AssetCardProps) {
  return (
    <article className="asset-card" data-testid={testId}>
      {children}
      <button
        aria-label={openLabel}
        className="asset-card-open"
        onClick={onOpen}
        type="button"
      >
        <span className="asset-card-title">{title}</span>
      </button>
      {context ? <p className="asset-card-context">{context}</p> : null}
      <div className="asset-card-footer">{footer}</div>
    </article>
  );
}
