// SPDX-License-Identifier: AGPL-3.0-only
import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { ReferenceNavigationProvider } from "./ReferenceNavigationContext";
import { ReferenceLink } from "./ReferenceLink";
import { referenceTargetFromHref } from "./referenceTypes";

describe("ReferenceLink", () => {
  it("keeps a touch preview open after WebKit sends pointerleave before click", async () => {
    vi.useFakeTimers();
    try {
      const onOpenMemory = vi.fn();
      const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      client.setQueryData(["reference-preview", "memory", "m-1"], { source_id: "m-1", title: "Related", content: "Stored context." });
      render(<QueryClientProvider client={client}><ReferenceNavigationProvider onOpenMemory={onOpenMemory}>
        <ReferenceLink href="#memory:m-1">Related</ReferenceLink>
      </ReferenceNavigationProvider></QueryClientProvider>);
      const link = screen.getByRole("link", { name: "Related" });
      const down = new MouseEvent("pointerdown", { bubbles: true, button: 0 });
      Object.defineProperty(down, "pointerType", { value: "touch" });
      fireEvent(link, down);
      fireEvent.pointerLeave(link);
      fireEvent.click(link);
      await act(async () => { await vi.advanceTimersByTimeAsync(200); });
      expect(screen.getByRole("dialog", { name: "Related" })).toHaveTextContent("Stored context.");
      expect(onOpenMemory).not.toHaveBeenCalled();
    } finally { vi.useRealTimers(); }
  });

  it("shows the reference type and routes canonical links without changing the fragment", () => {
    const onOpenPage = vi.fn();
    render(
      <ReferenceNavigationProvider onOpenPage={onOpenPage} onOpenMemory={vi.fn()}>
        <ReferenceLink href="#concept:page-17">A saved page</ReferenceLink>
      </ReferenceNavigationProvider>,
    );

    const link = screen.getByRole("link", { name: "A saved page" });
    expect(link).toHaveAttribute("href", "#concept:page-17");
    expect(link.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
    fireEvent.click(link, { button: 0 });
    expect(onOpenPage).toHaveBeenCalledWith("page-17");
    expect(window.location.hash).toBe("");
  });

  it("leaves modified clicks under browser control", () => {
    const onOpenPage = vi.fn();
    render(
      <ReferenceNavigationProvider onOpenPage={onOpenPage} onOpenMemory={vi.fn()}>
        <ReferenceLink href="#concept:page-17">A saved page</ReferenceLink>
      </ReferenceNavigationProvider>,
    );

    fireEvent.click(screen.getByRole("link", { name: "A saved page" }), {
      button: 0,
      metaKey: true,
    });
    expect(onOpenPage).not.toHaveBeenCalled();
  });

  it("routes exact memory anchors and rejects other fragment shapes", () => {
    const onOpenMemory = vi.fn();
    render(
      <ReferenceNavigationProvider onOpenPage={vi.fn()} onOpenMemory={onOpenMemory}>
        <ReferenceLink href="#memory:memory-42">Related memory</ReferenceLink>
      </ReferenceNavigationProvider>,
    );
    const link = screen.getByRole("link", { name: "Related memory" });
    expect(link).toHaveAttribute("data-reference-kind", "memory");
    fireEvent.click(link, { button: 0 });
    expect(onOpenMemory).toHaveBeenCalledWith("memory-42");
    expect(referenceTargetFromHref("#concept:page-1#heading")).toBeNull();
    expect(referenceTargetFromHref("#memory:")).toBeNull();
    expect(referenceTargetFromHref("https://example.com/#concept:page-1")).toBeNull();
  });
});
