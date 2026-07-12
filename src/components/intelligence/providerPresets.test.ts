// SPDX-License-Identifier: AGPL-3.0-only
import { describe, it, expect } from "vitest";
import {
  PROVIDER_PRESETS,
  keyPrefixMismatch,
  type ProviderPreset,
} from "./providerPresets";

const byId = (id: string): ProviderPreset => {
  const p = PROVIDER_PRESETS.find((x) => x.id === id);
  if (!p) throw new Error(`preset ${id} missing`);
  return p;
};

describe("providerPresets — §9.1 data shape", () => {
  it("carries the exact per-provider key metadata", () => {
    expect(byId("openai")).toMatchObject({
      name: "OpenAI",
      keyPlaceholder: "sk-proj-...",
      keyPrefixes: ["sk-"],
      getKeyUrl: "https://platform.openai.com/api-keys",
    });
    expect(byId("gemini")).toMatchObject({
      name: "Gemini",
      keyPlaceholder: "AIzaSy... or AQ....",
      keyPrefixes: ["AIzaSy", "AQ."],
      getKeyUrl: "https://aistudio.google.com/apikey",
    });
    expect(byId("groq")).toMatchObject({
      name: "Groq",
      keyPrefixes: ["gsk_"],
      getKeyUrl: "https://console.groq.com/keys",
    });
    expect(byId("openrouter")).toMatchObject({
      name: "OpenRouter",
      keyPrefixes: ["sk-or-"],
      getKeyUrl: "https://openrouter.ai/keys",
    });
    expect(byId("deepseek")).toMatchObject({
      name: "DeepSeek",
      keyPrefixes: ["sk-"],
      getKeyUrl: "https://platform.deepseek.com/api_keys",
    });
    expect(byId("xai")).toMatchObject({
      name: "xAI (Grok)",
      keyPlaceholder: "xai-...",
      keyPrefixes: ["xai-"],
      getKeyUrl: "https://console.x.ai",
    });
  });

  it("Mistral is keyed but opaque — a getKeyUrl, no prefixes", () => {
    const mistral = byId("mistral");
    expect(mistral.name).toBe("Mistral");
    expect(mistral.getKeyUrl).toBe("https://console.mistral.ai");
    expect(mistral.keyPrefixes).toBeUndefined();
  });

  it("keeps ids stable and endpoints unchanged after the renames", () => {
    expect(byId("gemini").endpoint).toBe(
      "https://generativelanguage.googleapis.com/v1beta/openai",
    );
    expect(byId("xai").endpoint).toBe("https://api.x.ai/v1");
  });
});

describe("keyPrefixMismatch — §9.1 soft validation", () => {
  it("no hint for an empty key", () => {
    expect(keyPrefixMismatch(byId("openai"), "")).toBe(false);
    expect(keyPrefixMismatch(byId("openai"), "   ")).toBe(false);
  });
  it("no hint when a prefix matches", () => {
    expect(keyPrefixMismatch(byId("openai"), "sk-proj-abc")).toBe(false);
    expect(keyPrefixMismatch(byId("xai"), "xai-abc")).toBe(false);
  });
  it("hint when no prefix matches", () => {
    expect(keyPrefixMismatch(byId("openai"), "nope-123")).toBe(true);
    expect(keyPrefixMismatch(byId("groq"), "sk-123")).toBe(true);
  });
  it("Gemini accepts either live format — neither mismatches", () => {
    expect(keyPrefixMismatch(byId("gemini"), "AIzaSyABC")).toBe(false);
    expect(keyPrefixMismatch(byId("gemini"), "AQ.abc")).toBe(false);
    expect(keyPrefixMismatch(byId("gemini"), "xyz")).toBe(true);
  });
  it("Mistral never hints (no documented prefix)", () => {
    expect(keyPrefixMismatch(byId("mistral"), "anything")).toBe(false);
  });
});
