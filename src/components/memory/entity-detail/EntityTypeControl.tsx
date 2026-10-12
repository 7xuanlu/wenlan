// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import "./EntityTypeControl.css";

const ENTITY_TYPES = ["concept", "person", "organization", "place", "project", "technology"] as const;

type EntityTypeControlProps = {
  readonly value: string;
  readonly archived: boolean;
  readonly pending: boolean;
  readonly onChange: (value: string) => Promise<boolean>;
};

export function EntityTypeControl({ value, archived, pending, onChange }: EntityTypeControlProps) {
  const { t } = useTranslation();
  const [selection, setSelection] = useState(value);
  const [saveFailed, setSaveFailed] = useState(false);

  useEffect(() => {
    setSelection(value);
  }, [value]);

  const save = async (nextValue: string) => {
    setSelection(nextValue);
    setSaveFailed(false);
    try {
      const saved = await onChange(nextValue);
      if (!saved) setSelection(value);
    } catch {
      // Keep the requested value selected so the user can retry it in place.
      setSaveFailed(true);
    }
  };

  const currentTypeIsUnknown = !ENTITY_TYPES.includes(value as (typeof ENTITY_TYPES)[number]);

  return (
    <div className="entity-type-control">
      <select
        className="entity-type-select"
        aria-label={t("entityDetail.typeLabel")}
        value={selection}
        disabled={archived || pending}
        onChange={(event) => {
          if (event.currentTarget.value !== selection) void save(event.currentTarget.value);
        }}
      >
        {currentTypeIsUnknown ? (
          <option value={value}>
            {humanizeType(value)} {t("entityDetail.typeCurrent")}
          </option>
        ) : null}
        {ENTITY_TYPES.map((type) => (
          <option key={type} value={type}>{t(`atlas.entityType.${type}`)}</option>
        ))}
      </select>
      {saveFailed ? (
        <span className="entity-type-save-error" role="alert">
          <span>{t("entityDetail.typeSaveError")}</span>
          <button type="button" className="memory-detail-text-button" disabled={pending} onClick={() => { void save(selection); }}>
            {t("entityDetail.retry")}
          </button>
        </span>
      ) : null}
    </div>
  );
}

function humanizeType(value: string): string {
  const words = value.trim().replace(/[_-]+/gu, " ").replace(/\s+/gu, " ");
  return words.replace(/(^|\s)\p{L}/gu, (character) => character.toLocaleUpperCase());
}
