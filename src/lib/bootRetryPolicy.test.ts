// SPDX-License-Identifier: AGPL-3.0-only
/**
 * The real boot ladder, pinned as arithmetic.
 *
 * `src/App.test.tsx` substitutes a short policy so its fail-closed case stays
 * a fast unit test; this file is where the production schedule is actually
 * checked, so the two together still cover what one slow end-to-end test used
 * to cover on its own.
 */
import { describe, expect, it, vi } from "vitest";

import {
  ATTEMPT_TIMEOUT_MS,
  BOOT_QUERY_RETRY,
  RUST_HEALTH_LOOP_BUDGET_MS,
  BOOT_SLOW_NOTICE_MS,
  bootQueryBudgetMs,
  bootQueryRetryDelay,
  withBootAttemptTimeout,
} from "./bootRetryPolicy";

describe("boot retry policy", () => {
  it("outlasts the Rust health loop's own wait for the same daemon", () => {
    // app/src/lib.rs: 10 attempts * 5s, plus 200ms * 2^i backoff for i in
    // 0..=8. If the frontend gave up first it would show the first-run wizard
    // to a configured user while Rust was still waiting on that same daemon.
    expect(RUST_HEALTH_LOOP_BUDGET_MS).toBe(152_200);
    expect(bootQueryBudgetMs()).toBeGreaterThanOrEqual(RUST_HEALTH_LOOP_BUDGET_MS);
    expect(bootQueryBudgetMs()).toBeGreaterThanOrEqual(150_000);
  });

  it("computes the budget as attempts x timeout plus every delay", () => {
    const attempts = BOOT_QUERY_RETRY + 1;
    const delays = Array.from({ length: BOOT_QUERY_RETRY }, (_, i) =>
      bootQueryRetryDelay(i),
    );
    const delayTotal = delays.reduce((sum, delay) => sum + delay, 0);

    expect(bootQueryBudgetMs()).toBe(attempts * ATTEMPT_TIMEOUT_MS + delayTotal);
    // 21 * 5_000 = 105_000, plus 1_000 + 2_000 + 18 * 3_000 = 57_000.
    expect(delayTotal).toBe(57_000);
    expect(bootQueryBudgetMs()).toBe(162_000);
  });

  it("backs off exponentially and then polls at a steady 3s", () => {
    expect(bootQueryRetryDelay(0)).toBe(1_000);
    expect(bootQueryRetryDelay(1)).toBe(2_000);
    expect(bootQueryRetryDelay(2)).toBe(3_000);
    expect(bootQueryRetryDelay(19)).toBe(3_000);
  });

  it("shrinks with the retry count, so a test double cannot inflate it", () => {
    expect(bootQueryBudgetMs(1)).toBe(2 * ATTEMPT_TIMEOUT_MS + 1_000);
    expect(bootQueryBudgetMs(0)).toBe(ATTEMPT_TIMEOUT_MS);
  });

  it("captions the boot screen well before the budget runs out", () => {
    // 15s is a caption on a wait that is still healthy, not a deadline: the
    // gate keeps retrying behind it for another two and a half minutes. If
    // this ever crept past the budget the line would never render at all.
    expect(BOOT_SLOW_NOTICE_MS).toBe(15_000);
    expect(BOOT_SLOW_NOTICE_MS).toBeLessThan(bootQueryBudgetMs());
    // And it must outlast a normal cold start, or every launch flashes it.
    expect(BOOT_SLOW_NOTICE_MS).toBeGreaterThanOrEqual(2 * ATTEMPT_TIMEOUT_MS);
  });

  it("times out a hung boot attempt instead of waiting forever", async () => {
    vi.useFakeTimers();
    try {
      const hung = withBootAttemptTimeout(new Promise<string>(() => {}), 5000);
      const timedOut = expect(hung).rejects.toThrow(
        "Boot attempt timed out after 5000 ms waiting for the app to answer",
      );
      await vi.advanceTimersByTimeAsync(5000);
      await timedOut;

      const fast = withBootAttemptTimeout(
        new Promise<string>((resolve) => {
          setTimeout(() => resolve("ok"), 10);
        }),
        5000,
      );
      const settled = expect(fast).resolves.toBe("ok");
      await vi.advanceTimersByTimeAsync(10);
      await settled;
      // The timer is cleared when the promise settles: running well past it
      // must not reject afterwards.
      await vi.advanceTimersByTimeAsync(10_000);
      await expect(fast).resolves.toBe("ok");
    } finally {
      vi.useRealTimers();
    }
  });
});
