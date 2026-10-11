// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { takeRemoteAccessNotice, type RemoteAccessNotice } from "../../lib/tauri";

/** The native side reports a kind, never words. These are the words. */
export const NOTICE_MESSAGE_KEY = {
  stopped: "remoteAccess.notifyStopped",
  expired: "remoteAccess.notifyExpired",
} as const satisfies Record<RemoteAccessNotice["kind"], string>;

async function notify(title: string, body: string) {
  try {
    const { sendNotification, isPermissionGranted, requestPermission } =
      await import("@tauri-apps/plugin-notification");
    let granted = await isPermissionGranted();
    if (!granted) granted = (await requestPermission()) === "granted";
    if (granted) await sendNotification({ title, body });
  } catch {
    // Notifications are unavailable or turned off. The settings panel still says so.
  }
}

/**
 * Tells the person, with a system notification, when Web access stops on its
 * own: the connection gave up retrying, or the key ran out. It mounts with
 * the app shell, so it works whichever screen is open and while the window is
 * closed to the tray. The native event is only a wake-up; the notice itself is
 * taken from the native side once, so it is never shown twice.
 */
export default function RemoteAccessNotifier() {
  const { t } = useTranslation();
  // The latest translator, so a language change applies without re-subscribing.
  const translate = useRef(t);
  translate.current = t;

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    const drain = async () => {
      try {
        const notice = await takeRemoteAccessNotice();
        if (disposed || !notice) return;
        const key = NOTICE_MESSAGE_KEY[notice.kind];
        if (key) await notify("Wenlan", translate.current(key));
      } catch {
        // Nothing waiting, or the native side is not reachable yet.
      }
    };
    listen("remote-access-notice", () => { void drain(); })
      .then((stop) => { if (disposed) stop(); else unlisten = stop; })
      .catch(() => {});
    // A notice may have been left before this window was listening.
    void drain();
    return () => { disposed = true; unlisten?.(); };
  }, []);

  return null;
}
