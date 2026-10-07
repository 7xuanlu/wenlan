// SPDX-License-Identifier: AGPL-3.0-only
import { createSpacesNavigationFixture, type SpacesNavigationFixture } from "./spacesNavigation";

export function createSpaceProjectFixture(empty = false): SpacesNavigationFixture {
  const fixture = createSpacesNavigationFixture();
  const wenlan = { ...fixture.spaces[0], id: "project-wenlan", name: "Wenlan", description: "Notes and references for a simpler writing experience." };
  const reading = { ...wenlan, id: "project-reading", name: "讀書會", description: "一起閱讀，留下自己的理解。" };
  return { ...fixture, spaces: [wenlan, reading], pages: fixture.pages.slice(0, 2).map(page => ({ ...page, domain: "Wenlan", space: "Wenlan" })), documents: empty ? [] : [
    { file: { source: "file", source_id: "design.md", title: "Writing experience observations", chunk_count: 1, last_modified: 1, domain: "Wenlan", space: "Wenlan" }, content: "# Writing experience observations\n\nStart with the note itself. Keep references close when the reader needs them." },
    { file: { source: "webpage", source_id: "https://example.com/reading", title: "A reference for reading", chunk_count: 1, last_modified: 1, domain: "Wenlan", space: "Wenlan" }, content: "Saved reference text for the Wenlan project. This is isolated fixture content." },
    { file: { source: "file", source_id: "design.md", title: "讀書會摘錄", chunk_count: 1, last_modified: 1, domain: "讀書會", space: "讀書會" }, content: "這份文件只屬於讀書會，用來驗證來源讀取不會跨越空間。" },
  ] };
}
