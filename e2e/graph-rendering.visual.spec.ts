// SPDX-License-Identifier: AGPL-3.0-only
import { expect, test, type Locator } from "@playwright/test";
import { collectBrowserErrors, installTauriMock } from "./tauriMock";

type CanvasEvidence = {
  coloredPixels: number;
  orangeCoverage: number;
  sampledPixels: number;
  uniqueColors: number;
};

test("renders Graph as a structured canvas instead of a flat orange field", async ({ page }) => {
  const browserErrors = collectBrowserErrors(page);
  await page.setViewportSize({ width: 1280, height: 900 });
  await installTauriMock(page, {
    locale: "en",
    localStorage: { "wenlan-theme": "light" },
    rawActions: [],
  });
  await page.goto("/");

  await page
    .getByRole("navigation", { name: "Primary navigation" })
    .getByRole("button", { name: "Graph", exact: true })
    .click();

  const graph = page.getByTestId("atlas-view");
  await expect(graph).toBeVisible();
  // Pages lead the line now, and the counts are over what is actually drawn:
  // seven wiki pages plus the three entities that have a connection.
  await expect(page.getByText(/^7 pages · 3 entities(?: · \d+ regions?)?$/)).toBeVisible();

  // Regions stay quiet by default. Names and contours have separate transparent
  // canvases, above and below Sigma respectively, and become visible together.
  const ours = graph.locator('canvas:not([class*="sigma-"])');
  await expect(ours).toHaveCount(2);
  const canvas = graph.getByTestId("atlas-region-names");
  const areas = graph.getByTestId("atlas-community-areas");
  const regions = page.getByRole("button", { name: "Regions", exact: true });
  await expect(regions).toHaveAttribute("aria-pressed", "false");
  await expect(canvas).toBeHidden();
  await expect(areas).toBeHidden();
  await expect(page.getByRole("group", { name: "Show in graph" })
    .getByRole("button", { name: "Memories", exact: true })).toHaveAttribute("aria-pressed", "false");
  await regions.click();
  await expect(regions).toHaveAttribute("aria-pressed", "true");
  await expect(canvas).toBeVisible();
  await expect(areas).toBeVisible();
  await expect(canvas).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
  await expect(areas).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");

  const readOverlay = (canvas: Locator): Promise<CanvasEvidence> =>
    canvas.evaluate((node): CanvasEvidence => {
      if (!(node instanceof HTMLCanvasElement)) {
        return { coloredPixels: 0, orangeCoverage: 1, sampledPixels: 0, uniqueColors: 0 };
      }
      const context = node.getContext("2d", { willReadFrequently: true });
      if (!context) {
        return { coloredPixels: 0, orangeCoverage: 1, sampledPixels: 0, uniqueColors: 0 };
      }
      const pixels = context.getImageData(0, 0, node.width, node.height).data;
      const colors = new Set<string>();
      let coloredPixels = 0;
      let orangePixels = 0;
      let sampledPixels = 0;
      for (let y = 0; y < node.height; y += 2) {
        for (let x = 0; x < node.width; x += 2) {
          sampledPixels += 1;
          const offset = (y * node.width + x) * 4;
          const red = pixels[offset] ?? 0;
          const green = pixels[offset + 1] ?? 0;
          const blue = pixels[offset + 2] ?? 0;
          const alpha = pixels[offset + 3] ?? 0;
          if (alpha === 0) continue;
          coloredPixels += 1;
          // Raw alpha in the key: anti-aliased text shows many alpha steps
          // where a flat fill shows one.
          colors.add(`${red >> 4}:${green >> 4}:${blue >> 4}:${alpha}`);
          if (red > 170 && green > 55 && green < 175 && blue < 100) {
            orangePixels += 1;
          }
        }
      }
      return {
        coloredPixels,
        orangeCoverage: sampledPixels === 0 ? 1 : orangePixels / sampledPixels,
        sampledPixels,
        uniqueColors: colors.size,
      };
    });

  // The fixture has one community. Verify it actually paints a contour without
  // recreating the old orange flood. Names may yield to a visible hub label.
  let evidence: CanvasEvidence = { coloredPixels: 0, orangeCoverage: 1, sampledPixels: 0, uniqueColors: 0 };
  await expect.poll(async () => {
    evidence = await readOverlay(areas);
    return evidence.coloredPixels;
  }).toBeGreaterThan(25);
  expect(evidence.sampledPixels).toBeGreaterThan(0);
  expect(evidence.coloredPixels / evidence.sampledPixels).toBeLessThan(0.5);
  expect(evidence.orangeCoverage).toBeLessThan(0.01);
  const names = await readOverlay(canvas);
  expect(names.coloredPixels / names.sampledPixels).toBeLessThan(0.02);
  expect(names.orangeCoverage).toBeLessThan(0.01);

  await regions.click();
  await expect(regions).toHaveAttribute("aria-pressed", "false");
  await expect(canvas).toBeHidden();
  await expect(areas).toBeHidden();
  await page.mouse.move(1, 1);
  // Capture the approved, uncluttered default after exercising both states.
  await expect(page).toHaveScreenshot("graph-1280x900-light.png", {
    animations: "disabled",
    fullPage: false,
    maxDiffPixelRatio: 0.002,
  });
  expect(browserErrors.pageErrors).toEqual([]);
  expect(browserErrors.consoleErrors).toEqual([]);
});
