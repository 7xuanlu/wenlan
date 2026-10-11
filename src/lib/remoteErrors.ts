// SPDX-License-Identifier: AGPL-3.0-only
/**
 * Turns what the native side says about the relay into something a person can
 * act on. The native side sends English sentences, never codes, so this reads
 * those sentences once, here, and the screens only ever see a kind.
 *
 * `details` keeps the original sentence for a "Details" disclosure. It is for
 * someone reporting a problem, not for the main screen.
 */

export type RemoteErrorKind =
  /** The pairing request is gone: it expired, was used, or was never there. */
  | "expired"
  /** Too many wrong codes in a short time. */
  | "tooMany"
  /** The relay could not be reached. */
  | "offline"
  /** This computer's Web access key no longer works. */
  | "ended"
  | "other";

export interface RemoteError {
  kind: RemoteErrorKind;
  details: string;
}

const MAX_DETAILS = 400;

function messageOf(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === "object" && "message" in error && typeof error.message === "string") {
    return error.message;
  }
  return String(error ?? "");
}

/**
 * `pairing`: a 404 or 409 means the request is gone. Anywhere else a 404 is
 * just an error and gets no special wording.
 */
export function describeRemoteError(error: unknown, context: "pairing" | "other" = "other"): RemoteError {
  const raw = messageOf(error).trim();
  const details = raw.length > MAX_DETAILS ? `${raw.slice(0, MAX_DETAILS)}…` : raw;
  const text = raw.toLowerCase();
  const status = /\(http (\d{3})\)/.exec(text)?.[1];

  if (context === "pairing" && (status === "404" || status === "409")) return { kind: "expired", details };
  if (status === "429" || text.includes("rate limited")) return { kind: "tooMany", details };
  if (text.includes("requires device authorization")) return { kind: "ended", details };
  if (text.includes("unavailable; retry later") || text.includes("failed to fetch")) {
    return { kind: "offline", details };
  }
  return { kind: "other", details };
}
