// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { GearSix, Info, User } from "@phosphor-icons/react";
import { getProfile } from "../../lib/tauri";
import ProfileAvatar from "./ProfileAvatar";
import "./IdentityCard.css";

interface IdentityCardProps {
  onOpenDetail: (entityId: string) => void;
  onOpenSettings?: () => void;
  onOpenAbout?: () => void;
}

export default function IdentityCard({ onOpenDetail, onOpenSettings, onOpenAbout }: IdentityCardProps) {
  const { t } = useTranslation();
  const [menuOpen, setMenuOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const { data: profile } = useQuery({ queryKey: ["profile"], queryFn: getProfile });
  const displayName = profile?.display_name || profile?.name || "";
  const labelText = displayName || t("identityCard.account");
  const triggerLabel = displayName
    ? t("identityCard.namedAccountMenu", { name: displayName })
    : t("identityCard.accountMenu");

  useEffect(() => {
    if (!menuOpen) return;
    menuRef.current?.querySelector<HTMLButtonElement>("button")?.focus();
    const closeOutside = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setMenuOpen(false);
    };
    document.addEventListener("pointerdown", closeOutside);
    return () => document.removeEventListener("pointerdown", closeOutside);
  }, [menuOpen]);

  return (
    <div
      className="identity-rail-anchor"
      data-sidebar-escape-scope={menuOpen ? "true" : undefined}
      onKeyDown={(event) => {
        if (event.key !== "Escape" || !menuOpen) return;
        event.preventDefault();
        event.stopPropagation();
        setMenuOpen(false);
        triggerRef.current?.focus();
      }}
      ref={rootRef}
    >
      <button
        aria-label={triggerLabel}
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        className="notes-rail-button identity-menu-trigger"
        onClick={() => setMenuOpen((open) => !open)}
        ref={triggerRef}
        title={labelText}
        type="button"
      >
        {displayName ? (
          <ProfileAvatar avatarPath={profile?.avatar_path} displayName={displayName} size={22} fontSize={9} tone="muted" />
        ) : (
          <span aria-hidden="true" className="notes-navigation-glyph"><User /></span>
        )}
      </button>
      {menuOpen && (
        <div
          aria-label={triggerLabel}
          className="identity-rail-menu"
          onKeyDown={(event) => {
            if (event.key === "Tab") {
              setMenuOpen(false);
              return;
            }
            if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
            event.preventDefault();
            const items = Array.from(menuRef.current?.querySelectorAll<HTMLButtonElement>("button") ?? []);
            const current = items.indexOf(document.activeElement as HTMLButtonElement);
            const next = event.key === "Home" ? 0 : event.key === "End" ? items.length - 1
              : (current + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length;
            items[next]?.focus();
          }}
          ref={menuRef}
          role="menu"
        >
          <div className="identity-menu-name">{labelText}</div>
          <button
            className="notes-more-item"
            onClick={() => {
              setMenuOpen(false);
              if (onOpenSettings) onOpenSettings();
              else onOpenDetail("__create_profile__");
            }}
            role="menuitem"
            type="button"
          >
            <span aria-hidden="true" className="notes-navigation-glyph"><GearSix /></span>
            {t("identityCard.settings")}
          </button>
          <button
            className="notes-more-item"
            onClick={() => { setMenuOpen(false); onOpenAbout?.(); }}
            role="menuitem"
            type="button"
          >
            <span aria-hidden="true" className="notes-navigation-glyph"><Info /></span>
            {t("identityCard.aboutWenlan")}
          </button>
        </div>
      )}
    </div>
  );
}
