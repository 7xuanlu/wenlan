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

/** A pin string from the daemon as a source, or null when it names none. */
function asSourcePin(pin: string | null): SourcePin | null {
  return pin === "anthropic" || pin === "external" || pin === "on_device" ? pin : null;
}

/** What a fill did, from the routing read it acted on. `written` is what this
 *  fill sent: null for a job left alone, both null when nothing was sent.
 *
 *  `inEffect` is READ-SIDE ONLY. It is this client's expectation of each job's
 *  pin, computed from the one routing snapshot the fill acted on plus what it
 *  sent, and it is never read back from the daemon. It is normally right, and
 *  it is wrong in exactly the case `onlyIfUnset` exists for: if the user pins a
 *  job between the read and the write, the daemon keeps their pin and
 *  `inEffect` still names the pin this fill sent.
 *
 *  So use it for copy and logging, never as proof of what the daemon holds. A
 *  caller that needs the truth must re-read routing after the write. Nothing
 *  does today, which is why this stays a prediction rather than costing every
 *  fill a second round trip. */
export interface PinFill {
  written: { everyday: SourcePin | null; synthesis: SourcePin | null };
  inEffect: { everyday: SourcePin | null; synthesis: SourcePin | null };
}

/** Called after a model or provider is turned on, in Settings, when the setup
 *  wizard's on-device download finishes, or when onboarding reaches Done.
 *  Without it the new source sits in the pool while both jobs stay unpinned,
 *  and background work waits for a model the user believes they already
 *  chose. Returns null only on a daemon without the routing endpoint; a fill
 *  with every job already pinned writes nothing and still reports the pins in
 *  effect.
 *
 *  Preservation rests on two things. A job already pinned at the read is sent
 *  as null, which the daemon leaves untouched even if the user changes it
 *  before the write lands. A job that was unset at the read but pinned before
 *  the write is covered by `onlyIfUnset`: the daemon re-checks under the write
 *  and keeps the user's pin. On a daemon that predates that flag the field is
 *  ignored and this window is unguarded, as it was before. */
export async function fillUnsetPins(): Promise<PinFill | null> {
  const routing = await getResolvedRouting();
  if (!routing) return null;
  const written = pinsToFill(routing);
  if (written.everyday !== null || written.synthesis !== null) {
    await setSourcePin(written.everyday, written.synthesis, true);
  }
  return {
    written,
    inEffect: {
      everyday: written.everyday ?? asSourcePin(routing.everyday.pin),
      synthesis: written.synthesis ?? asSourcePin(routing.synthesis.pin),
    },
  };
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

const PIN_FILL_ATTEMPTS = 3;
const PIN_FILL_FIRST_BACKOFF_MS = 1000;

/** `fillUnsetPins` for a write nobody is waiting on, such as the wizard's model
 *  download resolving after the user left the step, or onboarding's Done step,
 *  which must never hold up completion. A failed routing read or pin write is
 *  retried with doubling backoff (1 s, then 2 s), because the daemon is local
 *  and failures are usually transient; each attempt re-reads routing, so a job
 *  pinned in the meantime is left alone. After the last attempt the failure is
 *  logged and null returned; this never throws.
 *
 *  The retries live in this webview's JS context. They survive a step
 *  unmounting and the window hiding, but not a quit or a reload. */
export async function fillUnsetPinsWithRetry(): Promise<PinFill | null> {
  for (let attempt = 1; ; attempt++) {
    try {
      return await fillUnsetPins();
    } catch (e) {
      if (attempt >= PIN_FILL_ATTEMPTS) {
        console.error(
          `routing: could not choose the new source for unpinned jobs after ${attempt} attempts`,
          e,
        );
        return null;
      }
      const backoff = PIN_FILL_FIRST_BACKOFF_MS * 2 ** (attempt - 1);
      await new Promise((resolve) => setTimeout(resolve, backoff));
    }
  }
}
