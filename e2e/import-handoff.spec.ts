// SPDX-License-Identifier: AGPL-3.0-only
//
// The import handoff.
//
// The import used to show six phases and then a trailing bill of what the
// background still owed. This proves the new contract: the import surface
// shows the two phases that end in something the user can use, says plainly
// that those memories are searchable now, and points at the sidebar status
// line, which by then is reporting the rest.
//
import { expect, test, type Page } from "@playwright/test";
import { collectBrowserErrors, installTauriMock } from "./tauriMock";

/** The four phases that must never appear on the import surface again. */
const BACKGROUND_PHASE_LABELS = [
  "Detecting entities",
  "Enriching memories",
  "Linking memories",
  "Distilling pages",
] as const;

declare global {
  interface Window {
    __releaseHandoffImport?: boolean;
  }
}

/**
 * One stateful mock for both surfaces, because the point of this spec is that
 * they agree: the moment the import stops reporting background work, the
 * toolbar Activity button starts. `get_activity` therefore answers from the same counter
 * the batch status does, rather than from a fixed fixture.
 */
async function installHandoffMock(page: Page): Promise<void> {
  await page.addInitScript(() => {
    let imported = false;
    const internals = window.__TAURI_INTERNALS__;
    if (!internals) return;
    const orig = internals.invoke.bind(internals);
    const asset = (
      kind: string,
      state: string,
      done: number,
      total: number,
    ) => ({ kind, state, done, total, blocked: 0, steps: [] });

    internals.invoke = (async (command: string, args?: unknown) => {
      const params = (args ?? {}) as { batchId?: string | null; content?: unknown };

      if (command === "import_memories_cmd") {
        // Held until the spec has asserted on the progress phase, so the
        // summary cannot race past it.
        await new Promise<void>((resolve) => {
          const check = () => {
            if (window.__releaseHandoffImport) resolve();
            else setTimeout(check, 25);
          };
          check();
        });
        imported = true;
        return {
          imported: 3,
          skipped: 0,
          breakdown: { fact: 3 },
          entities_created: 0,
          observations_added: 0,
          relations_created: 0,
          batch_id: params.batchId ?? "batch-handoff",
        };
      }

      if (command === "import_batch_status_cmd") {
        return {
          batch_id: params.batchId ?? "batch-handoff",
          source: "chatgpt",
          started_at: 1_700_000_000,
          updated_at: 1_700_000_100,
          chunks_received: 1,
          memories_imported: 3,
          memories_skipped: 0,
          entities_detected: 2,
          entities_established: 0,
          pages_distilled: 0,
          phases: [
            { phase: "ingest", state: "complete", done: 3, total: 3, failed: 0 },
            { phase: "store", state: "complete", done: 3, total: 3, failed: 0 },
            { phase: "detect", state: "running", done: 1, total: 3, failed: 0 },
            { phase: "enrich", state: "running", done: 1, total: 3, failed: 0 },
            { phase: "link", state: "pending", done: 0, total: 0, failed: 0 },
            { phase: "distill", state: "running", done: 0, total: 0, failed: 0 },
          ],
          // Never settles: the handoff sentence has to hold while the
          // background is still working, which is exactly when it matters.
          complete: false,
          space: null,
        };
      }

      if (command === "active_import_batches_cmd") return { batches: [] };

      if (command === "get_activity") {
        // Before the import there is nothing to organize; after it, the
        // background work the import handed off is what the line reports.
        return imported
          ? {
              state: "organizing",
              last_activity_at: 1_783_728_000,
              assets: [
                asset("memories", "running", 1, 3),
                asset("entities", "running", 1, 3),
                asset("pages", "idle", 0, 0),
              ],
              everyday: { job: "everyday", lane: "on_device", model: "Qwen3 4B", mode: "auto", available: true },
              synthesis: { job: "synthesis", lane: "on_device", model: "Qwen3 4B", mode: "auto", available: true },
            }
          : {
              state: "up_to_date",
              last_activity_at: null,
              assets: [
                asset("memories", "idle", 0, 0),
                asset("entities", "idle", 0, 0),
                asset("pages", "idle", 0, 0),
              ],
              everyday: { job: "everyday", lane: "on_device", model: "Qwen3 4B", mode: "auto", available: true },
              synthesis: { job: "synthesis", lane: "on_device", model: "Qwen3 4B", mode: "auto", available: true },
            };
      }

      return orig(command, args);
    }) as typeof internals.invoke;
  });
}

test("an import shows Ingest and Store, then hands the rest to the toolbar Activity button", async ({ page }) => {
  const errors = collectBrowserErrors(page);

  await page.setViewportSize({ width: 1280, height: 900 });
  await installTauriMock(page, { locale: "en", rawActions: [], memories: [] });
  await installHandoffMock(page);
  await page.goto("/");

  // Nothing has been given to Wenlan yet, so the button says so.
  const statusLine = page.getByTestId("activity-status");
  await expect(statusLine).toHaveAttribute("data-state", "up_to_date");
  await expect(statusLine).toHaveAccessibleName("Activity, Up to date");

  // Account menu → Settings → Sources → Import memories.
  await page.getByRole("button", { name: /account menu/i }).click();
  await page.getByRole("menuitem", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Sources", exact: true }).click();
  await page.getByRole("button", { name: "Import", exact: true }).click();

  await page
    .getByPlaceholder(/paste your memories/i)
    .fill("Memory one\nMemory two\nMemory three");
  await page.getByRole("button", { name: "Import", exact: true }).click();

  // ── Mid-flight: two phases, and only two ──
  await expect(page.getByText("Receiving memories")).toBeVisible();
  await expect(page.getByText("Storing memories")).toBeVisible();
  for (const label of BACKGROUND_PHASE_LABELS) {
    await expect(page.getByText(label)).toHaveCount(0);
  }

  await page.evaluate(() => {
    window.__releaseHandoffImport = true;
  });

  // ── The handoff sentence, naming the number stored ──
  await expect(page.getByText(/3 memories stored and searchable now/)).toBeVisible();
  await expect(
    page.getByText(/Follow along from Activity in the toolbar/),
  ).toBeVisible();

  // ── And the button it points at is doing the reporting ──
  await expect(statusLine).toHaveAttribute("data-state", "organizing", { timeout: 15_000 });
  await expect(statusLine).toHaveAccessibleName("Activity, Steeping");
  await expect(page.getByTestId("activity-status-dot")).toHaveAttribute(
    "data-dot-state",
    "organizing",
  );

  // The background phases stay gone on the summary too, along with the
  // trailing figures that used to read as an unpaid bill.
  for (const label of BACKGROUND_PHASE_LABELS) {
    await expect(page.getByText(label)).toHaveCount(0);
  }
  await expect(page.getByText(/detected entities/)).toHaveCount(0);
  await expect(page.getByText(/related pages/)).toHaveCount(0);

  // The detail the import stopped showing is one click away, not gone.
  await statusLine.click();
  const popover = page.getByRole("dialog", { name: "Background activity" });
  await expect(popover).toBeVisible();
  await expect(popover.getByTestId("activity-asset-memories")).toContainText(
    "1 of 3 summarized and linked",
  );

  expect(errors.pageErrors).toEqual([]);
  expect(errors.consoleErrors).toEqual([]);
});
