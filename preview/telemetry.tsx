// SPDX-License-Identifier: AGPL-3.0-only
// Isolated rendered consent QA. Never contacts a daemon or telemetry receiver.
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import GeneralSection from "../src/components/memory/settings/sections/GeneralSection";
import { initializeI18n } from "../src/i18n";
import { APP_LOCALE_STORAGE_KEY } from "../src/i18n/locales";
import { applyTheme } from "../src/lib/theme";
import "../src/index.css";

const params = new URLSearchParams(location.search);
const locale = params.get("locale") ?? "en";
localStorage.setItem(APP_LOCALE_STORAGE_KEY, locale);
const mode = params.get("state") ?? "off";
let enabled = mode === "on" || mode === "unavailable-on";
const calls: string[] = [];
Object.assign(window, {
  __telemetryFixtureCalls: calls,
  __TAURI_INTERNALS__: {
    invoke: async (command: string, args?: { enabled?: boolean }) => {
      calls.push(command);
      switch (command) {
        case "get_profile": return null;
        case "is_run_at_login_enabled": return false;
        case "get_telemetry_status":
          if (mode === "unknown") throw new Error("fixture read failed");
          return {enabled, available: !mode.startsWith("unavailable"), pending_operations: enabled ? 3 : 0};
        case "set_telemetry_enabled":
          if (mode === "save-failure") throw new Error("private fixture error");
          enabled = args?.enabled === true;
          return {enabled, available: true, pending_operations: 0};
        default: throw new Error(`Unmocked consent-fixture command: ${command}`);
      }
    },
  },
});
applyTheme();
await initializeI18n();
const client = new QueryClient({ defaultOptions: {queries: {retry: false}, mutations: {retry: false}} });
ReactDOM.createRoot(document.getElementById("root")!).render(
  <QueryClientProvider client={client}>
    <main style={{maxWidth: 880, padding: 24, margin: "auto", color: "var(--mem-text)", background: "var(--mem-bg)", minHeight: "100vh", fontFamily: "var(--mem-font-body)"}}>
      <p style={{fontSize: 12, marginBottom: 20}}>Isolated consent fixture · no daemon / no data sent</p>
      <GeneralSection />
    </main>
  </QueryClientProvider>,
);
