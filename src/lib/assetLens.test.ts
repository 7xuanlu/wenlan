// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  assetLensStorageKey,
  readAssetLens,
  writeAssetLens,
} from "./assetLens";

describe("assetLens", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("defaults to cards when nothing is stored", () => {
    expect(readAssetLens("wiki")).toBe("cards");
    expect(readAssetLens("entities")).toBe("cards");
    expect(readAssetLens("spaces")).toBe("cards");
  });

  it("defaults to cards for an unknown stored value", () => {
    window.localStorage.setItem("wenlan-wiki-view-mode", "grid");
    expect(readAssetLens("wiki")).toBe("cards");
  });

  it("reads the stored lens per page", () => {
    window.localStorage.setItem("wenlan-wiki-view-mode", "rows");
    window.localStorage.setItem("wenlan-entities-view-mode", "cards");
    expect(readAssetLens("wiki")).toBe("rows");
    expect(readAssetLens("entities")).toBe("cards");
    expect(readAssetLens("spaces")).toBe("cards");
  });

  it("writes the lens under the page key", () => {
    writeAssetLens("wiki", "rows");
    expect(window.localStorage.getItem("wenlan-wiki-view-mode")).toBe("rows");
    writeAssetLens("wiki", "cards");
    expect(window.localStorage.getItem("wenlan-wiki-view-mode")).toBe("cards");
  });

  it("uses one key per page", () => {
    expect(assetLensStorageKey("wiki")).toBe("wenlan-wiki-view-mode");
    expect(assetLensStorageKey("entities")).toBe("wenlan-entities-view-mode");
    expect(assetLensStorageKey("spaces")).toBe("wenlan-spaces-view-mode");
  });

  it("survives localStorage throwing", () => {
    const getItem = vi
      .spyOn(Storage.prototype, "getItem")
      .mockImplementation(() => {
        throw new Error("storage unavailable");
      });
    const setItem = vi
      .spyOn(Storage.prototype, "setItem")
      .mockImplementation(() => {
        throw new Error("storage unavailable");
      });
    try {
      expect(readAssetLens("wiki")).toBe("cards");
      expect(() => writeAssetLens("wiki", "rows")).not.toThrow();
    } finally {
      getItem.mockRestore();
      setItem.mockRestore();
    }
  });
});
