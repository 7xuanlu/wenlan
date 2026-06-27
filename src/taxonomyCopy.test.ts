import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function read(path: string): string {
  return readFileSync(resolve(root, path), "utf8");
}

describe("taxonomy and product copy", () => {
  it("advertises canonical Wenlan v0.9 memory types without legacy goal", () => {
    const tauri = read("src/lib/tauri.ts");

    expect(tauri).toContain(
      "export type MemoryType = 'identity' | 'preference' | 'decision' | 'lesson' | 'gotcha' | 'fact';",
    );
    expect(tauri).toContain("{ type: 'lesson', label: 'Lesson'");
    expect(tauri).toContain("{ type: 'gotcha', label: 'Gotcha'");
    expect(tauri).not.toContain("{ type: 'goal', label: 'Goal'");
  });

  it("keeps legacy goal compatibility only outside advertised facets", () => {
    const tauri = read("src/lib/tauri.ts");

    expect(tauri).toContain("goal: \"bg-emerald-500/20");
    expect(tauri).toContain("goal: \"protected\"");
    expect(read("src/components/memory/StructuredEditor.tsx")).toContain(
      "legacy_goal",
    );
  });

  it("does not advertise goal in import or agent-trust copy", () => {
    const importView = read("src/components/memory/ImportView.tsx");
    const agents = read("src/lib/agents.ts");

    expect(importView).toContain(
      "TYPE must be exactly one of: identity, preference, decision, lesson, gotcha, fact",
    );
    expect(importView).not.toContain(
      "TYPE must be exactly one of: identity, preference, decision, fact, goal",
    );
    expect(importView).not.toContain("[goal]");
    expect(agents).not.toMatch(/\bgoals?\b/i);
  });

  it("keeps user-facing page copy out of legacy concept wording", () => {
    const productOwnedFiles = [
      "src/components/onboarding/WhatHappensNextCard.tsx",
      "src/components/onboarding/FirstPageModal.tsx",
      "src/components/onboarding/GhostPagesRow.tsx",
      "src/components/ChatImport/ImportProgress.tsx",
      "src/components/memory/Greeting.tsx",
      "src/components/memory/ActivityFeed.tsx",
      "src/components/memory/PageDetail.tsx",
      "src/components/memory/SpaceDetail.tsx",
      "src/components/memory/sources/SourcesSection.tsx",
    ];

    for (const file of productOwnedFiles) {
      const content = read(file).replaceAll("#concept:", "#legacy-page-anchor:");
      expect(content, file).not.toMatch(/\b[Cc]oncepts?\b/);
      expect(content, file).not.toMatch(/\bconcept files\b/i);
      expect(content, file).not.toMatch(/\bOpen concept\b/i);
      expect(content, file).not.toMatch(/\bEdit concept\b/i);
      expect(content, file).not.toMatch(/\bDelete this concept\b/i);
    }
  });
});
