// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { X } from "@phosphor-icons/react";
import "./PageInfoDrawer.css";

interface PageInfoDrawerProps {
  open: boolean;
  docked?: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  closeLabel: string;
}

const FOCUSABLE = 'summary, a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])';

function focusableControls(panel: HTMLElement): HTMLElement[] {
  return Array.from(panel.querySelectorAll<HTMLElement>(FOCUSABLE)).filter((element) => {
    if (element.tabIndex < 0 || element.matches(":disabled") || element.closest("[hidden], [inert]")) return false;
    for (let ancestor: HTMLElement | null = element; ancestor && ancestor !== panel; ancestor = ancestor.parentElement) {
      if (ancestor.tagName === "DETAILS" && !ancestor.hasAttribute("open")) {
        const summary = ancestor.querySelector(":scope > summary");
        if (!summary?.contains(element)) return false;
      }
      const style = getComputedStyle(ancestor);
      if (style.display === "none" || style.visibility === "hidden") return false;
    }
    return true;
  }).sort((left, right) => {
    const leftOrder = left.tabIndex > 0 ? left.tabIndex : Infinity;
    const rightOrder = right.tabIndex > 0 ? right.tabIndex : Infinity;
    if (leftOrder !== rightOrder) return leftOrder - rightOrder;
    return left.compareDocumentPosition(right) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1;
  });
}

export default function PageInfoDrawer({ open, onClose, title, children, closeLabel, docked = false }: PageInfoDrawerProps) {
  const [wide, setWide] = useState(() => typeof window.matchMedia === "function" && window.matchMedia("(min-width: 1100px)").matches);
  useEffect(() => {
    if (typeof window.matchMedia !== "function") return;
    const media = window.matchMedia("(min-width: 1100px)");
    const update = () => setWide(media.matches);
    media.addEventListener?.("change", update);
    return () => media.removeEventListener?.("change", update);
  }, []);
  const nonModal = docked && wide;
  const titleId = useId();
  const panelRef = useRef<HTMLElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    if (!open) return;
    const trigger = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    closeRef.current?.focus();

    const onKeyDown = (event: KeyboardEvent) => {
      if (nonModal && !(event.target instanceof Node && panelRef.current?.contains(event.target))) return;
      if (event.key === "Escape" && !event.isComposing) {
        event.preventDefault();
        event.stopPropagation();
        onCloseRef.current();
        return;
      }
      if (nonModal || event.key !== "Tab" || !panelRef.current) return;
      const controls = focusableControls(panelRef.current);
      const first = controls[0];
      const last = controls[controls.length - 1];
      const active = document.activeElement;
      const inside = active instanceof HTMLElement && controls.includes(active);
      if (event.shiftKey && (!inside || active === first)) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && (!inside || active === last)) {
        event.preventDefault();
        first?.focus();
      }
    };
    const containFocus = (event: FocusEvent) => {
      if (event.target instanceof Node && !panelRef.current?.contains(event.target)) closeRef.current?.focus();
    };
    document.addEventListener("keydown", onKeyDown, true);
    if (!nonModal) document.addEventListener("focusin", containFocus);
    return () => {
      document.removeEventListener("keydown", onKeyDown, true);
      document.removeEventListener("focusin", containFocus);
      if (trigger?.isConnected && (!nonModal || document.activeElement === document.body || panelRef.current?.contains(document.activeElement))) trigger.focus();
    };
  }, [open, nonModal]);

  if (!open) return null;

  return createPortal(
    <div className={`page-info-drawer-overlay${nonModal ? " page-info-drawer-docked" : ""}`} onClick={(event) => {
      if (event.target === event.currentTarget) onClose();
    }}>
      <aside
        aria-labelledby={titleId}
        aria-modal={nonModal ? undefined : true}
        className="page-info-drawer"
        ref={panelRef}
        role={nonModal ? "complementary" : "dialog"}
      >
        <header className="page-info-drawer-header">
          <h2 id={titleId}>{title}</h2>
          <button aria-label={closeLabel} className="page-info-drawer-close" onClick={onClose} ref={closeRef} type="button">
            <X aria-hidden="true" size={18} />
          </button>
        </header>
        <div className="page-info-drawer-content">{children}</div>
      </aside>
    </div>,
    document.body,
  );
}
