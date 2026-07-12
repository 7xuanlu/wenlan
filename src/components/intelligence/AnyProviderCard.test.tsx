// SPDX-License-Identifier: AGPL-3.0-only
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ComponentProps } from "react";
import "../../i18n";
import { open as shellOpen } from "@tauri-apps/plugin-shell";

const mocks = vi.hoisted(() => ({
  getDaemonVersion: vi.fn(),
  getExternalLlm: vi.fn(),
  setExternalLlm: vi.fn(),
  testExternalLlm: vi.fn(),
  listExternalModels: vi.fn(),
  getExternalLlmKeyConfigured: vi.fn(),
  getApiKey: vi.fn(),
}));
vi.mock("../../lib/tauri", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../lib/tauri")>();
  return { ...actual, ...mocks };
});

import AnyProviderCard from "./AnyProviderCard";

function renderCard(
  props: Partial<ComponentProps<typeof AnyProviderCard>> = {},
  qc: QueryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
) {
  render(
    <QueryClientProvider client={qc}>
      <AnyProviderCard {...props} />
    </QueryClientProvider>
  );
  return qc;
}

describe("AnyProviderCard", () => {
  // NOTE: reset runs in afterEach, not beforeEach — see
  // src/hooks/useDaemonVersion.test.tsx for why. Resetting a vi.fn() in
  // beforeEach immediately before it's reconfigured with mockRejectedValue
  // and consumed by a react-query queryFn deterministically triggers a
  // false-positive "unhandled rejection" failure on this project's pinned
  // vitest 4.1.5 + @tanstack/react-query 5.100.9 + React 19 combination.
  afterEach(() => {
    Object.values(mocks).forEach((m) => m.mockReset());
  });

  beforeEach(() => {
    mocks.getDaemonVersion.mockResolvedValue("0.12.0");
    mocks.getExternalLlm.mockResolvedValue([null, null]);
    mocks.getExternalLlmKeyConfigured.mockResolvedValue(false);
    mocks.getApiKey.mockResolvedValue(null);
    mocks.listExternalModels.mockResolvedValue(["llama3.2:3b"]);
    mocks.setExternalLlm.mockResolvedValue(undefined);
    mocks.testExternalLlm.mockResolvedValue({ response: "pong" });
  });

  it("preset fills the endpoint and discovers models", async () => {
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "ollama");
    expect(screen.getByLabelText("Endpoint URL")).toHaveValue("http://localhost:11434/v1");
    await waitFor(() =>
      expect(mocks.listExternalModels).toHaveBeenCalledWith("http://localhost:11434/v1", null)
    );
  });

  it("keyed presets are disabled with explanation below daemon 0.13", async () => {
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "openai");
    // Brief's i18n copy reads "API keys ... need Wenlan daemon 0.13+" (correct
    // subject-verb agreement for the plural "API keys"); matching that here.
    expect(await screen.findByText(/need Wenlan daemon 0.13\+/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    // key field hidden below 0.13
    expect(screen.queryByLabelText("API key")).not.toBeInTheDocument();
  });

  it("keyed preset works on 0.13: key field shown, save passes key, Applied note", async () => {
    mocks.getDaemonVersion.mockResolvedValue("0.13.0");
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "openai");
    const keyField = await screen.findByLabelText("API key");
    await userEvent.type(keyField, "sk-test");
    await userEvent.type(screen.getByLabelText("Model"), "gpt-4o-mini");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(mocks.setExternalLlm).toHaveBeenCalledWith(
        "https://api.openai.com/v1", "gpt-4o-mini", "sk-test"
      )
    );
    expect(await screen.findByText(/Applied/)).toBeInTheDocument();
  });

  it("clears the API key when switching presets on 0.13 (no cross-vendor key leak)", async () => {
    mocks.getDaemonVersion.mockResolvedValue("0.13.0");
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "openai");
    const keyField = await screen.findByLabelText("API key");
    await userEvent.type(keyField, "sk-openai-secret");
    await waitFor(() =>
      expect(mocks.listExternalModels).toHaveBeenCalledWith(
        "https://api.openai.com/v1", "sk-openai-secret"
      )
    );
    mocks.listExternalModels.mockClear();

    await userEvent.selectOptions(screen.getByLabelText("Provider"), "groq");

    expect(screen.getByLabelText("API key")).toHaveValue("");
    await waitFor(() =>
      expect(mocks.listExternalModels).toHaveBeenCalledWith(
        "https://api.groq.com/openai/v1", null
      )
    );
    expect(mocks.listExternalModels).not.toHaveBeenCalledWith(
      "https://api.groq.com/openai/v1", "sk-openai-secret"
    );
  });

  it("preserves the stored key on 0.13 when the field is left empty for an already-configured provider", async () => {
    mocks.getDaemonVersion.mockResolvedValue("0.13.0");
    mocks.getExternalLlmKeyConfigured.mockResolvedValue(true);
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "openai");
    await userEvent.type(screen.getByLabelText("Model"), "gpt-4o-mini");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(mocks.setExternalLlm).toHaveBeenCalledWith(
        "https://api.openai.com/v1", "gpt-4o-mini", undefined
      )
    );
  });

  it("keyless save on 0.12 omits the key and shows restart note", async () => {
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "ollama");
    // Discovery succeeds → the model field is a <select> over discovered ids (§9.2).
    await screen.findByText(/Connected to Ollama/);
    await userEvent.selectOptions(screen.getByLabelText("Model"), "llama3.2:3b");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() =>
      expect(mocks.setExternalLlm).toHaveBeenCalledWith(
        "http://localhost:11434/v1", "llama3.2:3b", undefined
      )
    );
    expect(await screen.findByText(/Restart Wenlan to apply/)).toBeInTheDocument();
  });

  it("discovery failure falls back to free-text model entry with hint", async () => {
    mocks.listExternalModels.mockRejectedValue(new Error("ECONNREFUSED"));
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "ollama");
    expect(await screen.findByText(/type a model name/i)).toBeInTheDocument();
    const modelField = screen.getByLabelText("Model");
    expect(modelField.tagName).toBe("INPUT");
    expect(modelField).toBeEnabled();
  });

  it("test button shows verbatim daemon error", async () => {
    mocks.testExternalLlm.mockRejectedValue(new Error("LLM request failed: 401 Unauthorized"));
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "ollama");
    await screen.findByText(/Connected to Ollama/);
    await userEvent.selectOptions(screen.getByLabelText("Model"), "llama3.2:3b");
    await userEvent.click(screen.getByRole("button", { name: "Test" }));
    expect(await screen.findByText(/401 Unauthorized/)).toBeInTheDocument();
  });

  it("settings card: selecting a local preset shows the connected chip and a model select", async () => {
    renderCard(); // all groups — provider <select>, no pills
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "ollama");
    expect(await screen.findByText(/Connected to Ollama — 1 models/)).toBeInTheDocument();
    // No pills in settings: the Provider select is still the picker.
    expect(screen.getByLabelText("Provider")).toBeInTheDocument();
    expect(screen.getByLabelText("Model").tagName).toBe("SELECT");
    // The probe result is announced to screen readers, not just shown visually.
    expect(screen.getByRole("status")).toHaveTextContent("Connected to Ollama — 1 models");
  });

  it("settings card: local preset with discovery failure keeps free text and shows the not-detected chip", async () => {
    mocks.listExternalModels.mockRejectedValue(new Error("ECONNREFUSED"));
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "ollama");
    expect(
      await screen.findByText(/Not detected at localhost:11434 — is Ollama running\?/),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Model").tagName).toBe("INPUT");
  });

  it("settings card: cloud presets keep the datalist input and never show a local chip", async () => {
    mocks.getDaemonVersion.mockResolvedValue("0.13.0");
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "openai");
    await waitFor(() => expect(mocks.listExternalModels).toHaveBeenCalled());
    expect(screen.getByLabelText("Model").tagName).toBe("INPUT");
    expect(screen.queryByText(/Connected to/)).not.toBeInTheDocument();
  });

  it("settings card: selecting a local preset does not duplicate-fetch (chip and select share the one discovery query)", async () => {
    // Unlike the wizard's local pane, the settings card has no dedicated
    // ollama/lmStudio probe running (both stay disabled off `isLocalOnly`) —
    // the chip and the model <select> must both read the single generic
    // `discovery` query for the selected endpoint, never a second fetch.
    // Count calls scoped to the Ollama endpoint instead of clearing mock
    // history before selecting: a `mockClear()` right before the assertion
    // would erase a dedicated local probe's mount-time call too (it fires
    // before this point, not after), silently hiding the very regression
    // this test exists to catch. Filtering by endpoint stays immune to
    // whatever the default preset does at mount and still fails if a
    // regression makes the dedicated local probes run (in addition to
    // `discovery`) for the same endpoint.
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "ollama");
    await screen.findByText(/Connected to Ollama/);
    const ollamaCalls = mocks.listExternalModels.mock.calls.filter(
      ([ep]) => ep === "http://localhost:11434/v1"
    );
    expect(ollamaCalls).toEqual([["http://localhost:11434/v1", null]]);
  });

  it("settings card: hand-editing the endpoint off the selected local preset drops the chip's false claim and the local dropdown (live endpoint decides, not the preset id)", async () => {
    // Task 5b review finding: the chip/select must follow the endpoint that
    // `discovery` actually probed (`trimmedEndpoint`), not just the selected
    // preset's group. Select Ollama, confirm the (correct) baseline chip,
    // then hand-edit Endpoint to a cloud host discovery still answers — the
    // stale "Connected to Ollama" claim and the Ollama-shaped <select> must
    // both disappear, mirroring the wizard's `selectedProbe` invariant.
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "ollama");
    await screen.findByText(/Connected to Ollama/);
    expect(screen.getByLabelText("Model").tagName).toBe("SELECT");

    await userEvent.clear(screen.getByLabelText("Endpoint URL"));
    await userEvent.type(screen.getByLabelText("Endpoint URL"), "https://api.groq.com/openai/v1");

    await waitFor(() =>
      expect(mocks.listExternalModels).toHaveBeenCalledWith("https://api.groq.com/openai/v1", null)
    );
    // The provider picker still shows "Ollama (local)" selected, but the live
    // endpoint now resolves to Groq — no chip may claim a local connection to
    // Ollama, and the model field must fall back to free text, not a
    // dropdown implying these are Ollama's models.
    expect(screen.queryByText(/Connected to Ollama/)).not.toBeInTheDocument();
    expect(screen.getByLabelText("Model").tagName).toBe("INPUT");
  });

  it("disables Save and Test while the model field is empty, even with a valid endpoint (canAct guard)", async () => {
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "ollama");
    // Endpoint is valid (preset default) but no model has been chosen —
    // Save/Test must stay disabled until a model is set.
    expect(screen.getByLabelText("Endpoint URL")).toHaveValue("http://localhost:11434/v1");
    expect(screen.getByLabelText("Model")).toHaveValue("");
    expect(screen.getByRole("button", { name: "Test" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("shows the Anthropic precedence warning when an Anthropic key is configured", async () => {
    mocks.getApiKey.mockResolvedValue("sk-ant-configured");
    renderCard();
    expect(await screen.findByText(/Anthropic takes precedence/)).toBeInTheDocument();
  });

  it("invalidates setup-status, external-llm, and external-llm-key-configured after a successful save (strip staleness fix)", async () => {
    mocks.getDaemonVersion.mockResolvedValue("0.13.0");
    const qc = renderCard();
    const invalidateSpy = vi.spyOn(qc, "invalidateQueries");
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "openai");
    const keyField = await screen.findByLabelText("API key");
    await userEvent.type(keyField, "sk-test");
    await userEvent.type(screen.getByLabelText("Model"), "gpt-4o-mini");
    await userEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(mocks.setExternalLlm).toHaveBeenCalled());

    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["setup-status"] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["external-llm"] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["external-llm-key-configured"] });
  });

  it("shows the provider-shaped key placeholder on 0.13", async () => {
    mocks.getDaemonVersion.mockResolvedValue("0.13.0");
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "openai");
    expect(await screen.findByLabelText("API key")).toHaveAttribute("placeholder", "sk-proj-...");
  });

  it("shows an amber soft hint for a key that matches no prefix, without blocking Save", async () => {
    mocks.getDaemonVersion.mockResolvedValue("0.13.0");
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "openai");
    const keyField = await screen.findByLabelText("API key");
    await userEvent.type(keyField, "nope-123");
    await userEvent.type(screen.getByLabelText("Model"), "gpt-4o-mini");
    expect(screen.getByText(/doesn't match the OpenAI key format/i)).toBeInTheDocument();
    // Soft only — Save stays enabled.
    expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();
  });

  it("shows no hint once the key matches a prefix", async () => {
    mocks.getDaemonVersion.mockResolvedValue("0.13.0");
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "openai");
    await userEvent.type(await screen.findByLabelText("API key"), "sk-proj-abc");
    expect(screen.queryByText(/doesn't match the .* key format/i)).not.toBeInTheDocument();
  });

  it("opens the provider console via the system browser from Get a key", async () => {
    mocks.getDaemonVersion.mockResolvedValue("0.13.0");
    vi.mocked(shellOpen).mockClear();
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "openai");
    await userEvent.click(await screen.findByRole("button", { name: /Get a key/ }));
    expect(shellOpen).toHaveBeenCalledWith("https://platform.openai.com/api-keys");
  });

  it("keeps the API key field's accessible name exact while the hint is visible (describedby, not named-by)", async () => {
    mocks.getDaemonVersion.mockResolvedValue("0.13.0");
    renderCard();
    await userEvent.selectOptions(await screen.findByLabelText("Provider"), "openai");
    const keyField = await screen.findByLabelText("API key");
    await userEvent.type(keyField, "nope-123");
    // Hint is visible now; the accessible name must stay exactly "API key" —
    // the hint sentence must not leak into the input's computed label text.
    const exactField = screen.getByLabelText("API key", { exact: true });
    expect(exactField).toBe(keyField);
    // The hint must instead be wired as a description, not folded into the name.
    const describedbyId = exactField.getAttribute("aria-describedby");
    expect(describedbyId).toBeTruthy();
    const hint = screen.getByText(/doesn't match the OpenAI key format/i);
    expect(hint.id).toBe(describedbyId);
  });

  it("local pane: both servers up → both pills connected, no auto-switch", async () => {
    // both probes succeed; resolve 2 models each so the chip's exact model count is meaningful.
    mocks.listExternalModels.mockResolvedValue(["llama3.2:3b", "qwen2.5:7b"]);
    renderCard({ groups: ["local"] });
    // Exact interpolated text — guards the modelCount (not count) i18next key.
    expect(await screen.findByText("Connected to Ollama — 2 models")).toBeInTheDocument();
    // Exact accessible name: the status-dot span must not fold into the pill's name.
    const ollamaPill = screen.getByRole("button", { name: "Ollama" });
    const lmStudioPill = screen.getByRole("button", { name: "LM Studio" });
    expect(ollamaPill).toBeInTheDocument();
    expect(lmStudioPill).toBeInTheDocument();
    // Selection is communicated to assistive tech via aria-pressed, not just color.
    expect(ollamaPill).toHaveAttribute("aria-pressed", "true");
    expect(lmStudioPill).toHaveAttribute("aria-pressed", "false");
  });

  it("local pane: both probes still pending → probing chip renders", async () => {
    mocks.listExternalModels.mockImplementation(() => new Promise(() => {}));
    renderCard({ groups: ["local"] });
    expect(await screen.findByText("Checking Ollama…")).toBeInTheDocument();
  });

  it("local pane: exactly one server up → auto-selects it", async () => {
    mocks.listExternalModels.mockImplementation((ep: string) =>
      ep.includes("1234")
        ? Promise.resolve(["qwen2.5:7b"])
        : Promise.reject(new Error("ECONNREFUSED")),
    );
    renderCard({ groups: ["local"] });
    // LM Studio (1234) is the sole responder → its chip is shown.
    expect(await screen.findByText(/Connected to LM Studio/)).toBeInTheDocument();
    expect(screen.getByLabelText("Endpoint URL")).toHaveValue("http://localhost:1234/v1");
  });

  it("local pane: no server up → not-detected chip for the selected pill", async () => {
    mocks.listExternalModels.mockRejectedValue(new Error("ECONNREFUSED"));
    renderCard({ groups: ["local"] });
    expect(
      await screen.findByText(/Not detected at localhost:11434 — is Ollama running\?/),
    ).toBeInTheDocument();
  });

  it("local pane: discovered models render as a <select>, not free text", async () => {
    mocks.listExternalModels.mockResolvedValue(["qwen2.5:7b", "llama3.2:3b"]);
    renderCard({ groups: ["local"] });
    await screen.findByText(/Connected to Ollama/);
    const modelField = await screen.findByLabelText("Model");
    expect(modelField.tagName).toBe("SELECT");
    await userEvent.selectOptions(modelField, "llama3.2:3b");
    expect(modelField).toHaveValue("llama3.2:3b");
  });

  it("local pane: discovery failure keeps free-text model entry with hint", async () => {
    mocks.listExternalModels.mockRejectedValue(new Error("ECONNREFUSED"));
    renderCard({ groups: ["local"] });
    await screen.findByText(/Not detected at localhost:11434/);
    const modelField = await screen.findByLabelText("Model");
    expect(modelField.tagName).toBe("INPUT");
    expect(screen.getByText(/type a model name/i)).toBeInTheDocument();
  });

  it("local pane: does not duplicate-fetch a probed preset's endpoint via the generic discovery query", async () => {
    // Default mock (["llama3.2:3b"]) covers both probes; a probed preset
    // (ollama, the default local selection) must not ALSO trigger the
    // generic `discovery` query against the same endpoint.
    renderCard({ groups: ["local"] });
    await screen.findByText(/Connected to Ollama/);
    expect(mocks.listExternalModels).toHaveBeenCalledTimes(2);
    expect(mocks.listExternalModels).toHaveBeenCalledWith("http://localhost:11434/v1", null);
    expect(mocks.listExternalModels).toHaveBeenCalledWith("http://localhost:1234/v1", null);
  });

  it("local pane: a hand-typed Custom endpoint still gets model discovery (no discovery hole for unprobed presets)", async () => {
    mocks.listExternalModels.mockResolvedValue(["custom-model-x"]);
    renderCard({ groups: ["local"] });
    await userEvent.click(await screen.findByRole("button", { name: "Custom…" }));
    await userEvent.type(screen.getByLabelText("Endpoint URL"), "http://localhost:9999/v1");
    await waitFor(() =>
      expect(mocks.listExternalModels).toHaveBeenCalledWith("http://localhost:9999/v1", null)
    );
    // Custom/unprobed presets have no dedicated probe, so they never get the
    // polished <select> — but discovery still runs and feeds the datalist,
    // so free-text entry is never left without autocomplete.
    const modelField = screen.getByLabelText("Model");
    expect(modelField.tagName).toBe("INPUT");
    expect(
      document.getElementById("any-provider-models")?.querySelector('option[value="custom-model-x"]'),
    ).toBeTruthy();
  });

  it("local pane: custom endpoint discovery failure keeps free text with the discovery-failed hint", async () => {
    mocks.listExternalModels.mockRejectedValue(new Error("ECONNREFUSED"));
    renderCard({ groups: ["local"] });
    await userEvent.click(await screen.findByRole("button", { name: "Custom…" }));
    await userEvent.type(screen.getByLabelText("Endpoint URL"), "http://localhost:9999/v1");
    expect(await screen.findByText(/type a model name/i)).toBeInTheDocument();
    expect(screen.getByLabelText("Model").tagName).toBe("INPUT");
  });

  it("local pane: editing the endpoint away from the probed Ollama preset drops the stale dropdown and discovers against the live endpoint", async () => {
    renderCard({ groups: ["local"] });
    // Ollama pill is the default local selection and is probed/connected.
    await screen.findByText(/Connected to Ollama/);
    mocks.listExternalModels.mockClear();

    await userEvent.clear(screen.getByLabelText("Endpoint URL"));
    await userEvent.type(screen.getByLabelText("Endpoint URL"), "http://192.168.1.5:11434/v1");

    // The edited endpoint no longer matches the Ollama preset's endpoint, so
    // the probe association must drop: no dropdown of the WRONG server's
    // models, and the generic discovery query must pick up the live value.
    expect(screen.getByLabelText("Model").tagName).toBe("INPUT");
    await waitFor(() =>
      expect(mocks.listExternalModels).toHaveBeenCalledWith("http://192.168.1.5:11434/v1", null)
    );
  });

  it("local pane: a saved model no longer discoverable stays visible and selectable, discovered ids still choosable", async () => {
    mocks.getExternalLlm.mockResolvedValue(["http://localhost:11434/v1", "llama3.2:1b"]);
    renderCard({ groups: ["local"] });
    await screen.findByText(/Connected to Ollama/);

    const modelField = await screen.findByLabelText("Model");
    expect(modelField.tagName).toBe("SELECT");
    // Saved model ("llama3.2:1b") isn't among the discovered ids
    // (["llama3.2:3b"], from the beforeEach default) — it must still render
    // as the selected value instead of a blank select.
    expect(modelField).toHaveValue("llama3.2:1b");
    // The discovered id must still be a choosable option.
    await userEvent.selectOptions(modelField, "llama3.2:3b");
    expect(modelField).toHaveValue("llama3.2:3b");
  });
});
