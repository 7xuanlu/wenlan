// SPDX-License-Identifier: AGPL-3.0-only
import type { ReactNode } from "react";

export type OpenProps =
  | { readonly onOpen: () => void; readonly openLabel: string }
  | { readonly onOpen?: undefined; readonly openLabel?: undefined };

interface AssetCardBase {
  readonly title: string;
  readonly context?: string | null;
  readonly footer: ReactNode;
  /** Review/truth state badges. Rendered on their own line so the footer
   *  always stays "space left, date right" and rows keep equal footers. */
  readonly status?: ReactNode;
  readonly testId?: string;
  readonly children?: ReactNode;
  /** Visual modifier. `detected` renders a dashed edge for not-yet-kept entities. */
  readonly variant?: "detected";
}

export type AssetCardProps = AssetCardBase & OpenProps;

/**
 * Flat presentational asset card. The root is an `<article>` (not a button)
 * because the footer carries its own interactive control (the space chip) —
 * nesting it inside a button would break keyboard and screen-reader
 * semantics. The title button is the card's open control; the surface picks
 * up hover/focus styling through `:hover`/`:has(:focus-visible)`. When
 * `onOpen` is absent the title renders as plain text (no button), for
 * entities with no dossier to open.
 */
export function AssetCard({
  title,
  context,
  footer,
  status,
  onOpen,
  openLabel,
  testId,
  children,
  variant,
}: AssetCardProps) {
  return (
    <article
      className={variant === "detected" ? "asset-card asset-card--detected" : "asset-card"}
      data-testid={testId}
    >
      {children}
      {onOpen ? (
        <button
          aria-label={openLabel}
          className="asset-card-open"
          onClick={onOpen}
          type="button"
        >
          <span className="asset-card-title">{title}</span>
        </button>
      ) : (
        <span className="asset-card-title">{title}</span>
      )}
      {context ? <p className="asset-card-context">{context}</p> : null}
      {status ? <div className="asset-card-status">{status}</div> : null}
      <div className="asset-card-footer">{footer}</div>
    </article>
  );
}
