// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useId, useMemo, useRef, useState, type ChangeEvent, type KeyboardEvent } from "react";
import { Check, CaretDown } from "@phosphor-icons/react";
import "./AtlasSelect.css";

export interface AtlasSelectOption {
  value: string;
  label: string;
}

export interface AtlasSelectProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: AtlasSelectOption[];
  searchLabel: string;
  noMatchesLabel: string;
}

type PopupAlign = "start" | "end";

function optionId(listboxId: string, index: number): string {
  return `${listboxId}-option-${index}`;
}

export default function AtlasSelect({
  label,
  value,
  onChange,
  options,
  searchLabel,
  noMatchesLabel,
}: AtlasSelectProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const listboxId = `atlas-select-${useId().replace(/:/g, "")}`;
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);
  const [popupAlign, setPopupAlign] = useState<PopupAlign>("start");

  const selectedOption = options.find((option) => option.value === value);
  const matches = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    if (!normalizedQuery) return options;
    return options.filter((option) => option.label.toLocaleLowerCase().includes(normalizedQuery));
  }, [options, query]);

  const openMenu = () => {
    const selectedIndex = matches.findIndex((option) => option.value === value);
    setActiveIndex(selectedIndex >= 0 ? selectedIndex : 0);
    setQuery("");
    setOpen(true);
  };

  const closeMenu = (restoreFocus = true) => {
    setOpen(false);
    setQuery("");
    if (restoreFocus) triggerRef.current?.focus();
  };

  const choose = (option: AtlasSelectOption) => {
    onChange(option.value);
    closeMenu();
  };

  useEffect(() => {
    if (!open) return;
    searchRef.current?.focus();
    const measure = () => {
      const rect = rootRef.current?.getBoundingClientRect();
      if (!rect) return;
      setPopupAlign(rect.left + Math.min(280, window.innerWidth - 24) > window.innerWidth - 12 ? "end" : "start");
    };
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const dismissOutside = (event: MouseEvent | PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) closeMenu(false);
    };
    document.addEventListener("pointerdown", dismissOutside);
    document.addEventListener("mousedown", dismissOutside);
    return () => {
      document.removeEventListener("pointerdown", dismissOutside);
      document.removeEventListener("mousedown", dismissOutside);
    };
  }, [open]);

  useEffect(() => {
    if (matches.length === 0) {
      setActiveIndex(0);
    } else {
      setActiveIndex((index) => Math.min(index, matches.length - 1));
    }
  }, [matches.length]);

  useEffect(() => {
    if (open) document.getElementById(optionId(listboxId, activeIndex))?.scrollIntoView?.({ block: "nearest" });
  }, [open, activeIndex, listboxId]);

  const handleTriggerKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.key === "ArrowDown" || event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (!open) openMenu();
    } else if (event.key === "Escape" && open) {
      event.preventDefault();
      event.stopPropagation();
      closeMenu();
    }
  };

  const handleSearchChange = (event: ChangeEvent<HTMLInputElement>) => {
    setQuery(event.target.value);
    setActiveIndex(0);
  };

  const handleSearchKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActiveIndex((index) => (matches.length ? (index + 1) % matches.length : 0));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActiveIndex((index) => (matches.length ? (index - 1 + matches.length) % matches.length : 0));
    } else if (event.key === "Home") {
      event.preventDefault();
      setActiveIndex(0);
    } else if (event.key === "End") {
      event.preventDefault();
      setActiveIndex(Math.max(0, matches.length - 1));
    } else if (event.key === "Enter") {
      event.preventDefault();
      const option = matches[activeIndex];
      if (option) choose(option);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      closeMenu();
    }
  };

  return (
    <div ref={rootRef} className={`atlas-select atlas-select--${popupAlign}`}>
      <button
        ref={triggerRef}
        type="button"
        className="atlas-select-trigger"
        role="combobox"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listboxId : undefined}
        aria-activedescendant={open && matches[activeIndex] ? optionId(listboxId, activeIndex) : undefined}
        onClick={() => (open ? closeMenu() : openMenu())}
        onKeyDown={handleTriggerKeyDown}
      >
        <span className={selectedOption ? "atlas-select-value" : "atlas-select-placeholder"}>
          {selectedOption?.label ?? value}
        </span>
        <CaretDown size={15} aria-hidden="true" />
      </button>
      {open && (
        <div className="atlas-select-popup">
          <input
            ref={searchRef}
            type="text"
            className="atlas-select-search"
            role="combobox"
            aria-expanded={true}
            aria-controls={listboxId}
            aria-autocomplete="list"
            aria-activedescendant={matches[activeIndex] ? optionId(listboxId, activeIndex) : undefined}
            aria-label={searchLabel}
            placeholder={searchLabel}
            value={query}
            onChange={handleSearchChange}
            onKeyDown={handleSearchKeyDown}
            autoComplete="off"
          />
          <div id={listboxId} className="atlas-select-list" role="listbox" aria-label={label}>
            {matches.length > 0 ? matches.map((option, index) => (
              <button
                key={option.value}
                id={optionId(listboxId, index)}
                type="button"
                className={`atlas-select-option${index === activeIndex ? " is-active" : ""}`}
                role="option"
                aria-selected={option.value === value}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => choose(option)}
                onMouseEnter={() => setActiveIndex(index)}
              >
                <span>{option.label}</span>
                <Check className="atlas-select-check" size={15} weight="bold" aria-hidden="true" />
              </button>
            )) : (
              <div className="atlas-select-empty" role="status">{noMatchesLabel}</div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
