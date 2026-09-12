// SPDX-License-Identifier: AGPL-3.0-only
import type { ReactNode } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AssetCard } from "./AssetCard";

function renderCard({
  onOpen = vi.fn(),
  context = "A page can stand on its own.",
  footer = <span>footer</span>,
}: {
  onOpen?: () => void;
  context?: string | null;
  footer?: ReactNode;
} = {}) {
  return {
    onOpen,
    ...render(
      <AssetCard
        context={context}
        footer={footer}
        onOpen={onOpen}
        openLabel="Open Independent research note"
        testId="card-1"
        title="Independent research note"
      />,
    ),
  };
}

describe("AssetCard", () => {
  it("renders title, context, and footer", () => {
    renderCard();
    expect(screen.getByText("Independent research note")).toBeInTheDocument();
    expect(screen.getByText("A page can stand on its own.")).toBeInTheDocument();
    expect(screen.getByText("footer")).toBeInTheDocument();
  });

  it("opens through the title button by click and keyboard", async () => {
    const user = userEvent.setup();
    const { onOpen } = renderCard();

    const openControl = screen.getByRole("button", {
      name: "Open Independent research note",
    });
    await user.click(openControl);
    expect(onOpen).toHaveBeenCalledTimes(1);

    openControl.focus();
    await user.keyboard("{Enter}");
    expect(onOpen).toHaveBeenCalledTimes(2);

    await user.keyboard(" ");
    expect(onOpen).toHaveBeenCalledTimes(3);
  });

  it("renders no context node when the summary is absent", () => {
    // Rendered directly: the renderCard helper defaults an omitted context
    // to sample copy, which would mask the absent case.
    const shared = {
      footer: <span>footer</span>,
      onOpen: vi.fn(),
      openLabel: "Open Independent research note",
      testId: "card-1",
      title: "Independent research note",
    };
    const withoutContext = render(<AssetCard {...shared} context={undefined} />);
    expect(withoutContext.container.querySelector(".asset-card-context")).toBeNull();
    withoutContext.unmount();

    const withNullContext = render(<AssetCard {...shared} context={null} />);
    expect(withNullContext.container.querySelector(".asset-card-context")).toBeNull();
    withNullContext.unmount();
  });

  it("keeps footer controls interactive without nesting them in the button", async () => {
    const user = userEvent.setup();
    const onSelectSpace = vi.fn();
    renderCard({
      footer: (
        <button onClick={onSelectSpace} type="button">
          Research
        </button>
      ),
    });

    const openControl = screen.getByRole("button", {
      name: "Open Independent research note",
    });
    // The footer control is a sibling of the open button, never a descendant.
    expect(openControl.querySelector("button")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Research" }));
    expect(onSelectSpace).toHaveBeenCalledTimes(1);
  });

  it("renders the title as plain text when there is no open action", () => {
    const { container } = render(
      <AssetCard footer={<span>footer</span>} testId="card-no-open" title="Detected entity" />,
    );

    expect(screen.getByText("Detected entity")).toBeInTheDocument();
    expect(screen.queryByRole("button")).toBeNull();
    expect(container.querySelector(".asset-card-open")).toBeNull();
    expect(container.querySelector("span.asset-card-title")).not.toBeNull();
  });

  it("renders the future avatar slot above the title", () => {
    const { container } = render(
      <AssetCard
        footer={<span>footer</span>}
        onOpen={vi.fn()}
        openLabel="Open slot card"
        testId="card-slot"
        title="Slot card"
      >
        <span data-testid="card-mark">mark</span>
      </AssetCard>,
    );

    const card = screen.getByTestId("card-slot");
    expect(screen.getByText("Slot card")).toBeInTheDocument();
    const order = Array.from(card.children);
    expect(order[0].getAttribute("data-testid")).toBe("card-mark");
    expect(order[1].classList.contains("asset-card-open")).toBe(true);
    expect(container.querySelector("article.asset-card")).not.toBeNull();
  });
});
