// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import "./quickCaptureScrim.css";

type Placement = "bottom-right" | "centered-over-main";

export default function QuickCaptureScrim() {
  const [open, setOpen] = useState(false);
  useEffect(() => {
    const opened = listen<Placement>("quick-capture-opened", (event) => setOpen(event.payload === "centered-over-main"));
    const closed = listen("quick-capture-closed", () => setOpen(false));
    return () => { opened.then((fn) => fn()); closed.then((fn) => fn()); };
  }, []);
  useEffect(() => {
    if (!open) return;
    // Capture phase so this runs before Main's bubble-phase Escape branch
    // regardless of subscription order; preventDefault marks the event as
    // handled so Main skips its own Escape handling (clearing search, back
    // navigation) for the same keypress.
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        void invoke("dismiss_quick_capture");
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open]);
  if (!open) return null;
  return <div className="quick-capture-scrim" data-testid="quick-capture-scrim" onClick={() => void invoke("dismiss_quick_capture")} />;
}
