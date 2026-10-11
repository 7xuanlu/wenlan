// SPDX-License-Identifier: AGPL-3.0-only
import { useId, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "./settings/primitives";
import { describeRemoteError, type RemoteError, type RemoteErrorKind } from "../../lib/remoteErrors";

export const secondaryText = "text-[var(--mem-text-secondary)] text-sm";
export const errorText = "text-sm text-[var(--mem-status-danger-text)] break-words";

const MESSAGE_KEY = {
  expired: "remoteAccess.errorExpired",
  tooMany: "remoteAccess.errorTooMany",
  offline: "remoteAccess.errorOffline",
  ended: "remoteAccess.errorEnded",
  other: "remoteAccess.errorGeneric",
} as const satisfies Record<RemoteErrorKind, string>;

/** A show/hide control for text most people never need. Collapsed until asked. */
export function Disclosure({ label, children, defaultOpen = false }: { label: string; children: ReactNode; defaultOpen?: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  const id = useId();
  return (
    <div className="min-w-0">
      <button type="button" aria-expanded={open} aria-controls={id}
        className="text-sm text-[var(--mem-text-secondary)] underline decoration-dotted underline-offset-2 hover:text-[var(--mem-text)] focus-visible:outline-2 focus-visible:outline-[var(--mem-focus-ring)]"
        onClick={() => setOpen((value) => !value)}>
        {label}
      </button>
      <div id={id} hidden={!open} className="mt-2">{open ? children : null}</div>
    </div>
  );
}

/** What went wrong in plain words; the native sentence sits behind "Details". */
export function RemoteErrorMessage({ error, context = "other" }: { error: unknown; context?: "pairing" | "other" }) {
  const { t } = useTranslation();
  const described: RemoteError = describeRemoteError(error, context);
  return (
    <div className="min-w-0 space-y-1">
      <p role="alert" className={errorText}>{t(MESSAGE_KEY[described.kind])}</p>
      {described.details && (
        <Disclosure label={t("remoteAccess.details")}>
          <p className="text-xs break-words text-[var(--mem-text-secondary)]">{described.details}</p>
        </Disclosure>
      )}
    </div>
  );
}

/** Asks once, in place, before something that ends connections. Escape cancels. */
export function InlineConfirm({ message, confirmLabel, busy, onConfirm, onCancel }: {
  message: string;
  confirmLabel: string;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div role="group" aria-label={message}
      className="min-w-0 space-y-2 rounded border border-[var(--mem-border)] bg-[var(--mem-bg)] p-3"
      onKeyDown={(event) => { if (event.key === "Escape") { event.stopPropagation(); onCancel(); } }}>
      <p className="text-sm break-words">{message}</p>
      <div className="flex flex-wrap gap-2">
        <Button variant="danger" size="sm" autoFocus disabled={busy} onClick={onConfirm}>{confirmLabel}</Button>
        <Button variant="ghost" size="sm" disabled={busy} onClick={onCancel}>{t("remoteAccess.cancel")}</Button>
      </div>
    </div>
  );
}
