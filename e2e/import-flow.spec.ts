// SPDX-License-Identifier: AGPL-3.0-only
//
// Honest import flow: paste memories, watch the daemon's real phase counts
// (never an elapsed-time bar), and land on a summary with measured figures.
//
import { expect, test, type Page } from "@playwright/test";
import { collectBrowserErrors, installTauriMock } from "./tauriMock";

type ImportCommandArgs = {
  batchId?: string | null;
  chunkIndex?: number | null;
  chunkTotal?: number | null;
  content?: unknown;
};

declare global {
  interface Window {
    __importCalls?: ImportCommandArgs[];
    __releaseImport?: boolean;
  }
}

// The shared Tauri mock has no import commands yet, so this spec layers a
// small stateful wrapper on top of its invoke: chunk uploads return measured
// per-chunk results (held until the progress assertions pass), and the batch
// status stays mid-flight so the summary keeps saying background work
// continues.
async function installImportMock(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const calls: ImportCommandArgs[] = [];
    let batchPolls = 0;
    const internals = window.__TAURI_INTERNALS__;
    if (!internals) return;
    const orig = internals.invoke.bind(internals);
    internals.invoke = (async (command: string, args?: unknown) => {
      const params = (args ?? {}) as ImportCommandArgs;
      if (command === "import_memories_cmd") {
        calls.push(params);
        // Hold the chunk until the spec has asserted on the progress phase —
        // otherwise the mock resolves before React paints it and the phase
        // assertions race the summary.
        await new Promise<void>((resolve) => {
          const check = () => {
            if (window.__releaseImport) resolve();
            else setTimeout(check, 25);
          };
          check();
        });
        const lines = String(params.content ?? "")
          .split("\n")
          .filter((line) => line.trim() !== "");
        return {
          imported: lines.length,
          skipped: 0,
          breakdown: { fact: lines.length },
          entities_created: 0,
          observations_added: 0,
          relations_created: 0,
          batch_id: params.batchId ?? "batch-e2e",
        };
      }
      if (command === "import_batch_status_cmd") {
        batchPolls += 1;
        // Stay mid-flight for the whole spec: the summary must keep saying
        // background work continues while these assertions run.
        const settled = batchPolls >= 1000;
        return {
          batch_id: params.batchId ?? "batch-e2e",
          source: "chatgpt",
          started_at: 1_700_000_000,
          updated_at: 1_700_000_100,
          chunks_received: 1,
          memories_imported: 3,
          memories_skipped: 0,
          entities_detected: settled ? 2 : 1,
          entities_established: settled ? 1 : 0,
          pages_distilled: settled ? 1 : 0,
          phases: [
            { phase: "ingest", state: "complete", done: 3, total: 3, failed: 0 },
            { phase: "store", state: "complete", done: 3, total: 3, failed: 0 },
            {
              phase: "detect",
              state: settled ? "complete" : "running",
              done: settled ? 3 : 1,
              total: 3,
              failed: 0,
            },
            {
              phase: "enrich",
              state: settled ? "complete" : "pending",
              done: 0,
              total: 0,
              failed: 0,
            },
            {
              phase: "link",
              state: settled ? "complete" : "pending",
              done: 0,
              total: 0,
              failed: 0,
            },
            {
              phase: "distill",
              state: settled ? "complete" : "running",
              done: settled ? 1 : 0,
              total: 0,
              failed: 0,
            },
          ],
          complete: settled,
          space: null,
        };
      }
      if (command === "active_import_batches_cmd") return { batches: [] };
      return orig(command, args);
    }) as typeof internals.invoke;
    window.__importCalls = calls;
  });
}

test("import flow shows real phases and an honest summary", async ({ page }) => {
  const errors = collectBrowserErrors(page);

  await page.setViewportSize({ width: 1280, height: 900 });
  await installTauriMock(page, { locale: "en", rawActions: [], memories: [] });
  await installImportMock(page);
  await page.goto("/");

  // Account menu → Settings → Sources → Import memories. The Sources view in
  // the primary navigation only offers "Manage sources" once a source is
  // registered, and this fixture registers none, so it renders its empty state
  // instead. The account menu reaches Settings either way.
  await page.getByRole("button", { name: /account menu/i }).click();
  await page.getByRole("menuitem", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Sources", exact: true }).click();
  await page.getByRole("button", { name: "Import", exact: true }).click();

  // Paste three memories and start the import.
  await page
    .getByPlaceholder(/paste your memories/i)
    .fill("Memory one\nMemory two\nMemory three");
  await page.getByRole("button", { name: "Import", exact: true }).click();

  // The daemon's row counts — not an elapsed-time bar. Only the two phases
  // that end in something the user can use; the rest report from the sidebar.
  await expect(page.getByText("Receiving memories")).toBeVisible();
  await expect(page.getByText("Storing memories")).toBeVisible();
  await expect(page.getByText("Detecting entities")).toHaveCount(0);
  await expect(page.getByText("Distilling pages")).toHaveCount(0);
  await expect(page.getByText("3 of 3").first()).toBeVisible();
  await expect(page.getByText(/Processing your memories/)).not.toBeVisible();

  // Release the held chunk so the request phases finish.
  await page.evaluate(() => {
    window.__releaseImport = true;
  });

  // The request phases finish: one chunk, one batch id.
  await expect
    .poll(() => page.evaluate(() => (window.__importCalls ?? []).length), { timeout: 10_000 })
    .toBe(1);
  const calls = await page.evaluate(() => window.__importCalls ?? []);
  expect(calls).toHaveLength(1);
  expect(typeof calls[0]?.batchId).toBe("string");
  expect(calls[0]?.batchId).not.toHaveLength(0);
  expect(calls[0]?.chunkIndex).toBe(0);
  expect(calls[0]?.chunkTotal).toBe(1);

  // The summary reports what the import produced and hands the rest off.
  await expect(page.getByText(/3 memories imported/i)).toBeVisible();
  await expect(page.getByText("1 detected entities")).toHaveCount(0);
  await expect(
    page.getByText(/3 memories stored and searchable now/),
  ).toBeVisible();
  await expect(
    page.getByText(/status line at the bottom of the sidebar/),
  ).toBeVisible();

  expect(errors.pageErrors).toEqual([]);
  expect(errors.consoleErrors).toEqual([]);
});
