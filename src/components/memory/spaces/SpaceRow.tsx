import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Space } from "../../../lib/tauri";
import { formatLocaleDate } from "../../../lib/dateFormat";
import { SpaceActionsMenu } from "./SpaceActionsMenu";
import { SpaceEditor } from "./SpaceEditor";
import type { SpacesOverviewLabels, SpaceEditorValue } from "./spacesTypes";

type SpaceRowProps = {
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
  readonly onDragStart: (space: Space, pointerId: number) => void;
};

export function SpaceRow(props: SpaceRowProps) {
  const { i18n } = useTranslation();
  const [renaming, setRenaming] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const updated = props.space.updated_at > 0
    ? formatLocaleDate(new Date(props.space.updated_at * 1000), i18n.language)
    : { label: "—" };

  if (renaming) {
    return (
      <div className="spaces-row spaces-row-edit" data-testid={`space-row-${props.space.id}`}>
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
    <div className="spaces-row" data-testid={`space-row-${props.space.id}`} aria-busy={props.pending}>
      <button
        type="button"
        className="mem-icon-action spaces-drag-handle"
        data-space-column="drag"
        aria-label={props.labels.dragSpace(props.space.name)}
        onPointerDown={(event) => {
          event.preventDefault();
          props.onDragStart(props.space, event.pointerId);
        }}
      >
        <svg aria-hidden="true" width="8" height="14" viewBox="0 0 8 14" fill="currentColor">
          <circle cx="2" cy="2" r="1" /><circle cx="6" cy="2" r="1" />
          <circle cx="2" cy="7" r="1" /><circle cx="6" cy="7" r="1" />
          <circle cx="2" cy="12" r="1" /><circle cx="6" cy="12" r="1" />
        </svg>
      </button>
      <button
        className="spaces-row-main"
        data-space-column="name"
        aria-label={props.space.name}
        onClick={() => props.onSelect(props.space.name)}
      >
        <span className="spaces-row-name">
          {props.space.starred ? <span className="spaces-star" aria-hidden="true">★</span> : null}
          {props.space.name}
        </span>
        {props.space.description === null ? null : <span className="spaces-row-description">{props.space.description}</span>}
      </button>
      <span className="spaces-row-pages" data-space-column="pages" data-testid="space-pages">{props.pageCount}</span>
      <span className="spaces-row-count" data-space-column="memories" data-testid="space-memories">{props.space.memory_count}</span>
      <time className="spaces-row-updated" data-space-column="updated" data-testid="space-updated" dateTime={updated.dateTime}>
        {updated.label}
      </time>
      <dl className="spaces-mobile-metadata" data-testid="space-mobile-metadata">
        <div data-testid="space-mobile-pages">
          <dt>{props.labels.pages}</dt>
          <dd>{props.pageCount}</dd>
        </div>
        <div data-testid="space-mobile-memories">
          <dt>{props.labels.memories}</dt>
          <dd>{props.space.memory_count}</dd>
        </div>
        <div data-testid="space-mobile-updated">
          <dt>{props.labels.updated}</dt>
          <dd><time dateTime={updated.dateTime}>{updated.label}</time></dd>
        </div>
      </dl>
      <div className="spaces-menu-anchor" data-space-column="menu">
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
    </div>
  );
}
