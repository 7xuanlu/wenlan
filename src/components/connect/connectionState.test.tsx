// SPDX-License-Identifier: AGPL-3.0-only
import { describe, expect, it } from "vitest";
import type { AgentConnection } from "../../lib/tauri";
import { heardSince, seenFamiliesOf } from "./connectionState";

function agent(name: string, overrides: Partial<AgentConnection> = {}): AgentConnection {
  return {
    id: name,
    name,
    display_name: null,
    agent_type: "mcp",
    description: null,
    enabled: true,
    trust_level: "full",
    last_seen_at: null,
    memory_count: 0,
    created_at: 0,
    updated_at: 0,
    ...overrides,
  };
}

// `last_seen_at` and the wizard's entry time are whole epoch seconds. The
// cutoff is inclusive, so a tool's first call in the second the wizard opened
// is a sighting; a second earlier is still before the wizard.
describe("heardSince", () => {
  const SINCE = 1_800_000_000;

  it("counts a call in the same second", () => {
    expect(heardSince({ last_seen_at: SINCE }, SINCE)).toBe(true);
  });

  it("counts a later call", () => {
    expect(heardSince({ last_seen_at: SINCE + 1 }, SINCE)).toBe(true);
  });

  it("does not count a call one second earlier", () => {
    expect(heardSince({ last_seen_at: SINCE - 1 }, SINCE)).toBe(false);
  });

  it("does not count an identity that was never heard from", () => {
    expect(heardSince({ last_seen_at: null }, SINCE)).toBe(false);
  });
});

describe("seenFamiliesOf with a cutoff", () => {
  const SINCE = 1_800_000_000;

  it("includes a family heard in the same second and excludes one a second earlier", () => {
    const seen = seenFamiliesOf(
      [
        agent("cursor", { last_seen_at: SINCE }),
        agent("claude-desktop", { last_seen_at: SINCE - 1 }),
      ],
      SINCE,
    );

    expect(seen.has("cursor")).toBe(true);
    expect(seen.has("claude-desktop")).toBe(false);
  });

  it("ignores a stored memory when a cutoff is given: only a call since then counts", () => {
    const seen = seenFamiliesOf([agent("cursor", { memory_count: 9, last_seen_at: SINCE - 60 })], SINCE);

    expect(seen.has("cursor")).toBe(false);
  });

  it("without a cutoff, any sighting or stored memory counts", () => {
    const seen = seenFamiliesOf(
      [agent("cursor", { last_seen_at: 1 }), agent("codex-mcp-client", { memory_count: 2 })],
      undefined,
    );

    expect(seen.has("cursor")).toBe(true);
    expect(seen.has("codex")).toBe(true);
  });
});
