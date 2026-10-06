// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_VISIBLE_NAVIGATION, NAVIGATION_PREFERENCE_KEY, readNavigationPreferences, sanitizeVisibleNavigation, writeNavigationPreferences } from "./navigationPreferences";

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

describe("navigation preferences", () => {
  it("defaults when no preference is saved", () => {
    expect(readNavigationPreferences()).toEqual(DEFAULT_VISIBLE_NAVIGATION);
    expect(localStorage.getItem(NAVIGATION_PREFERENCE_KEY)).toBeNull();
  });

  it.each([
    "{", "null", "[]", '{}', '{"version":2,"visible":["home"]}',
    '{"version":1,"visible":"home"}', '{"version":1,"visible":["unknown",null,3]}',
  ])("defaults for malformed or unsupported data %s", (payload) => {
    localStorage.setItem(NAVIGATION_PREFERENCE_KEY, payload);
    expect(readNavigationPreferences()).toEqual(DEFAULT_VISIBLE_NAVIGATION);
  });

  it("preserves a deliberately empty list", () => {
    writeNavigationPreferences([]);
    expect(readNavigationPreferences()).toEqual([]);
  });

  it("ignores legacy Home pins while preserving other destinations", () => {
    localStorage.setItem(NAVIGATION_PREFERENCE_KEY, JSON.stringify({ version: 1, visible: ["home", "sources", "home", "memories"] }));
    expect(readNavigationPreferences()).toEqual(["sources", "memories"]);
    expect(sanitizeVisibleNavigation(["home", "pages", "home"])).toEqual(["pages"]);
  });

  it("restores defaults when the only saved destination was Home", () => {
    localStorage.setItem(NAVIGATION_PREFERENCE_KEY, JSON.stringify({ version: 1, visible: ["home"] }));
    expect(readNavigationPreferences()).toEqual(DEFAULT_VISIBLE_NAVIGATION);
  });

  it("keeps known unique keys in canonical order", () => {
    localStorage.setItem(NAVIGATION_PREFERENCE_KEY, JSON.stringify({ version: 1, visible: ["home", "unknown", "spaces", "home", "pages"] }));
    expect(readNavigationPreferences()).toEqual(["pages", "spaces"]);
    expect(sanitizeVisibleNavigation(["entities", "graph", "graph", null])).toEqual(["graph", "entities"]);
    writeNavigationPreferences(["entities", "sources", "sources", "pages"]);
    expect(JSON.parse(localStorage.getItem(NAVIGATION_PREFERENCE_KEY)!)).toEqual({ version: 1, visible: ["pages", "sources", "entities"] });
  });

  it("continues when storage cannot be accessed or written", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("disabled"); });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("disabled"); });
    expect(readNavigationPreferences()).toEqual(DEFAULT_VISIBLE_NAVIGATION);
    expect(() => writeNavigationPreferences(["sources"])).not.toThrow();
  });
});
