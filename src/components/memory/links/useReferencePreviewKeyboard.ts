// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, type RefObject } from "react";

const FOCUSABLE = "button:not(:disabled), summary, a[href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])";

function isAvailable(element: HTMLElement): boolean {
  if (element.tabIndex < 0 || element.matches(":disabled") || element.closest("[hidden], [inert]")) return false;
  const closedDetails = element.closest("details:not([open])");
  if (closedDetails && element !== closedDetails.querySelector("summary")) return false;
  for (let parent: HTMLElement | null = element; parent; parent = parent.parentElement) {
    const style = getComputedStyle(parent);
    if (style.display === "none" || style.visibility === "hidden") return false;
  }
  return true;
}

function focusableElements(root: ParentNode): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(FOCUSABLE)]
    .filter(isAvailable)
    .sort((left, right) => left.compareDocumentPosition(right) & Node.DOCUMENT_POSITION_FOLLOWING ? -1 : 1);
}

function nextFocusableAfter(anchor: HTMLElement): HTMLElement | null {
  return focusableElements(document).find((element) => {
    if (!(anchor.compareDocumentPosition(element) & Node.DOCUMENT_POSITION_FOLLOWING)) return false;
    if (element.closest("[data-reference-preview]")) return false;
    return true;
  }) ?? null;
}

export function useReferencePreviewKeyboard({
  anchor,
  boxRef,
  enabled,
  keyboardEntry,
  onDismiss,
  onEscape,
}: {
  anchor: HTMLElement | null;
  boxRef: RefObject<HTMLElement | null>;
  enabled: boolean;
  keyboardEntry: boolean;
  onDismiss(): void;
  onEscape?: () => void;
}) {
  useEffect(() => {
    if (!enabled || !anchor) return;
    const handleKey = (event: KeyboardEvent) => {
      const box = boxRef.current;
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        if (onEscape) onEscape();
        else {
          anchor.focus({ preventScroll: true });
          onDismiss();
        }
        return;
      }
      if (event.key !== "Tab") return;
      if (keyboardEntry && !event.shiftKey && document.activeElement === anchor) {
        const first = box ? focusableElements(box)[0] : null;
        if (first) {
          event.preventDefault();
          first.focus();
        }
        return;
      }
      if (!box?.contains(document.activeElement)) return;
      const controls = focusableElements(box);
      const index = controls.indexOf(document.activeElement as HTMLElement);
      if (index < 0) return;
      if (event.shiftKey) {
        event.preventDefault();
        if (index === 0) anchor.focus({ preventScroll: true });
        else controls[index - 1]?.focus();
      } else if (index < controls.length - 1) {
        event.preventDefault();
        controls[index + 1]?.focus();
      } else {
        const next = nextFocusableAfter(anchor);
        if (next) {
          event.preventDefault();
          next.focus();
        }
      }
    };
    document.addEventListener("keydown", handleKey, true);
    return () => document.removeEventListener("keydown", handleKey, true);
  }, [anchor, boxRef, enabled, keyboardEntry, onDismiss, onEscape]);
}
