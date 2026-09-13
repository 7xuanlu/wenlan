// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import {
  ACTIVITY_NOW_LAYOUTS,
  ACTIVITY_RAIL_MIN_WIDTH,
  __resetActivityNowLayoutForTests,
  getActivityNowLayout,
  setActivityNowLayout,
  useActivityNowLayout,
} from "./activityNowLayout";

const STORAGE_KEY = "wenlan-activity-now-layout";

beforeEach(() => {
  localStorage.clear();
  __resetActivityNowLayoutForTests();
});

describe("activityNowLayout", () => {
  it("defaults to the rail", () => {
    expect(getActivityNowLayout()).toBe("rail");
  });

  it("falls back to the rail for an unknown stored value", () => {
    // A hand-edited or stale preference must not leave the page with no layout.
    localStorage.setItem(STORAGE_KEY, "sidecar");
    __resetActivityNowLayoutForTests();
    expect(getActivityNowLayout()).toBe("rail");
  });

  it("restores a stored layout", () => {
    localStorage.setItem(STORAGE_KEY, "timeline");
    __resetActivityNowLayoutForTests();
    expect(getActivityNowLayout()).toBe("timeline");
  });

  it("persists a change", () => {
    setActivityNowLayout("card");
    expect(localStorage.getItem(STORAGE_KEY)).toBe("card");
    expect(getActivityNowLayout()).toBe("card");
  });

  it("repaints subscribers when Settings changes the layout", () => {
    const { result } = renderHook(() => useActivityNowLayout());
    expect(result.current[0]).toBe("rail");

    act(() => {
      setActivityNowLayout("timeline");
    });
    expect(result.current[0]).toBe("timeline");
  });

  it("does not notify when the layout is unchanged", () => {
    const { result } = renderHook(() => useActivityNowLayout());
    const spy = vi.spyOn(Storage.prototype, "setItem");

    act(() => {
      result.current[1]("rail");
    });

    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });

  it("offers exactly the three layouts the settings control renders", () => {
    expect([...ACTIVITY_NOW_LAYOUTS]).toEqual(["rail", "card", "timeline"]);
    expect(ACTIVITY_RAIL_MIN_WIDTH).toBe(900);
  });
});
