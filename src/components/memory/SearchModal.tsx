// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useLayoutEffect, useRef, type KeyboardEvent, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import "./search-modal.css";

interface SearchModalProps {
  open: boolean;
  query: string;
  placeholder: string;
  loading: boolean;
  error: boolean;
  inputRef: RefObject<HTMLInputElement | null>;
  returnFocusRef: RefObject<HTMLElement | null>;
  fallbackFocusRef: RefObject<HTMLElement | null>;
  onQueryChange: (query: string) => void;
  onClose: () => void;
  children: ReactNode;
}

export function SearchModal({
  open,
  query,
  placeholder,
  loading,
  error,
  inputRef,
  returnFocusRef,
  fallbackFocusRef,
  onQueryChange,
  onClose,
  children,
}: SearchModalProps) {
  const { t } = useTranslation();
  const dialogRef = useRef<HTMLDivElement>(null);
  const wasOpenRef = useRef(false);

  useLayoutEffect(() => {
    if (open) {
      wasOpenRef.current = true;
      inputRef.current?.focus();
    } else if (wasOpenRef.current) {
      wasOpenRef.current = false;
      const requested = returnFocusRef.current;
      const canReturnToRequested = requested?.isConnected
        && !requested.closest("[inert], [hidden], [aria-hidden='true']")
        && getComputedStyle(requested).visibility !== "hidden"
        && getComputedStyle(requested).display !== "none";
      const target = canReturnToRequested
        ? requested
        : fallbackFocusRef.current;
      if (target?.isConnected) target.focus();
    }
  }, [open, inputRef, returnFocusRef, fallbackFocusRef]);

  useEffect(() => {
    if (!open) return;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    return () => { document.body.style.overflow = previousOverflow; };
  }, [open]);

  if (!open) return null;

  const trapFocus = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose();
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      const active = document.activeElement;
      const results = dialogRef.current?.querySelectorAll<HTMLButtonElement>(
        ".search-modal-results button:not(:disabled)",
      );
      if (!results?.length) return;
      if (active === inputRef.current && event.key === "ArrowDown") {
        event.preventDefault();
        results[0].focus();
        return;
      }
      const index = Array.from(results).indexOf(active as HTMLButtonElement);
      if (index < 0) return;
      event.preventDefault();
      if (event.key === "ArrowDown") {
        (index === results.length - 1 ? inputRef.current : results[index + 1])?.focus();
      } else {
        (index === 0 ? inputRef.current : results[index - 1])?.focus();
      }
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = dialogRef.current?.querySelectorAll<HTMLElement>(
      "button:not(:disabled), input:not(:disabled), [href], select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex='-1'])",
    );
    if (!focusable?.length) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  return createPortal(
    <div
      className="search-modal-backdrop"
      onMouseDown={(event) => {
        if (event.target !== event.currentTarget) return;
        // Prevent the backdrop default action from moving restored focus to body.
        event.preventDefault();
        onClose();
      }}
    >
      <section
        aria-label={t("main.searchButton")}
        aria-modal="true"
        className="search-modal"
        onKeyDown={trapFocus}
        ref={dialogRef}
        role="dialog"
      >
        <div className="search-modal-field">
          <svg aria-hidden="true" className="search-modal-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path d="m21 21-4.35-4.35m2.35-5.65a8 8 0 1 1-16 0 8 8 0 0 1 16 0Z" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.8" />
          </svg>
          <input
            aria-label={t("main.searchButton")}
            autoComplete="off"
            data-wenlan-search-input
            onChange={(event) => onQueryChange(event.target.value)}
            placeholder={placeholder}
            ref={inputRef}
            spellCheck={false}
            value={query}
          />
          <button aria-label={t("main.searchClose")} className="search-modal-close" onClick={onClose} type="button">
            <svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path d="M18 6 6 18M6 6l12 12" strokeLinecap="round" strokeWidth="1.8" />
            </svg>
          </button>
        </div>
        <div aria-live="polite" className="search-modal-results">
          {loading && <p className="search-modal-status" role="status">{t("main.searchLoading")}</p>}
          {error && <p className="search-modal-status" role="alert">{t("main.searchError")}</p>}
          {children}
        </div>
      </section>
    </div>,
    document.body,
  );
}
