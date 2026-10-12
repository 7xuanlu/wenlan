// SPDX-License-Identifier: AGPL-3.0-only
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { MemoryItem } from "../../../lib/tauri";
import type { ReferencePreviewRequest, ReferenceTarget } from "../links/referenceTypes";
import { ReferenceNavigationProvider } from "../links/ReferenceNavigationContext";

type MarkdownEditorMockProps = {
  onReferenceActivate?: (target: ReferenceTarget, anchor: HTMLAnchorElement) => void;
  onReferencePreview?: (target: ReferenceTarget, anchor: HTMLAnchorElement, keyboard: boolean) => void;
};

type ReferencePreviewMockProps = {
  request: ReferencePreviewRequest | null;
  onOpenPage?: (pageId: string) => void;
  onOpenMemory?: (sourceId: string) => void;
};

vi.mock("../../../lib/tauri", () => ({
  getMemoryDetail: vi.fn().mockResolvedValue(null),
  updateMemory: vi.fn(),
}));

vi.mock("./MarkdownEditor", async () => {
  const React = await import("react");
  return { MarkdownEditor: (props: MarkdownEditorMockProps) => {
    const anchor = React.useRef<HTMLAnchorElement>(null);
    const memory: ReferenceTarget = { kind: "memory", id: "linked-memory" };
    const page: ReferenceTarget = { kind: "page", id: "linked-page" };
    return <>
      <a ref={anchor} href="#memory:linked-memory">Linked memory</a>
      <button type="button" onClick={() => anchor.current && props.onReferenceActivate?.(memory, anchor.current)}>Activate memory</button>
      <button type="button" onClick={() => anchor.current && props.onReferenceActivate?.(page, anchor.current)}>Activate page</button>
      <button type="button" onClick={() => anchor.current && props.onReferencePreview?.(memory, anchor.current, true)}>Preview memory</button>
    </>;
  } };
});

vi.mock("../links/ReferencePreview", () => ({
  ReferencePreview: ({ request, onOpenPage, onOpenMemory }: ReferencePreviewMockProps) => {
    if (!request) return null;
    const target = request.target;
    return <aside data-testid="reference-preview" data-target-kind={target.kind}>
      <button type="button" onClick={() => {
        if (target.kind === "page") onOpenPage?.(target.id);
        else if (target.kind === "memory") onOpenMemory?.(target.id);
      }}>Open preview target</button>
    </aside>;
  },
}));

import { MemoryDocumentEditor } from "./MemoryDocumentEditor";

const record: MemoryItem = {
  source_id: "memory-current", title: "Current memory", content: "Current content", summary: null,
  memory_type: "fact", domain: null, source_agent: null, confidence: 0.8, confirmed: false,
  pinned: false, supersedes: null, last_modified: 1, chunk_count: 1,
};

describe("MemoryDocumentEditor references", () => {
  beforeEach(() => vi.clearAllMocks());

  function mount() {
    const onOpenPage = vi.fn();
    const onOpenMemory = vi.fn();
    const client = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
    render(
      <QueryClientProvider client={client}>
        <ReferenceNavigationProvider onOpenPage={onOpenPage} onOpenMemory={onOpenMemory}>
          <MemoryDocumentEditor memory={record} canonical={record} disabled={false}
            onRegisterFlush={() => {}} onBusyChange={() => {}} />
        </ReferenceNavigationProvider>
      </QueryClientProvider>,
    );
    return { onOpenPage, onOpenMemory };
  }

  it("routes direct page and memory references through shared navigation", () => {
    const { onOpenPage, onOpenMemory } = mount();
    fireEvent.click(screen.getByRole("button", { name: "Activate memory" }));
    expect(onOpenMemory).toHaveBeenCalledWith("linked-memory");
    fireEvent.click(screen.getByRole("button", { name: "Activate page" }));
    expect(onOpenPage).toHaveBeenCalledWith("linked-page");
  });

  it("shows a shared memory preview and routes its open action", () => {
    const { onOpenMemory } = mount();
    fireEvent.click(screen.getByRole("button", { name: "Preview memory" }));
    expect(screen.getByTestId("reference-preview")).toHaveAttribute("data-target-kind", "memory");
    fireEvent.click(screen.getByRole("button", { name: "Open preview target" }));
    expect(onOpenMemory).toHaveBeenCalledWith("linked-memory");
  });
});
