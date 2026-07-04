// SPDX-License-Identifier: AGPL-3.0-only
// Browser preview harness for the page-detail citations redesign.
import { StrictMode, useState } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import PageDetail from "../src/components/memory/PageDetail";
import "../src/index.css";

const VARIANTS = [
  { id: "page-cited", label: "Cited (all kinds)" },
  { id: "page-cleared", label: "Edit-cleared" },
  { id: "page-mismatch", label: "Mismatch" },
  { id: "page-plain", label: "No citations" },
];

const client = new QueryClient({
  defaultOptions: { queries: { retry: false, staleTime: 0, gcTime: 0 } },
});

function Harness() {
  const [pageId, setPageId] = useState("page-cited");
  const [theme, setTheme] = useState("dark");

  const applyTheme = (next: string) => {
    document.documentElement.setAttribute("data-theme", next);
    setTheme(next);
  };

  return (
    <div style={{ minHeight: "100vh", background: "var(--mem-bg)" }}>
      <div
        style={{
          display: "flex",
          gap: 8,
          padding: "10px 16px",
          borderBottom: "1px solid var(--mem-border)",
          alignItems: "center",
          fontFamily: "var(--mem-font-mono)",
          fontSize: 12,
          color: "var(--mem-text-secondary)",
        }}
      >
        <span style={{ fontWeight: 600 }}>PREVIEW</span>
        {VARIANTS.map((v) => (
          <button
            key={v.id}
            onClick={() => setPageId(v.id)}
            style={{
              padding: "3px 10px",
              borderRadius: 6,
              border: "1px solid var(--mem-border)",
              background: pageId === v.id ? "var(--mem-accent, #6366f1)" : "transparent",
              color: pageId === v.id ? "#fff" : "inherit",
              cursor: "pointer",
            }}
          >
            {v.label}
          </button>
        ))}
        <button
          onClick={() => applyTheme(theme === "dark" ? "light" : "dark")}
          style={{
            marginLeft: "auto",
            padding: "3px 10px",
            borderRadius: 6,
            border: "1px solid var(--mem-border)",
            background: "transparent",
            color: "inherit",
            cursor: "pointer",
          }}
        >
          {theme === "dark" ? "☀ light" : "☾ dark"}
        </button>
      </div>
      <div style={{ maxWidth: 860, margin: "0 auto", padding: "24px 16px" }}>
        <PageDetail
          key={pageId}
          pageId={pageId}
          onBack={() => console.log("[preview] onBack")}
          onMemoryClick={(id: string) => console.log("[preview] onMemoryClick:", id)}
          onPageClick={(id: string) => {
            console.log("[preview] onPageClick:", id);
            setPageId(id);
          }}
        />
      </div>
    </div>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={client}>
      <Harness />
    </QueryClientProvider>
  </StrictMode>,
);
