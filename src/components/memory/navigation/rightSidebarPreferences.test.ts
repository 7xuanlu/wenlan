// SPDX-License-Identifier: AGPL-3.0-only
import { afterEach, describe, expect, it, vi } from "vitest";
import { readRightSidebarWidth, RIGHT_SIDEBAR_PREFERENCE_KEY, rightSidebarPreferenceKey, sanitizeRightSidebarWidth, writeRightSidebarWidth } from "./rightSidebarPreferences";

afterEach(() => {
  localStorage.removeItem(RIGHT_SIDEBAR_PREFERENCE_KEY);
  localStorage.removeItem(rightSidebarPreferenceKey("left"));
  localStorage.removeItem(rightSidebarPreferenceKey("right"));
  vi.restoreAllMocks();
});

describe("right sidebar width preference", () => {
  it("sanitizes valid versioned widths and ignores invalid payloads", () => {
    expect(sanitizeRightSidebarWidth(312.4)).toBe(320);
    expect(sanitizeRightSidebarWidth(421.6)).toBe(422);
    expect(sanitizeRightSidebarWidth(700)).toBe(600);
    expect(sanitizeRightSidebarWidth(Number.NaN)).toBeUndefined();
    localStorage.setItem(RIGHT_SIDEBAR_PREFERENCE_KEY, JSON.stringify({ version: 2, width: 420 }));
    expect(readRightSidebarWidth()).toBeUndefined();
    localStorage.setItem(RIGHT_SIDEBAR_PREFERENCE_KEY, JSON.stringify({ version: 1, width: 420 }));
    expect(readRightSidebarWidth()).toBe(420);
  });

  it("writes only an explicit committed width under its unique versioned key", () => {
    const spy = vi.spyOn(Storage.prototype, "setItem");
    writeRightSidebarWidth(481);
    expect(spy).toHaveBeenCalledOnce();
    expect(spy).toHaveBeenCalledWith(RIGHT_SIDEBAR_PREFERENCE_KEY, JSON.stringify({ version: 1, width: 481 }));
    expect(readRightSidebarWidth()).toBe(481);
  });

  it("keeps widths independent by group and reads the legacy width as a fallback", () => {
    localStorage.setItem(RIGHT_SIDEBAR_PREFERENCE_KEY, JSON.stringify({ version: 1, width: 410 }));
    expect(readRightSidebarWidth("left")).toBe(410);
    writeRightSidebarWidth(360, "left");
    writeRightSidebarWidth(520, "right");
    expect(readRightSidebarWidth("left")).toBe(360);
    expect(readRightSidebarWidth("right")).toBe(520);
    expect(readRightSidebarWidth()).toBe(410);
    expect(localStorage.getItem(RIGHT_SIDEBAR_PREFERENCE_KEY)).toBe(JSON.stringify({ version: 1, width: 410 }));
    expect(rightSidebarPreferenceKey("left")).toBe(`${RIGHT_SIDEBAR_PREFERENCE_KEY}:left`);
  });
});
