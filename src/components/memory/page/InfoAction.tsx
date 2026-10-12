// SPDX-License-Identifier: AGPL-3.0-only
import { useId } from "react";
import "./InfoAction.css";

/** A resource action and its consequence, shared by every Info pane. */
export function InfoAction({ label, description, active = false, disabled = false, pending = false, error, showDescription = true, onClick }: {
  label: string;
  description: string;
  active?: boolean;
  disabled?: boolean;
  pending?: boolean;
  error?: string;
  showDescription?: boolean;
  onClick: () => void;
}) {
  const descriptionId = useId();
  return <div className="info-action">
    <button type="button" className="info-action-button" data-active={active}
      disabled={disabled || pending} aria-busy={pending} title={showDescription ? description : undefined}
      aria-describedby={showDescription ? descriptionId : undefined} onClick={onClick}>
      <svg aria-hidden="true" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
        <circle cx="12" cy="12" r="9" />
        {active ? <path d="m8 12 2.5 2.5L16 9" /> : <path d="m8 12 2.5 2.5L16 9" opacity=".45" />}
      </svg>
      <span>{label}</span>
    </button>
    {showDescription && <p id={descriptionId} className="info-action-description">{description}</p>}
    {error && <p className="info-action-error" role="alert">{error}</p>}
  </div>;
}
