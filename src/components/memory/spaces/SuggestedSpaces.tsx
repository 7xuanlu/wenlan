import { useEffect, useRef } from "react";
import type { Space } from "../../../lib/tauri";
import type { SpacesOverviewLabels } from "./spacesTypes";

type SuggestedSpacesProps = {
  readonly spaces: readonly Space[];
  readonly labels: SpacesOverviewLabels;
  readonly pendingIds: readonly string[];
  readonly onSelect: (spaceName: string) => void;
  readonly onKeep: (space: Space) => void;
  readonly onDiscard: (space: Space) => void;
};

export function SuggestedSpaces(props: SuggestedSpacesProps) {
  const disclosure = useRef<HTMLDetailsElement>(null);
  useEffect(() => {
    const closeOutside = (event: PointerEvent) => {
      if (event.target instanceof Node && !disclosure.current?.contains(event.target)) {
        disclosure.current?.removeAttribute("open");
      }
    };
    document.addEventListener("pointerdown", closeOutside);
    return () => document.removeEventListener("pointerdown", closeOutside);
  }, []);
  if (props.spaces.length === 0) return null;

  return (
    <details ref={disclosure} className="spaces-suggestions" onKeyDown={(event) => {
      if (event.key === "Escape") {
        event.stopPropagation();
        disclosure.current?.removeAttribute("open");
        disclosure.current?.querySelector("summary")?.focus();
      }
    }}>
      <summary id="suggested-spaces-heading">
        {props.labels.suggestedHeading} ({props.spaces.length})
      </summary>
      <div className="spaces-rows">
        {props.spaces.map((space) => {
          const pending = props.pendingIds.includes(space.id);
          return (
            <div
              className="spaces-row spaces-row-suggested"
              data-testid={`space-row-${space.id}`}
              aria-busy={pending}
              key={space.id}
            >
              <button className="spaces-row-main" aria-label={space.name} onClick={() => props.onSelect(space.name)}>
                <span className="spaces-row-name">{space.name}</span>
                {space.description === null ? null : <span className="spaces-row-description">{space.description}</span>}
              </button>
              <div className="spaces-row-decisions">
                <button
                  type="button"
                  disabled={pending}
                  className="spaces-suggestion-action spaces-suggestion-keep"
                  onClick={() => props.onKeep(space)}
                >
                  {props.labels.keep}
                </button>
                <button
                  type="button"
                  disabled={pending}
                  className="spaces-suggestion-action spaces-suggestion-discard"
                  onClick={() => props.onDiscard(space)}
                >
                  {props.labels.discard}
                </button>
              </div>
            </div>
          );
        })}
      </div>
    </details>
  );
}
