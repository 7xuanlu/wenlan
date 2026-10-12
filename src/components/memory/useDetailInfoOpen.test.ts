// SPDX-License-Identifier: AGPL-3.0-only
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useDetailInfoOpen } from "./useDetailInfoOpen";

const KEY = "wenlan.memory-info-open.v1";

function installMatchMedia(initial: boolean) {
  let matches = initial;
  const listeners = new Set<(event: MediaQueryListEvent) => void>();
  const mediaQuery = {
    get matches() { return matches; },
    media: "(min-width: 1100px)",
    onchange: null,
    addEventListener: (_type: string, listener: (event: MediaQueryListEvent) => void) => listeners.add(listener),
    removeEventListener: (_type: string, listener: (event: MediaQueryListEvent) => void) => listeners.delete(listener),
    dispatchEvent: () => true,
  } as unknown as MediaQueryList;
  vi.stubGlobal("matchMedia", vi.fn(() => mediaQuery));
  return (next: boolean) => {
    matches = next;
    const event = { matches: next, media: mediaQuery.media } as MediaQueryListEvent;
    act(() => listeners.forEach(listener => listener(event)));
  };
}

describe("useDetailInfoOpen", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("starts open for every desktop entry and ignores the legacy preference", () => {
    installMatchMedia(true);
    window.localStorage.setItem(KEY, "false");
    const getItem = vi.spyOn(Storage.prototype, "getItem");
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    const { result, rerender, unmount } = renderHook(({ entryId }) => useDetailInfoOpen(entryId), { initialProps: { entryId: "memory-1" } });

    expect(result.current.open).toBe(true);
    expect(getItem).not.toHaveBeenCalled();
    expect(setItem).not.toHaveBeenCalled();
    act(() => result.current.toggle());
    expect(result.current.open).toBe(false);
    expect(setItem).not.toHaveBeenCalled();

    rerender({ entryId: "memory-2" });
    expect(result.current.open).toBe(true);
    act(() => result.current.setOpen(false));
    rerender({ entryId: "memory-3" });
    expect(result.current.open).toBe(true);

    unmount();
    const remounted = renderHook(() => useDetailInfoOpen("memory-3"));
    expect(remounted.result.current.open).toBe(true);
  });

  it("preserves a desktop choice for the same entry across resize and resets narrow overlay state", () => {
    const resize = installMatchMedia(true);
    const { result } = renderHook(() => useDetailInfoOpen("memory-1"));

    act(() => result.current.setOpen(false));
    act(() => resize(false));
    expect(result.current.open).toBe(false);
    act(() => result.current.setOpen(true));
    expect(result.current.open).toBe(true);
    act(() => resize(true));
    expect(result.current.open).toBe(false);
    act(() => resize(false));
    expect(result.current.open).toBe(false);
  });

  it("starts closed below desktop width and can be opened manually", () => {
    installMatchMedia(false);
    const { result } = renderHook(() => useDetailInfoOpen("memory-1"));

    expect(result.current.open).toBe(false);
    act(() => result.current.setOpen(true));
    expect(result.current.open).toBe(true);
  });
});
