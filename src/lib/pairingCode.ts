// SPDX-License-Identifier: AGPL-3.0-only
/**
 * What a person can type into "Have a code?": the short code the browser page
 * shows (`ABCD-2345`), or the long code a `wenlan://pair` link carries.
 * The alphabet is the relay's: no 0/O/1/I/L/U look-alikes.
 */

const SHORT_ALPHABET = "23456789ABCDEFGHJKMNPQRSTVWXYZ";
const SHORT_LENGTH = 8;
const LONG = /^[A-Za-z0-9_-]{64}$/;

export type PairingCodeInput =
  | { kind: "short"; code: string }
  | { kind: "long"; code: string }
  | { kind: "invalid" };

export function classifyPairingCode(input: string): PairingCodeInput {
  const trimmed = input.trim();
  if (LONG.test(trimmed)) return { kind: "long", code: trimmed };
  const short = trimmed.replace(/[\s-]+/g, "").toUpperCase();
  if (short.length === SHORT_LENGTH && [...short].every((char) => SHORT_ALPHABET.includes(char))) {
    return { kind: "short", code: short };
  }
  return { kind: "invalid" };
}

/** `ABCD2345` shown the way the browser page shows it. */
export function formatShortCode(code: string): string {
  return code.length === SHORT_LENGTH ? `${code.slice(0, 4)}-${code.slice(4)}` : code;
}
