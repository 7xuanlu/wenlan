// SPDX-License-Identifier: AGPL-3.0-only
import { afterEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  getResolvedRouting: vi.fn(),
  setSourcePin: vi.fn(),
}));
vi.mock("./tauri", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./tauri")>();
  return { ...actual, ...mocks };
});

import {
  fillUnsetPins,
  fillUnsetPinsAfterSave,
  fillUnsetPinsWithRetry,
  pinsToFill,
} from "./routingPins";
import type { JobRoute, ResolvedRouting } from "./tauri";

function job(pin: string | null): JobRoute {
  return pin === null
    ? { source: "none", model: null, mode: "unconfigured", pin: null }
    : { source: pin, model: "a-model", mode: "pinned", pin };
}

function routing(
  fields: {
    everydayPin?: string | null;
    synthesisPin?: string | null;
    pool?: Partial<ResolvedRouting["pool"]>;
  } = {},
): ResolvedRouting {
  return {
    everyday: job(fields.everydayPin ?? null),
    synthesis: job(fields.synthesisPin ?? null),
    pool: {
      anthropic: { configured: false, everyday_model: null, synthesis_model: null },
      external: null,
      on_device: null,
      ...fields.pool,
    },
  };
}

const onDevice = { on_device: { selected: "qwen3-4b", loaded: true } };

describe("pinsToFill", () => {
  it("chooses the on-device model for both jobs when nothing is pinned", () => {
    expect(pinsToFill(routing({ pool: onDevice }))).toEqual({
      everyday: "on_device",
      synthesis: "on_device",
    });
  });

  it("never replaces a job the user already pinned", () => {
    expect(
      pinsToFill(
        routing({
          everydayPin: "external",
          pool: { ...onDevice, anthropic: { configured: true, everyday_model: "h", synthesis_model: "s" } },
        }),
      ),
    ).toEqual({ everyday: null, synthesis: "anthropic" });
  });

  it("keeps a pin whose source is unavailable", () => {
    const r = routing({ pool: onDevice });
    r.everyday = { source: "basic", model: null, mode: "pinned_unavailable", pin: "external" };
    r.synthesis = { source: "none", model: null, mode: "pinned_unavailable", pin: "external" };
    expect(pinsToFill(r)).toEqual({ everyday: null, synthesis: null });
  });

  it("writes nothing when no source is configured", () => {
    expect(pinsToFill(routing())).toEqual({ everyday: null, synthesis: null });
  });

  it("passes over an on-device model that is selected but not loaded", () => {
    expect(
      pinsToFill(
        routing({
          pool: {
            on_device: { selected: "qwen3-4b", loaded: false },
            anthropic: { configured: true, everyday_model: "h", synthesis_model: "s" },
          },
        }),
      ),
    ).toEqual({ everyday: "anthropic", synthesis: "anthropic" });
  });

  it("writes nothing when the only source is an unloaded on-device model", () => {
    expect(
      pinsToFill(routing({ pool: { on_device: { selected: "qwen3-4b", loaded: false } } })),
    ).toEqual({ everyday: null, synthesis: null });
  });
});

