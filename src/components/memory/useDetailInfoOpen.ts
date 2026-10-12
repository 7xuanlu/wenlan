// SPDX-License-Identifier: AGPL-3.0-only
import { useCallback, useEffect, useState } from "react";

const DESKTOP_QUERY = "(min-width: 1100px)";

function createMediaQueryList(): MediaQueryList | null {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return null;
  try {
    return window.matchMedia(DESKTOP_QUERY);
  } catch {
    return null;
  }
}

export function useDetailInfoOpen(entryId: string) {
  const [mediaQuery] = useState(createMediaQueryList);
  const [wide, setWide] = useState(() => mediaQuery?.matches ?? false);
  const [entryState, setEntryState] = useState(() => ({
    entryId,
    desktopOpen: true,
    narrowOpen: false,
  }));

  // A detail entry gets a fresh desktop default even when its owner reuses the
  // component instance. Updating during render avoids briefly showing the old
  // entry's choice before an effect runs.
  if (entryState.entryId !== entryId) {
    setEntryState({ entryId, desktopOpen: true, narrowOpen: false });
  }

  useEffect(() => {
    if (!mediaQuery) return;
    const handleChange = (event: MediaQueryListEvent) => {
      setWide(event.matches);
      setEntryState((current) => ({ ...current, narrowOpen: false }));
    };
    setWide(mediaQuery.matches);
    setEntryState((current) => ({ ...current, narrowOpen: false }));
    mediaQuery.addEventListener?.("change", handleChange);
    return () => mediaQuery.removeEventListener?.("change", handleChange);
  }, [mediaQuery]);

  const open = wide ? entryState.desktopOpen : entryState.narrowOpen;
  const setOpen = useCallback((next: boolean) => {
    setEntryState((current) => wide
      ? { ...current, desktopOpen: next }
      : { ...current, narrowOpen: next });
  }, [wide]);
  const toggle = useCallback(() => setOpen(!open), [open, setOpen]);

  return { open, setOpen, toggle };
}
