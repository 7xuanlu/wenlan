// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from "vitest";
import { classifyPairingCode, formatShortCode } from "./pairingCode";

describe("classifyPairingCode", () => {
  it.each(["ABCD-2345", "abcd2345", " abcd 2345 ", "ABCD2345", "abcd-2345"])("accepts %j as the short code", (typed) => {
    expect(classifyPairingCode(typed)).toEqual({ kind: "short", code: "ABCD2345" });
  });

  it("accepts the 64-character code from a link as it is", () => {
    const long = "aB3_-".repeat(13).slice(0, 64);
    expect(classifyPairingCode(` ${long} `)).toEqual({ kind: "long", code: long });
  });

  it.each([
    ["", "empty"],
    ["ABCD-234", "too short"],
    ["ABCD-23456", "too long"],
    ["ABCD-2O45", "O is not in the alphabet"],
    ["ABCD-2145", "1 is not in the alphabet"],
    ["ABCD-2045", "0 is not in the alphabet"],
    ["ABCD-2I45", "I is not in the alphabet"],
    ["abcd-2l45", "l is not in the alphabet"],
    ["ABCD-2U45", "U is not in the alphabet"],
    ["ABCD-23!5", "punctuation"],
    ["a".repeat(63), "63 characters"],
    ["a".repeat(65), "65 characters"],
  ])("rejects %j (%s)", (typed) => {
    expect(classifyPairingCode(typed)).toEqual({ kind: "invalid" });
  });

  it("formats a short code the way the browser shows it", () => {
    expect(formatShortCode("ABCD2345")).toBe("ABCD-2345");
  });
});
