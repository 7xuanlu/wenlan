// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from "vitest";
import {
  overviewLabelMaxWidth,
  selectOverviewFallbackLabels,
  truncateOverviewLabel,
} from "./overviewLabels";

describe("overview label sizing", () => {
  const measured = {
    measureText: (text: string) => ({ width: Array.from(text).length * 10 }),
  } as unknown as Pick<CanvasRenderingContext2D, "measureText">;

  it("keeps a label intact when its measured width fits", () => {
    expect(truncateOverviewLabel(measured, "Atlas", 60)).toBe("Atlas");
  });

  it("truncates by measured canvas width and preserves the ellipsis", () => {
    expect(truncateOverviewLabel(measured, "Atlas network", 60)).toBe("Atlas…");
    expect(measured.measureText(truncateOverviewLabel(measured, "Atlas network", 60)).width).toBeLessThanOrEqual(60);
  });

  it("uses a smaller readable budget on narrow viewports", () => {
    expect(overviewLabelMaxWidth(1200)).toBe(160);
    expect(overviewLabelMaxWidth(320)).toBe(134.4);
  });

  it("caps fallback names while distributing them by stable positions", () => {
    const candidates = Array.from({ length: 20 }, (_, index) => ({
      id: `node-${String(index).padStart(2, "0")}`,
      x: index % 5,
      y: Math.floor(index / 5),
    }));
    const first = selectOverviewFallbackLabels(candidates, 12);
    const second = selectOverviewFallbackLabels([...candidates].reverse(), 12);

    expect(first.size).toBe(12);
    expect(second).toEqual(first);
    expect([...first].some((id) => id === "node-19")).toBe(true);
  });
});
