// SPDX-License-Identifier: AGPL-3.0-only
import { useState } from "react";
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
  listOnboardingMilestones,
  listRegisteredSources,
  type AgentConnection,
  type MilestoneRecord,
  type Page,
  type RegisteredSource,
} from "../../lib/tauri";
import { seenFamiliesOf, useLiveAgents } from "../connect/connectionState";
import "./gettingStarted.css";

/** Per-viewer convenience only: losing it just shows the card again. */
const DISMISSED_KEY = "onboarding:gettingStartedDismissed";
/** A library with this many pages is past first use; existing users upgrading
 *  into this card would read it as noise. */
export const ESTABLISHED_LIBRARY_PAGES = 10;
/** Wenlan's own identities: the app, its CLI, and the setup probe. Their calls
 *  say nothing about whether the user has connected an AI tool. */
const OWN_IDENTITY = /^(wenlan|origin)(-|$)|^(manual|unknown)$/;

export type GettingStartedStepId = "note" | "tool" | "sources" | "recall";

export interface GettingStartedProgress {
  readonly note: boolean;
  readonly tool: boolean;
  readonly sources: boolean;
  readonly recall: boolean;
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
  const tools = [...seenFamiliesOf(agents)].filter((family) => !OWN_IDENTITY.test(family));
  return {
    note: pages.length > 0,
    tool: tools.length > 0,
    sources: sources.length > 0
      || pages.some((page) => page.creation_kind === "source" || page.creation_kind === "imported"),
    recall: milestones.some((milestone) => milestone.id === "first-recall")
      || pages.some((page) => page.creation_kind === "distilled"),
  };
}

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

export interface GettingStartedProps {
  /** The Wiki inventory (active pages and drafts, entity shadows excluded). */
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
  const steps: {
    id: GettingStartedStepId;
    icon: React.ReactNode;
    onAction: () => void;
  }[] = [
    { id: "note", icon: <NotePencil weight="regular" />, onAction: onWriteNote },
    { id: "tool", icon: <PlugsConnected weight="regular" />, onAction: onConnectTool },
    { id: "sources", icon: <FolderSimplePlus weight="regular" />, onAction: onAddSources },
    { id: "recall", icon: <Sparkle weight="regular" />, onAction: onSeeExample },
  ];
  const doneCount = steps.filter((step) => progress[step.id]).length;
  if (doneCount === steps.length) return null;

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
        {t("firstUse.checklist.progress", { done: doneCount, total: steps.length })}
      </p>
      <ol className="getting-started-steps">
        {steps.map((step) => {
          const done = progress[step.id];
          return (
            <li
              key={step.id}
              data-testid={`getting-started-${step.id}`}
              data-done={done ? "true" : "false"}
            >
              <span className="getting-started-icon" aria-hidden="true">
                {done ? <CheckCircle weight="fill" /> : step.icon}
              </span>
              <div className="getting-started-copy">
                <p className="getting-started-step-title">
                  {t(`firstUse.checklist.steps.${step.id}.title`)}
                  {done ? (
                    <span className="sr-only">{` ${t("firstUse.checklist.done")}`}</span>
                  ) : null}
                </p>
                {done ? null : (
                  <p className="getting-started-step-body">
                    {t(`firstUse.checklist.steps.${step.id}.body`)}
                  </p>
                )}
              </div>
              {done ? null : (
                <button
                  type="button"
                  className="getting-started-action"
                  onClick={step.onAction}
                >
                  {t(`firstUse.checklist.steps.${step.id}.action`)}
                </button>
              )}
            </li>
          );
        })}
      </ol>
    </section>
  );
}
