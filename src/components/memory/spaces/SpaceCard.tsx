import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Space } from "../../../lib/tauri";
import { formatLocaleDate } from "../../../lib/dateFormat";
import { AssetCard } from "../assets/AssetCard";
import { SpaceMark } from "../navigation/SpaceMark";
import { SpaceActionsMenu } from "./SpaceActionsMenu";
import { SpaceEditor } from "./SpaceEditor";
import type { SpacesOverviewLabels, SpaceEditorValue } from "./spacesTypes";

type SpaceCardProps = {
  readonly space: Space;
  readonly spaces: readonly Space[];
  readonly labels: SpacesOverviewLabels;
  readonly pageCount: number;
  readonly pending: boolean;
  readonly canMoveUp: boolean;
  readonly canMoveDown: boolean;
  readonly onSelect: (name: string) => void;
  readonly onStar: (space: Space) => void;
  readonly onRename: (space: Space, value: SpaceEditorValue) => Promise<boolean>;
  readonly onMoveUp: (space: Space) => void;
  readonly onMoveDown: (space: Space) => void;
  readonly onDelete: (space: Space) => void;
};

export function SpaceCard(props: SpaceCardProps) {
  const { t, i18n } = useTranslation();
  const [renaming, setRenaming] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const updated = props.space.updated_at > 0
    ? formatLocaleDate(new Date(props.space.updated_at * 1000), i18n.language)
    : { label: "—" };

  if (renaming) {
    return (
      <div className="spaces-row spaces-row-edit space-card-edit" data-testid={`space-card-${props.space.id}`}>
        <SpaceEditor
          labels={props.labels}
          spaces={props.spaces}
          initialName={props.space.name}
          initialDescription={props.space.description ?? ""}
          showDescription={false}
          submitLabel={props.labels.save}
          pending={props.pending}
          onCancel={() => setRenaming(false)}
          onSubmit={async (value) => {
            if (await props.onRename(props.space, value)) setRenaming(false);
          }}
        />
      </div>
    );
  }

  return (
    <AssetCard
      testId={`space-card-${props.space.id}`}
      title={props.space.name}
      context={props.space.description}
      footer={(
        <>
          <span className="space-card-counts">
            <span data-testid="space-card-pages">{t("spaces.overview.card.pages", { count: props.pageCount })}</span>
            <span data-testid="space-card-memories">{t("spaces.overview.card.memories", { count: props.space.memory_count })}</span>
            <span data-testid="space-card-entities">{t("spaces.overview.card.entities", { count: props.space.entity_count })}</span>
          </span>
          <time data-testid="space-card-updated" dateTime={updated.dateTime}>{updated.label}</time>
        </>
      )}
      onOpen={() => props.onSelect(props.space.name)}
      openLabel={t("spaces.overview.card.open", { name: props.space.name })}
    >
      <div className="space-card-head">
        <span className="space-card-mark" aria-hidden="true">
          <SpaceMark active={props.space.starred} />
        </span>
        {props.space.starred ? <span className="spaces-star" aria-hidden="true">★</span> : null}
        <div className="spaces-menu-anchor">
          <SpaceActionsMenu
            space={props.space}
            labels={props.labels}
            pending={props.pending}
            canMoveUp={props.canMoveUp}
            canMoveDown={props.canMoveDown}
            onStar={props.onStar}
            onRename={() => setRenaming(true)}
            onMoveUp={props.onMoveUp}
            onMoveDown={props.onMoveDown}
            onDelete={() => setConfirmingDelete(true)}
          />
        </div>
      </div>
      {confirmingDelete ? (
        <div className="spaces-delete-confirmation">
          <button className="spaces-danger" disabled={props.pending} onClick={() => { props.onDelete(props.space); setConfirmingDelete(false); }}>
            {props.labels.confirmDelete}
          </button>
          <button className="spaces-quiet-action" disabled={props.pending} onClick={() => setConfirmingDelete(false)}>
            {props.labels.cancel}
          </button>
        </div>
      ) : null}
    </AssetCard>
  );
}
