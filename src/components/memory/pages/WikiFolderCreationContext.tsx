// SPDX-License-Identifier: AGPL-3.0-only
import { createContext, useContext } from "react";

export interface WikiFolderCreationContextValue {
  readonly creationFolderPath: string | null;
  readonly reportCreationFolderPath: (path: string | null) => void;
  readonly canCreateFolder: boolean;
  readonly reportCanCreateFolder: (available: boolean) => void;
  readonly registerCreateFolderForm: (openForm: () => void) => () => void;
  readonly requestCreateFolder: () => boolean;
}

export const WikiFolderCreationContext = createContext<WikiFolderCreationContextValue | null>(null);

export function useWikiFolderCreation(): WikiFolderCreationContextValue | null {
  return useContext(WikiFolderCreationContext);
}
