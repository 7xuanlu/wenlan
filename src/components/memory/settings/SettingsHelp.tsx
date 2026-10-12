// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { useReferencePopoverPosition } from "../links/useReferencePopoverPosition";

const POPOVER_WIDTH = 320;

export function SettingsHelp({ title, details, id }: {
  title: string;
  details: string;
  id?: string;
}) {
  const { t } = useTranslation();
  const generatedId = useId();
  const detailsId = id ?? generatedId;
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const position = useReferencePopoverPosition(open ? triggerRef.current : null, popoverRef, POPOVER_WIDTH);

  const dismiss = useCallback((restoreFocus = false) => {
    setOpen(false);
    if (restoreFocus) triggerRef.current?.focus();
  }, []);

  useEffect(() => {
    if (!open) return;

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Node)) return;
      if (triggerRef.current?.contains(target) || popoverRef.current?.contains(target)) return;
      dismiss();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      dismiss(true);
    };

    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open, dismiss]);

  const popover = open && typeof document !== "undefined" && document.body
    ? createPortal(
        <div
          ref={popoverRef}
          className="settings-help-popover"
          role="tooltip"
          style={{ top: position.top, left: position.left, zIndex: position.zIndex }}
        >
          {details}
        </div>,
        document.body,
      )
    : null;

  return (
    <>
      <span className="settings-help-anchor">
        <button
          ref={triggerRef}
          type="button"
          className="settings-help-trigger"
          aria-label={`${t("settings.detailsLabel")}: ${title}`}
          aria-expanded={open}
          aria-describedby={detailsId}
          onClick={() => setOpen((current) => !current)}
        >
          <svg aria-hidden="true" viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="12" cy="12" r="9" />
            <path d="M12 11v5" />
            <circle cx="12" cy="8" r="0.7" fill="currentColor" stroke="none" />
          </svg>
        </button>
        <span id={detailsId} className="sr-only">{details}</span>
      </span>
      {popover}
    </>
  );
}
