// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import {
  CheckCircle,
  FolderSimplePlus,
  NotePencil,
  PlugsConnected,
  Sparkle,
  X,
} from "@phosphor-icons/react";
import {
  clipboardWrite,
  listOnboardingMilestones,
  listRegisteredSources,
  type AgentConnection,
  type MilestoneRecord,
  type Page,
  type RegisteredSource,
} from "../../lib/tauri";
import { OWN_IDENTITY, resolveAgentDisplayName, toolFamilyOf } from "../../lib/agents";
import { useLiveAgents } from "../connect/connectionState";
import "./gettingStarted.css";

/** Per-viewer convenience only: losing it just shows the card again. */
const DISMISSED_KEY = "onboarding:gettingStartedDismissed";
/** A library with this many pages is past first use; existing users upgrading
 *  into this card would read it as noise. */
export const ESTABLISHED_LIBRARY_PAGES = 10;
const COPIED_MS = 2_000;

export type GettingStartedStepId = "note" | "tool" | "sources" | "recall";

export interface GettingStartedProgress {
  readonly note: boolean;
  readonly tool: boolean;
  readonly sources: boolean;
  readonly recall: boolean;
}

/** A draft lives only in the editor until the user leaves it; recall cannot
 *  see it, so it does not count as a saved note. */
export function publishedPages(pages: readonly Page[]): Page[] {
  return pages.filter((page) => page.status !== "draft");
}

/** The AI tools the daemon knows, Wenlan's own identities excluded.
 *  Connecting a tool from the app registers it (`POST /api/agents`), so a row
 *  is the user's own act of connecting; the step does not also wait for the
 *  tool's first call, which happens outside the app and reads as "I did it,
 *  why isn't it done". */
export function connectedToolFamilies(agents: readonly AgentConnection[]): string[] {
  const families = new Set<string>();
  for (const agent of agents) {
    const family = toolFamilyOf(agent);
    if (!OWN_IDENTITY.test(family)) families.add(family);
  }
  return [...families];
}

/** Each step is read from what the library holds now, not from a stored
 *  checkmark, so it stays true to what the user actually did. */
export function gettingStartedProgress({
  pages,
  agents,
  sources,
  milestones,
}: {
  pages: readonly Page[];
  agents: readonly AgentConnection[];
  sources: readonly RegisteredSource[];
  milestones: readonly MilestoneRecord[];
}): GettingStartedProgress {
  return {
    note: publishedPages(pages).length > 0,
    tool: connectedToolFamilies(agents).length > 0,
    sources: sources.length > 0
      || pages.some((page) => page.creation_kind === "source" || page.creation_kind === "imported"),
    recall: milestones.some((milestone) => milestone.id === "first-recall")
      || pages.some((page) => page.creation_kind === "distilled"),
  };
}

const DEFAULT_ORDER: readonly GettingStartedStepId[] = ["note", "tool", "sources", "recall"];

function readDismissed(): boolean {
  try {
    return localStorage.getItem(DISMISSED_KEY) === "1";
  } catch {
    return false;
  }
}

function writeDismissed(value: boolean) {
  try {
    if (value) localStorage.setItem(DISMISSED_KEY, "1");
    else localStorage.removeItem(DISMISSED_KEY);
  } catch {
    // Storage can be unavailable; the card still hides for this session.
  }
}

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
  /** The Wiki inventory (active pages and drafts, entity shadows excluded),
   *  most recently modified first. */
  readonly pages: readonly Page[];
  readonly pagesReady: boolean;
  readonly onWriteNote: () => void;
  readonly onConnectTool: () => void;
  readonly onAddSources: () => void;
  readonly onSeeExample: () => void;
}

export function GettingStarted({
  pages,
  pagesReady,
  onWriteNote,
  onConnectTool,
  onAddSources,
  onSeeExample,
}: GettingStartedProps) {
  const { t } = useTranslation();
  const [dismissed, setDismissed] = useState(readDismissed);
  const [copied, setCopied] = useState(false);
  const agentsQuery = useLiveAgents();
  const sourcesQuery = useQuery({
    queryKey: ["registeredSources"],
    queryFn: listRegisteredSources,
    refetchOnWindowFocus: true,
  });
  // Same key as useMilestones, read without its event listener: the card only
  // needs whether a recall has happened, and the shared cache keeps it fresh.
  const milestonesQuery = useQuery({
    queryKey: ["onboarding-milestones"],
    queryFn: listOnboardingMilestones,
    staleTime: 30_000,
  });

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), COPIED_MS);
    return () => clearTimeout(timer);
  }, [copied]);

  // A failed read would claim a step is undone when it may not be, so the
  // card stays away rather than guess.
  if (!pagesReady || !agentsQuery.isSuccess || !sourcesQuery.isSuccess || !milestonesQuery.isSuccess) {
    return null;
  }
  if (pages.length >= ESTABLISHED_LIBRARY_PAGES) return null;

  const progress = gettingStartedProgress({
    pages,
    agents: agentsQuery.data,
    sources: sourcesQuery.data,
    milestones: milestonesQuery.data,
  });
  const stepsById: Record<GettingStartedStepId, { icon: React.ReactNode; onAction: () => void }> = {
    note: { icon: <NotePencil weight="regular" />, onAction: onWriteNote },
    tool: { icon: <PlugsConnected weight="regular" />, onAction: onConnectTool },
    sources: { icon: <FolderSimplePlus weight="regular" />, onAction: onAddSources },
    recall: { icon: <Sparkle weight="regular" />, onAction: onSeeExample },
  };
  const order = DEFAULT_ORDER;
  const doneCount = order.filter((id) => progress[id]).length;
  if (doneCount === order.length) return null;

  if (dismissed) {
    return (
      <button
        type="button"
        className="getting-started-reopen"
        onClick={() => {
          writeDismissed(false);
          setDismissed(false);
        }}
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
    ? resolveAgentDisplayName(connectedToolFamilies(agentsQuery.data)[0], agentsQuery.data)
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
        <div>
          <h2 id="getting-started-title">{t("firstUse.checklist.title")}</h2>
          <p className="getting-started-lede">{t("firstUse.checklist.lede")}</p>
        </div>
        <button
          type="button"
          className="getting-started-hide"
          aria-label={t("firstUse.checklist.hide")}
          title={t("firstUse.checklist.hide")}
          onClick={() => {
            writeDismissed(true);
            setDismissed(true);
          }}
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
          const body = id === "note" && onlyDrafts
            ? t("firstUse.checklist.steps.note.draftBody")
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
                {!done && id === "recall" && promptTool && promptText ? (
                  <div className="getting-started-prompt" data-testid="getting-started-prompt">
                    <p className="getting-started-prompt-lead">
                      {t("firstUse.checklist.steps.recall.promptLead", { tool: promptTool })}
                    </p>
                    <div className="getting-started-prompt-row">
                      <code className="getting-started-prompt-text">{promptText}</code>
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
                    </div>
                  </div>
                ) : null}
              </div>
              {done ? null : (
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
