// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  CheckCircle,
  FolderSimplePlus,
  NotePencil,
  PlugsConnected,
  Sparkle,
  X,
} from "@phosphor-icons/react";
import { clipboardWrite } from "../../lib/tauri";
import { resolveAgentDisplayName } from "../../lib/agents";
import {
  connectedToolFamilies,
  GETTING_STARTED_ORDER,
  publishedPages,
  useGettingStartedState,
  type GettingStartedStepId,
} from "./useGettingStartedState";
import "./gettingStarted.css";

const COPIED_MS = 2_000;

async function copyText(text: string): Promise<boolean> {
  try {
    await clipboardWrite(text);
    return true;
  } catch {
    // Outside Tauri (browser preview) the plugin is missing; try the web API.
  }
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

export interface GettingStartedProps {
  readonly onWriteNote: () => void;
  readonly onConnectTool: () => void;
  readonly onAddSources: () => void;
  readonly onSeeExample: () => void;
}

export function GettingStarted({
  onWriteNote,
  onConnectTool,
  onAddSources,
  onSeeExample,
}: GettingStartedProps) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const { visible, progress, doneCount, dismissed, setDismissed, pages, agents } = useGettingStartedState();

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), COPIED_MS);
    return () => clearTimeout(timer);
  }, [copied]);

  if (!visible || !progress) return null;

  const stepsById: Record<GettingStartedStepId, { icon: React.ReactNode; onAction: () => void }> = {
    note: { icon: <NotePencil weight="regular" />, onAction: onWriteNote },
    tool: { icon: <PlugsConnected weight="regular" />, onAction: onConnectTool },
    sources: { icon: <FolderSimplePlus weight="regular" />, onAction: onAddSources },
    recall: { icon: <Sparkle weight="regular" />, onAction: onSeeExample },
  };
  const order = GETTING_STARTED_ORDER;

  if (dismissed) {
    return (
      <button
        type="button"
        className="getting-started-reopen"
        onClick={() => setDismissed(false)}
      >
        {t("firstUse.checklist.reopen")}
      </button>
    );
  }

  // The moment the whole thing is for happens outside Wenlan, in the AI tool.
  // Once a note is saved and a tool has called, hand the user the exact
  // question to paste there, about the note they just wrote.
  const published = publishedPages(pages);
  const promptTool = progress.note && progress.tool && !progress.recall
    ? resolveAgentDisplayName(connectedToolFamilies(agents)[0], agents)
    : null;
  const promptText = promptTool
    ? t("firstUse.checklist.steps.recall.prompt", { title: published[0].title })
    : null;
  const onlyDrafts = !progress.note && pages.length > 0;

  return (
    <section
      className="getting-started"
      data-testid="getting-started"
      aria-labelledby="getting-started-title"
    >
      <header className="getting-started-header">
        <h2 id="getting-started-title">{t("firstUse.checklist.title")}</h2>
        <button
          type="button"
          className="getting-started-hide"
          aria-label={t("firstUse.checklist.hide")}
          title={t("firstUse.checklist.hide")}
          onClick={() => setDismissed(true)}
        >
          <X aria-hidden="true" weight="regular" />
        </button>
      </header>
      <p className="getting-started-progress">
        {t("firstUse.checklist.progress", { done: doneCount, total: order.length })}
      </p>
      <ol className="getting-started-steps">
        {order.map((id) => {
          const step = stepsById[id];
          const done = progress[id];
          // Once there is a question to paste, the recall row's one button
          // copies it; the sample becomes a quiet link under the question.
          const asking = !done && id === "recall" && promptTool != null && promptText != null;
          const body = id === "note" && onlyDrafts
            ? t("firstUse.checklist.steps.note.draftBody")
            : asking
              ? t("firstUse.checklist.steps.recall.promptLead", { tool: promptTool })
              : t(`firstUse.checklist.steps.${id}.body`);
          return (
            <li
              key={id}
              data-testid={`getting-started-${id}`}
              data-done={done ? "true" : "false"}
            >
              <span className="getting-started-icon" aria-hidden="true">
                {done ? <CheckCircle weight="fill" /> : step.icon}
              </span>
              <div className="getting-started-copy">
                <p className="getting-started-step-title">
                  {t(`firstUse.checklist.steps.${id}.title`)}
                  {done ? (
                    <span className="sr-only">{` ${t("firstUse.checklist.done")}`}</span>
                  ) : null}
                </p>
                {done ? null : <p className="getting-started-step-body">{body}</p>}
                {asking ? (
                  <div className="getting-started-prompt" data-testid="getting-started-prompt">
                    <p className="getting-started-prompt-text">{promptText}</p>
                    <button
                      type="button"
                      className="getting-started-link"
                      onClick={step.onAction}
                    >
                      {t("firstUse.checklist.steps.recall.exampleLink")}
                    </button>
                  </div>
                ) : null}
              </div>
              {done ? null : asking ? (
                <button
                  type="button"
                  className="getting-started-action"
                  aria-live="polite"
                  onClick={() => {
                    void copyText(promptText).then((ok) => {
                      if (ok) setCopied(true);
                    });
                  }}
                >
                  {copied
                    ? t("firstUse.checklist.steps.recall.copied")
                    : t("firstUse.checklist.steps.recall.copy")}
                </button>
              ) : (
                <button
                  type="button"
                  className="getting-started-action"
                  onClick={step.onAction}
                >
                  {t(`firstUse.checklist.steps.${id}.action`)}
                </button>
              )}
            </li>
          );
        })}
      </ol>
    </section>
  );
}
