// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";

type EntityTopicMenuProps = {
  archived: boolean;
  actionsPending: boolean;
  onArchive: () => void;
  onDelete: () => void;
};

export function EntityTopicMenu({ archived, actionsPending, onArchive, onDelete }: EntityTopicMenuProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [keyboardNavigation, setKeyboardNavigation] = useState(false);
  const anchorRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const initialFocusRef = useRef<"first" | "last">("first");
  const items = () => Array.from(menuRef.current?.querySelectorAll<HTMLButtonElement>('[role^="menuitem"]:not(:disabled)') ?? []);
  const close = () => { setOpen(false); triggerRef.current?.focus(); };
  const invoke = (action: () => void) => { close(); action(); };
  useEffect(() => {
    if (!open) return;
    const controls = Array.from(menuRef.current?.querySelectorAll<HTMLButtonElement>('[role^="menuitem"]:not(:disabled)') ?? []);
    if (controls.length > 0) {
      (initialFocusRef.current === "last" ? controls[controls.length - 1] : controls[0])?.focus();
    } else {
      menuRef.current?.focus();
    }
    const onPointerDown = (event: PointerEvent) => {
      if (!anchorRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const onFocusIn = (event: FocusEvent) => {
      if (!anchorRef.current?.contains(event.target as Node)) setOpen(false);
    };
    const onEscape = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape" || event.isComposing || event.defaultPrevented) return;
      event.preventDefault(); event.stopPropagation();
      setOpen(false); triggerRef.current?.focus();
    };
    window.addEventListener("keydown", onEscape, true);
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("focusin", onFocusIn);
    return () => {
      window.removeEventListener("keydown", onEscape, true);
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("focusin", onFocusIn);
    };
  }, [open]);
  const handleKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault(); event.stopPropagation(); close(); return;
    }
    const controls = items();
    if (controls.length === 0) return;
    setKeyboardNavigation(true);
    const index = controls.indexOf(document.activeElement as HTMLButtonElement);
    let next = index;
    if (event.key === "ArrowDown") next = (index + 1) % controls.length;
    else if (event.key === "ArrowUp") next = (index - 1 + controls.length) % controls.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = controls.length - 1;
    else return;
    event.preventDefault(); event.stopPropagation(); controls[next]?.focus();
  };
  return (
    <div className="page-detail-actions-anchor" ref={anchorRef}>
      <button
        ref={triggerRef} type="button" className="mem-icon-action page-detail-actions-menu-trigger"
        aria-label={t("entityDetail.actionsLabel")} title={t("entityDetail.actionsLabel")}
        aria-haspopup="menu" aria-expanded={open}
        onClick={(event) => {
          setKeyboardNavigation(event.detail === 0);
          initialFocusRef.current = "first";
          setOpen((value) => !value);
        }}
        onKeyDown={(event) => {
          if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
          event.preventDefault(); event.stopPropagation();
          setKeyboardNavigation(true);
          initialFocusRef.current = event.key === "ArrowUp" ? "last" : "first";
          setOpen(true);
        }}
      >
        <svg aria-hidden="true" width="16" height="4" viewBox="0 0 16 4" fill="currentColor">
          <circle cx="2" cy="2" r="1.5" /><circle cx="8" cy="2" r="1.5" /><circle cx="14" cy="2" r="1.5" />
        </svg>
      </button>
      {open ? (
        <div className="mem-popover-surface page-detail-actions-menu entity-topic-actions-menu" data-keyboard-navigation={keyboardNavigation ? "true" : "false"} role="menu" aria-label={t("entityDetail.actionsLabel")} ref={menuRef} tabIndex={-1} onKeyDown={handleKeyDown}>
          <button type="button" role="menuitem" disabled={actionsPending} onClick={() => invoke(onArchive)}>{t(archived ? "entities.actions.restore" : "entities.actions.archive")}</button>
          <button type="button" role="menuitem" className="page-detail-menu-danger" disabled={actionsPending} onClick={() => invoke(onDelete)}>{t("entityDetail.deleteEntity")}</button>
        </div>
      ) : null}
    </div>
  );
}
