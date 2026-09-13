// SPDX-License-Identifier: AGPL-3.0-only
import { getResolvedRouting, setSourcePin, type ResolvedRouting } from "./tauri";

/** A source a job can be pinned to. */
export type SourcePin = "anthropic" | "external" | "on_device";

/** First-onboarding routing derivation from what the daemon reports configured.
 *  everyday prefers on-device (private, free, the recommended everyday source),
 *  then a connected provider; synthesis prefers a connected provider (better
 *  synthesis quality), with on-device as the fallback. When both cloud and an
 *  external provider are configured, synthesis prefers Anthropic — the summary
 *  names it, so the choice is visible.
 *
 *  Invariant: everyday and synthesis are both null or both set — "nothing
 *  configured at all" is the only null case, so a partial pin write never
 *  happens (the caller relies on this to decide write-or-skip). */
export function deriveOnboardingPins(pool: ResolvedRouting["pool"]): {
  everyday: SourcePin | null;
  synthesis: SourcePin | null;
} {
  const hasAnthropic = pool.anthropic.configured;
  const hasExternal = pool.external != null;
  const hasOnDevice = pool.on_device != null;
  const everyday: SourcePin | null = hasOnDevice
    ? "on_device"
    : hasAnthropic
      ? "anthropic"
      : hasExternal
        ? "external"
        : null;
  const synthesis: SourcePin | null = hasAnthropic
    ? "anthropic"
    : hasExternal
      ? "external"
      : hasOnDevice
        ? "on_device"
        : null;
  return { everyday, synthesis };
}

/** The pins to write so a job with no model chosen gets one, using the same
 *  preference as onboarding. A job the user already pinned is never touched,
 *  even when its pinned source is unavailable: that choice is theirs. Null for
 *  a job means "leave it", matching `setSourcePin`'s patch semantics.
 *
 *  Unlike onboarding, an on-device model counts only once it is loaded. The
 *  pool lists a model as soon as one is selected, so a load that failed or was
 *  never finished would otherwise win over the provider the user just saved
 *  and leave that job pinned to nothing that runs. Settings' Load returns only
 *  after the model is loaded, so that path still chooses it. */
export function pinsToFill(routing: ResolvedRouting): {
  everyday: SourcePin | null;
  synthesis: SourcePin | null;
} {
  const { pool } = routing;
  const derived = deriveOnboardingPins({
    ...pool,
    on_device: pool.on_device?.loaded ? pool.on_device : null,
  });
  return {
    everyday: routing.everyday.pin === null ? derived.everyday : null,
    synthesis: routing.synthesis.pin === null ? derived.synthesis : null,
  };
}

/** Called after a model or provider is turned on in Settings. Without it the
 *  new source sits in the pool while both jobs stay unpinned, and background
 *  work waits for a model the user believes they already chose. Returns what
 *  was written, or null when nothing was (a daemon without the routing
 *  endpoint, or every job already pinned). */
export async function fillUnsetPins(): Promise<{
  everyday: SourcePin | null;
  synthesis: SourcePin | null;
} | null> {
  const routing = await getResolvedRouting();
  if (!routing) return null;
  const pins = pinsToFill(routing);
  if (pins.everyday === null && pins.synthesis === null) return null;
  await setSourcePin(pins.everyday, pins.synthesis);
  return pins;
}

/** `fillUnsetPins` for a save handler whose own write already succeeded: a
 *  failed pin write is logged, not shown as a failed save, matching how
 *  onboarding treats the same write. */
export async function fillUnsetPinsAfterSave(): Promise<void> {
  try {
    await fillUnsetPins();
  } catch (e) {
    console.error("routing: could not choose the new source for unpinned jobs", e);
  }
}
