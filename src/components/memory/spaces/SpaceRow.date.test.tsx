import { render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { i18n } from "../../../i18n";
import { SpaceRow } from "./SpaceRow";
import { labels, makeSpace } from "./SpacesOverview.testUtils";

function renderRow(updatedAt: number): HTMLElement {
  const space = makeSpace({ updated_at: updatedAt });
  render(
    <SpaceRow
      space={space}
      spaces={[space]}
      labels={labels}
      pageCount={2}
      pending={false}
      canMoveUp={false}
      canMoveDown={false}
      onSelect={vi.fn()}
      onStar={vi.fn()}
      onRename={vi.fn()}
      onMoveUp={vi.fn()}
      onMoveDown={vi.fn()}
      onDelete={vi.fn()}
      onDragStart={vi.fn()}
    />,
  );
  return screen.getByTestId(`space-row-${space.id}`);
}

const invalidTimestamps = [
  ["zero", 0],
  ["negative", -1],
  ["NaN", Number.NaN],
  ["positive infinity", Number.POSITIVE_INFINITY],
  ["negative infinity", Number.NEGATIVE_INFINITY],
  ["out of Date range", 8_640_000_000_001],
] as const;

describe("SpaceRow quiet metadata", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it.each([...invalidTimestamps, ["valid", 1_720_569_600]] as const)("retains only page counts with %s timestamp", (_name, updatedAt) => {
    const row = renderRow(updatedAt);
    expect(within(row).getByTestId("space-pages")).toHaveTextContent("2");
    expect(within(row).getByTestId("space-mobile-pages")).toHaveTextContent("2");
    expect(within(row).queryByTestId("space-memories")).not.toBeInTheDocument();
    expect(within(row).queryByTestId("space-mobile-memories")).not.toBeInTheDocument();
    expect(row.querySelector("time")).toBeNull();
    expect(row).not.toHaveTextContent(/NaN|Invalid Date|Infinity/);
  });
});
