// SPDX-License-Identifier: AGPL-3.0-only
import { describe, it, expect } from "vitest";
import {
  getSampleData,
  resolveSampleLocale,
  type SampleLocale,
} from "../firstUseSampleData";

const LOCALES: SampleLocale[] = ["en", "zh-Hans", "zh-Hant"];

describe("sample dataset integrity", () => {
  it.each(LOCALES)("keeps every %s citation quote inside its source body", (locale) => {
    const data = getSampleData(locale);
    const bodies = new Map(data.sources.map((source) => [source.id, source.body]));
    expect(data.sources.length).toBe(3);
    for (const citation of data.citations) {
      const body = bodies.get(citation.sourceId);
      expect(body, citation.id).toBeDefined();
      const joined = (body ?? []).join("\n");
      expect(
        joined.includes(citation.quote),
        `${locale}:${citation.id}`,
      ).toBe(true);
    }
  });

  it.each(LOCALES)("resolves every %s paragraph citation to a real quote", (locale) => {
    const data = getSampleData(locale);
    const quotes = new Set(data.citations.map((citation) => citation.id));
    for (const paragraph of data.page.paragraphs) {
      expect(paragraph.citationIds.length).toBeGreaterThan(0);
      for (const id of paragraph.citationIds) {
        expect(quotes.has(id), `${locale}:${id}`).toBe(true);
      }
    }
    expect(data.page.related.title).toBeTruthy();
    expect(data.page.related.body).toBeTruthy();
  });

  it.each(LOCALES)("gives %s distinct own-data commands per client", (locale) => {
    const data = getSampleData(locale);
    const { chatgpt, codex, claude } = data.commands;
    expect(chatgpt.recall).toMatch(/@/);
    expect(codex.recall.startsWith("/recall")).toBe(true);
    expect(claude.recall.startsWith("/recall")).toBe(true);
    expect(chatgpt.recall).not.toBe(codex.recall);
    for (const commands of [chatgpt, codex, claude]) {
      expect(commands.recall).toBeTruthy();
      expect(commands.handoff).toBeTruthy();
      expect(commands.brief).toBeTruthy();
    }
  });
});

describe("resolveSampleLocale", () => {
  it.each([
    [undefined, "en"],
    ["en", "en"],
    ["en-US", "en"],
    ["fr", "en"],
    ["zh", "zh-Hans"],
    ["zh-CN", "zh-Hans"],
    ["zh-Hans", "zh-Hans"],
    ["zh-TW", "zh-Hant"],
    ["zh-HK", "zh-Hant"],
    ["zh-Hant", "zh-Hant"],
    ["zh-Hant-HK", "zh-Hant"],
  ])("maps %s to %s", (input, expected) => {
    expect(resolveSampleLocale(input)).toBe(expected);
  });
});