describe("fillUnsetPins", () => {
  afterEach(() => {
    Object.values(mocks).forEach((m) => m.mockReset());
  });

  it("writes only the unpinned job", async () => {
    mocks.getResolvedRouting.mockResolvedValue(
      routing({ synthesisPin: "anthropic", pool: onDevice }),
    );
    mocks.setSourcePin.mockResolvedValue(undefined);
    await expect(fillUnsetPins()).resolves.toEqual({
      written: { everyday: "on_device", synthesis: null },
      inEffect: { everyday: "on_device", synthesis: "anthropic" },
    });
    // The fill's pins came from a read, so the write must be the atomic one:
    // a pin the user chose since that read has to win.
    expect(mocks.setSourcePin).toHaveBeenCalledWith("on_device", null, true);
  });

  it("asks the daemon to keep a pin set between the read and the write", async () => {
    // The fill is a read then a write. Without the flag, a user who pins a job
    // in that window has their choice overwritten by what the read saw.
    mocks.getResolvedRouting.mockResolvedValue(routing({ pool: onDevice }));
    mocks.setSourcePin.mockResolvedValue(undefined);
    await fillUnsetPins();
    expect(mocks.setSourcePin).toHaveBeenCalledWith("on_device", "on_device", true);
  });

  it("skips the write when every job is pinned, and reports the pins already in effect", async () => {
    mocks.getResolvedRouting.mockResolvedValue(
      routing({ everydayPin: "on_device", synthesisPin: "external", pool: onDevice }),
    );
    await expect(fillUnsetPins()).resolves.toEqual({
      written: { everyday: null, synthesis: null },
      inEffect: { everyday: "on_device", synthesis: "external" },
    });
    expect(mocks.setSourcePin).not.toHaveBeenCalled();
  });

  it("reports a job left unpinned when there is nothing to fill it with", async () => {
    mocks.getResolvedRouting.mockResolvedValue(routing());
    await expect(fillUnsetPins()).resolves.toEqual({
      written: { everyday: null, synthesis: null },
      inEffect: { everyday: null, synthesis: null },
    });
    expect(mocks.setSourcePin).not.toHaveBeenCalled();
  });

  it("skips the write on a daemon without the routing endpoint", async () => {
    mocks.getResolvedRouting.mockResolvedValue(null);
    await expect(fillUnsetPins()).resolves.toBeNull();
    expect(mocks.setSourcePin).not.toHaveBeenCalled();
  });

  it("does not turn a failed pin write into a failed save", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    mocks.getResolvedRouting.mockResolvedValue(routing({ pool: onDevice }));
    mocks.setSourcePin.mockRejectedValue(new Error("daemon down"));
    await expect(fillUnsetPinsAfterSave()).resolves.toBeUndefined();
    expect(error).toHaveBeenCalled();
    error.mockRestore();
  });
});

describe("fillUnsetPinsWithRetry", () => {
  afterEach(() => {
    vi.useRealTimers();
    Object.values(mocks).forEach((m) => m.mockReset());
  });

  it("re-reads routing on retry, so a job pinned after a failed attempt is left alone", async () => {
    vi.useFakeTimers();
    mocks.getResolvedRouting
      .mockResolvedValueOnce(routing({ pool: onDevice }))
      .mockResolvedValueOnce(routing({ synthesisPin: "external", pool: onDevice }));
    mocks.setSourcePin.mockRejectedValueOnce(new Error("daemon busy")).mockResolvedValue(undefined);

    const filled = fillUnsetPinsWithRetry();
    await vi.advanceTimersByTimeAsync(1000);

    await expect(filled).resolves.toEqual({
      written: { everyday: "on_device", synthesis: null },
      inEffect: { everyday: "on_device", synthesis: "external" },
    });
    expect(mocks.setSourcePin).toHaveBeenCalledTimes(2);
    expect(mocks.setSourcePin).toHaveBeenLastCalledWith("on_device", null, true);
  });

  it("retries a failed routing read", async () => {
    vi.useFakeTimers();
    mocks.getResolvedRouting
      .mockRejectedValueOnce(new Error("connection refused"))
      .mockResolvedValue(routing({ pool: onDevice }));
    mocks.setSourcePin.mockResolvedValue(undefined);

    const filled = fillUnsetPinsWithRetry();
    await vi.advanceTimersByTimeAsync(1000);

    await expect(filled).resolves.toEqual({
      written: { everyday: "on_device", synthesis: "on_device" },
      inEffect: { everyday: "on_device", synthesis: "on_device" },
    });
  });

  it("stops after a bounded number of attempts with backoff, logs, and never throws", async () => {
    vi.useFakeTimers();
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    mocks.getResolvedRouting.mockResolvedValue(routing({ pool: onDevice }));
    mocks.setSourcePin.mockRejectedValue(new Error("daemon down"));

    const filled = fillUnsetPinsWithRetry();
    await vi.advanceTimersByTimeAsync(999);
    expect(mocks.setSourcePin).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(mocks.setSourcePin).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(2000);
    expect(mocks.setSourcePin).toHaveBeenCalledTimes(3);
    await vi.advanceTimersByTimeAsync(60_000);

    await expect(filled).resolves.toBeNull();
    expect(mocks.setSourcePin).toHaveBeenCalledTimes(3);
    expect(error).toHaveBeenCalledTimes(1);
    error.mockRestore();
  });

  it("does not retry on a daemon without the routing endpoint", async () => {
    mocks.getResolvedRouting.mockResolvedValue(null);
    await expect(fillUnsetPinsWithRetry()).resolves.toBeNull();
    expect(mocks.getResolvedRouting).toHaveBeenCalledTimes(1);
    expect(mocks.setSourcePin).not.toHaveBeenCalled();
  });
});
