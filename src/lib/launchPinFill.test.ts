// SPDX-License-Identifier: AGPL-3.0-only
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  shouldShowWizard: vi.fn(),
  getResolvedRouting: vi.fn(),
  setSourcePin: vi.fn(),
}));
vi.mock("./tauri", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./tauri")>();
  return { ...actual, ...mocks };
});

import { fillUnsetPinsAtLaunch, resetLaunchPinFillForTest } from "./launchPinFill";
import type { ResolvedRouting } from "./tauri";

/** Routing with a loaded on-device model and both jobs unpinned. */
function unpinnedWithLoadedModel(): ResolvedRouting {
  return {
    everyday: { source: "basic", model: null, mode: "unconfigured", pin: null },
    synthesis: { source: "none", model: null, mode: "unconfigured", pin: null },
    pool: {
      anthropic: { configured: false, everyday_model: null, synthesis_model: null },
      external: null,
      on_device: { selected: "qwen3-4b", loaded: true },
    },
  };
}

beforeEach(() => {
  resetLaunchPinFillForTest();
  mocks.shouldShowWizard.mockResolvedValue(false);
  mocks.getResolvedRouting.mockResolvedValue(unpinnedWithLoadedModel());
  mocks.setSourcePin.mockResolvedValue(undefined);
});

afterEach(() => {
  Object.values(mocks).forEach((m) => m.mockReset());
});

describe("fillUnsetPinsAtLaunch", () => {
  it("pins the loaded model for both jobs when setup is complete", async () => {
    await expect(fillUnsetPinsAtLaunch()).resolves.toEqual({
      written: { everyday: "on_device", synthesis: "on_device" },
      inEffect: { everyday: "on_device", synthesis: "on_device" },
    });
    expect(mocks.setSourcePin).toHaveBeenCalledWith("on_device", "on_device", true);
  });

  it("runs once per launch, however many times it is called", async () => {
    await fillUnsetPinsAtLaunch();
    await fillUnsetPinsAtLaunch();
    expect(mocks.getResolvedRouting).toHaveBeenCalledTimes(1);
    expect(mocks.setSourcePin).toHaveBeenCalledTimes(1);
  });

  it("does not let two calls in the same tick both fill", async () => {
    // React's development double-effect mounts the hook twice immediately.
    const [first, second] = await Promise.all([
      fillUnsetPinsAtLaunch(),
      fillUnsetPinsAtLaunch(),
    ]);
    expect(mocks.setSourcePin).toHaveBeenCalledTimes(1);
    expect([first, second].filter((r) => r !== null)).toHaveLength(1);
  });

  it("writes nothing while the wizard would be showing, and can still fill afterwards", async () => {
    mocks.shouldShowWizard.mockResolvedValueOnce(true);
    await expect(fillUnsetPinsAtLaunch()).resolves.toBeNull();
    expect(mocks.getResolvedRouting).not.toHaveBeenCalled();
    expect(mocks.setSourcePin).not.toHaveBeenCalled();

    // Finishing setup and reaching the main screen must not be blocked by the
    // attempt that bailed out.
    await fillUnsetPinsAtLaunch();
    expect(mocks.setSourcePin).toHaveBeenCalledTimes(1);
  });

  it("writes nothing when no source is configured yet", async () => {
    const routing = unpinnedWithLoadedModel();
    routing.pool.on_device = { selected: "qwen3-4b", loaded: false };
    mocks.getResolvedRouting.mockResolvedValue(routing);
    await expect(fillUnsetPinsAtLaunch()).resolves.toEqual({
      written: { everyday: null, synthesis: null },
      inEffect: { everyday: null, synthesis: null },
    });
    expect(mocks.setSourcePin).not.toHaveBeenCalled();
  });

  it("leaves a job the user already pinned alone", async () => {
    const routing = unpinnedWithLoadedModel();
    routing.everyday = { source: "external", model: "llama3", mode: "pinned", pin: "external" };
    mocks.getResolvedRouting.mockResolvedValue(routing);
    await fillUnsetPinsAtLaunch();
    expect(mocks.setSourcePin).toHaveBeenCalledWith(null, "on_device", true);
  });

  it("logs and gives up when the daemon cannot say whether setup is done", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    mocks.shouldShowWizard.mockRejectedValueOnce(new Error("connection refused"));
    await expect(fillUnsetPinsAtLaunch()).resolves.toBeNull();
    expect(error).toHaveBeenCalled();
    expect(mocks.setSourcePin).not.toHaveBeenCalled();
    error.mockRestore();
  });

  it("does not surface a failed pin write", async () => {
    vi.useFakeTimers();
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    mocks.setSourcePin.mockRejectedValue(new Error("daemon down"));
    const filled = fillUnsetPinsAtLaunch();
    await vi.advanceTimersByTimeAsync(10_000);
    await expect(filled).resolves.toBeNull();
    expect(mocks.setSourcePin).toHaveBeenCalledTimes(3);
    error.mockRestore();
    vi.useRealTimers();
  });
});
