// SPDX-License-Identifier: AGPL-3.0-only
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import AboutWenlanDialog from "./AboutWenlanDialog";
const api = vi.hoisted(() => ({ emit: vi.fn(), listen: vi.fn(), open: vi.fn(), getVersion: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: api.listen, emit: api.emit }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: api.getVersion }));
vi.mock("@tauri-apps/plugin-shell", () => ({ open: api.open }));
describe("About Wenlan", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    api.getVersion.mockResolvedValue("0.18.4");
    api.listen.mockResolvedValue(vi.fn());
    api.emit.mockResolvedValue(undefined);
  });
  it("shows the installed version and checks without closing the dialog", async () => {
    render(<AboutWenlanDialog open onClose={vi.fn()} />);
    expect(await screen.findByText("Version 0.18.4")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(api.emit).toHaveBeenCalledWith("updater://check-now");
    expect(screen.getByRole("button", { name: "Check for updates" })).toBeDisabled();
    const handler = api.listen.mock.calls[0][1];
    act(() => handler({ payload: { state: "current" } }));
    expect(screen.getByText("You’re up to date.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Check for updates" })).toBeEnabled();
  });
  it("traps keyboard focus, closes with Escape and restores the trigger", async () => {
    const trigger = document.createElement("button"); document.body.append(trigger); trigger.focus();
    const onClose = vi.fn();
    const { unmount } = render(<AboutWenlanDialog open onClose={onClose} />);
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveFocus();
    fireEvent.keyDown(dialog, {key:"Tab", shiftKey:true});
    expect(screen.getByRole("link", {name:"Report an issue"})).toHaveFocus();
    fireEvent.keyDown(document.activeElement!, {key:"Tab"});
    expect(screen.getByRole("button", {name:"Close"})).toHaveFocus();
    fireEvent.keyDown(dialog, {key:"Escape"}); expect(onClose).toHaveBeenCalledOnce();
    unmount(); expect(trigger).toHaveFocus(); trigger.remove();
  });
  it("makes check failures retryable and opens real support links", async () => {
    api.emit.mockImplementation((event: string) => event === "updater://check-now" ? Promise.reject(new Error("offline")) : Promise.resolve());
    api.open.mockResolvedValue(undefined);
    render(<AboutWenlanDialog open onClose={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", {name:"Check for updates"}));
    expect(await screen.findByText("Couldn’t check for updates. Try again.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("link",{name:"Release notes"}));
    await waitFor(() => expect(api.open).toHaveBeenCalledWith("https://github.com/7xuanlu/wenlan/releases"));
  });
});
