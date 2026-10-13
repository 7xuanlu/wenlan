// SPDX-License-Identifier: AGPL-3.0-only
import { createContext, type RefObject } from "react";

/** The note group whose local workspace hosts own inspector panes. */
export interface WorkspaceNoteGroupContextValue {
  readonly id: string;
  readonly element: HTMLElement | null;
  /** Holds the open inspector's close action, so the workspace can make room. */
  readonly inspectorCloser?: { current: (() => void) | null };
}

export const WorkspaceNoteGroupContext = createContext<WorkspaceNoteGroupContextValue | null>(null);

/** The optional right-side workspace column reserved for nonmodal panes. */
export const WorkspacePaneHostContext = createContext<HTMLElement | null>(null);

/** Optional titlebar slot for the active document’s context toggle. */
export const WorkspaceDocumentToolsHostContext = createContext<HTMLElement | null>(null);

/** Main reading area for expanding a contextual source without losing its parent. */
export const WorkspaceReadingHostContext = createContext<RefObject<HTMLElement | null> | null>(null);
