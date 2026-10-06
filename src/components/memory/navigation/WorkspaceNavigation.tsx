// SPDX-License-Identifier: AGPL-3.0-only
import { createContext, useContext, type ComponentProps, type ReactNode } from "react";

const WorkspaceNavigationContext = createContext(false);

/** The workspace shell owns Back/Forward; standalone details keep their Back. */
export function WorkspaceNavigationProvider({ children }: { children: ReactNode }) {
  return <WorkspaceNavigationContext.Provider value={true}>{children}</WorkspaceNavigationContext.Provider>;
}

export function WorkspaceBackButton(props: ComponentProps<"button">) {
  const inWorkspace = useContext(WorkspaceNavigationContext);
  return inWorkspace ? null : <button {...props} />;
}
