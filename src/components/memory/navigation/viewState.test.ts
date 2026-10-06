import { describe, expect, it } from "vitest";
import { activeNavigationForView } from "./viewState";

describe("activeNavigationForView", () => {
  it("keeps the first-use guide and legacy Home alias in Wiki navigation", () => {
    expect(activeNavigationForView({ kind: "home" })).toBe("pages");
    expect(activeNavigationForView({ kind: "first-use" })).toBe("pages");
    expect(activeNavigationForView({
      kind: "first-use",
      showKnowledge: true,
      batchId: "batch-1",
    })).toBe("pages");
  });
  it("keeps the Page library and Page detail first-class instead of nesting them under Spaces", () => {
    expect(activeNavigationForView({ kind: "pages" })).toBe("pages");
    expect(activeNavigationForView({ kind: "page", pageId: "page-1" })).toBe("pages");
    expect(activeNavigationForView({
      kind: "page-draft",
      draftId: "draft-1",
      space: "Launch",
    })).toBe("pages");
    expect(activeNavigationForView({ kind: "distill-review" })).toBe("pages");
    expect(activeNavigationForView({ kind: "space", spaceId: "space-1", spaceName: "Launch" })).toBe("spaces");
  });
  it("keeps topic detail in the same navigation context as topic browsing", () => {
    expect(activeNavigationForView({ kind: "entities" })).toBe("entities");
    expect(activeNavigationForView({ kind: "entity", entityId: "entity-1" })).toBe("entities");
  });
});
