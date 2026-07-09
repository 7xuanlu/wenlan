// SPDX-License-Identifier: AGPL-3.0-only
import { afterEach, describe, expect, it, vi } from "vitest";
import { liveInvoke } from "./live-invoke";

describe("liveInvoke list_all_tags", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("preserves the full TagData shape (tags + document_tags)", async () => {
    const payload = {
      tags: ["alpha", "beta"],
      document_tags: { "memory::m1": ["alpha"] },
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(JSON.stringify(payload), { status: 200 })),
    );

    const result = (await liveInvoke("list_all_tags")) as {
      tags: string[];
      document_tags: Record<string, string[]>;
    };

    expect(result.tags).toEqual(["alpha", "beta"]);
    expect(result.document_tags).toEqual({ "memory::m1": ["alpha"] });
  });
});
