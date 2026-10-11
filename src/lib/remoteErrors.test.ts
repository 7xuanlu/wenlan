// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from "vitest";
import { describeRemoteError } from "./remoteErrors";

describe("describeRemoteError", () => {
  it.each([
    ["Remote connection rejected (HTTP 404)"],
    ["Remote connection rejected (HTTP 409)"],
  ])("reads %j as an expired request when a pairing was involved", (message) => {
    expect(describeRemoteError(message, "pairing").kind).toBe("expired");
  });

  it("does not call a 404 elsewhere an expired request", () => {
    expect(describeRemoteError("Remote connection rejected (HTTP 404)", "other").kind).toBe("other");
    expect(describeRemoteError("Remote connection rejected (HTTP 404)").kind).toBe("other");
  });

  it("reads a rate limit as too many tries, in any context", () => {
    expect(describeRemoteError("Remote connection rate limited", "pairing").kind).toBe("tooMany");
    expect(describeRemoteError("Remote connection rejected (HTTP 429)").kind).toBe("tooMany");
  });

  it("reads an unreachable relay as offline", () => {
    expect(describeRemoteError("Remote connection unavailable; retry later", "pairing").kind).toBe("offline");
    expect(describeRemoteError(new Error("Failed to fetch")).kind).toBe("offline");
  });

  it("reads a refused device key as an ended key", () => {
    expect(describeRemoteError("Remote connection requires device authorization").kind).toBe("ended");
  });

  it("keeps everything else as an unknown problem and keeps the sentence for Details", () => {
    const result = describeRemoteError({ message: "Unexpected remote connection response" });
    expect(result).toEqual({ kind: "other", details: "Unexpected remote connection response" });
    expect(describeRemoteError(undefined)).toEqual({ kind: "other", details: "" });
  });

  it("caps the details so a long native warning cannot flood the screen", () => {
    const result = describeRemoteError("x".repeat(2000));
    expect(result.details.length).toBeLessThanOrEqual(401);
  });
});
