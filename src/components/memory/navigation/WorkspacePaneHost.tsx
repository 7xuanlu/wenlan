// SPDX-License-Identifier: AGPL-3.0-only
import { createContext } from "react";

/** The optional right-side workspace column reserved for nonmodal panes. */
export const WorkspacePaneHostContext = createContext<HTMLElement | null>(null);
