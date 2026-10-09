// SPDX-License-Identifier: AGPL-3.0-only
import { createContext, useContext, type ReactNode } from "react";

export interface ReferenceNavigation {
  onOpenPage?: (pageId: string) => void;
  onOpenMemory?: (sourceId: string) => void;
}

const ReferenceNavigationContext = createContext<ReferenceNavigation | null>(null);

export function ReferenceNavigationProvider({
  children,
  onOpenPage,
  onOpenMemory,
}: ReferenceNavigation & { children: ReactNode }) {
  return (
    <ReferenceNavigationContext.Provider value={{ onOpenPage, onOpenMemory }}>
      {children}
    </ReferenceNavigationContext.Provider>
  );
}

export function useReferenceNavigation(): ReferenceNavigation {
  return useContext(ReferenceNavigationContext) ?? {};
}
