import { useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import type { Space } from "../../../lib/tauri";
import type { SpacesOverviewLabels } from "./spacesTypes";

type SpaceActionsMenuProps = {
  readonly space: Space;
  readonly labels: SpacesOverviewLabels;
  readonly pending: boolean;
  readonly canMoveUp: boolean;
  readonly canMoveDown: boolean;
  readonly onStar: (space: Space) => void;
  /** Starts the inline rename editor. */
  readonly onRename: (space: Space) => void;
  readonly onMoveUp: (space: Space) => void;
  readonly onMoveDown: (space: Space) => void;
  /** Starts the delete confirmation. */
  readonly onDelete: (space: Space) => void;
};

/**
 * The inventory actions trigger + popover shared by the rows and cards
 * lenses. The parent keeps its `spaces-menu-anchor` wrapper so the popover
 * positions the same way in both lenses.
 */
export function SpaceActionsMenu(props: SpaceActionsMenuProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);

  const closeMenuOnEscape = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    setMenuOpen(false);
    triggerRef.current?.focus();
  };

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        className="mem-icon-action spaces-menu-trigger"
        aria-label={props.labels.actionsFor(props.space.name)}
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        disabled={props.pending}
        onClick={() => setMenuOpen((open) => !open)}
      >
        <svg aria-hidden="true" width="16" height="4" viewBox="0 0 16 4" fill="currentColor">
          <circle cx="2" cy="2" r="1.5" /><circle cx="8" cy="2" r="1.5" /><circle cx="14" cy="2" r="1.5" />
        </svg>
      </button>
      {menuOpen ? (
        <div className="mem-popover-surface spaces-menu" role="menu" onKeyDown={closeMenuOnEscape}>
          <button role="menuitem" onClick={() => { props.onStar(props.space); setMenuOpen(false); }}>
            {props.space.starred ? props.labels.unstar : props.labels.star}
          </button>
          <button role="menuitem" onClick={() => { props.onRename(props.space); setMenuOpen(false); }}>{props.labels.rename}</button>
          <button role="menuitem" disabled={!props.canMoveUp} onClick={() => { props.onMoveUp(props.space); setMenuOpen(false); }}>
            {props.labels.moveUp}
          </button>
          <button role="menuitem" disabled={!props.canMoveDown} onClick={() => { props.onMoveDown(props.space); setMenuOpen(false); }}>
            {props.labels.moveDown}
          </button>
          <button role="menuitem" className="spaces-danger" onClick={() => { props.onDelete(props.space); setMenuOpen(false); }}>
            {props.labels.delete}
          </button>
        </div>
      ) : null}
    </>
  );
}
