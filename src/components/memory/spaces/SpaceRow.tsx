import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { PointerEvent as ReactPointerEvent } from "react";
import type { Space } from "../../../lib/tauri";
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
  readonly onDragStart: (event: ReactPointerEvent<HTMLButtonElement>) => void;
  readonly dragDisabled?: boolean;
  readonly dragActive?: boolean;
};

export function SpaceRow(props: SpaceRowProps) {
  const { t } = useTranslation();
  const [renaming, setRenaming] = useState(false);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

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
    <div className={`spaces-row${props.dragActive === true ? " spaces-row-dragging" : ""}`} data-testid={`space-row-${props.space.id}`} aria-busy={props.pending}>
      <button
        type="button"
        className="mem-icon-action spaces-drag-handle"
        data-space-column="drag"
        aria-label={props.labels.dragSpace(props.space.name)}
        title={props.labels.dragSpace(props.space.name)}
        disabled={props.dragDisabled === true}
        onPointerDown={(event) => {
          props.onDragStart(event);
        }}
        onKeyDown={(event) => {
          if (event.key === "ArrowUp" && props.canMoveUp) { event.preventDefault(); props.onMoveUp(props.space); }
          if (event.key === "ArrowDown" && props.canMoveDown) { event.preventDefault(); props.onMoveDown(props.space); }
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
      <span className="spaces-row-pages" data-space-column="pages" data-testid="space-pages">{t("spaces.overview.card.pages", { count: props.pageCount })}</span>
      <div className="spaces-mobile-metadata" data-testid="space-mobile-metadata">
        <div data-testid="space-mobile-pages">
          <span>{t("spaces.overview.card.pages", { count: props.pageCount })}</span>
        </div>
      </div>
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
