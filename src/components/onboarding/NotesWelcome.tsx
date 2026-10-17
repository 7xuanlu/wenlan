// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useId, useRef, useState } from "react";
import {
  BookOpenText,
  FolderSimplePlus,
  NotePencil,
  PlugsConnected,
} from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import { dragStripHeight } from "../../lib/windowChrome";
import { Button } from "../memory/settings/primitives";
import { FirstUseSample } from "./FirstUseSample";
import { writeStartPath, type StartPath } from "./startPath";
import "./notesWelcome.css";

export type NotesWelcomeDestination =
  | "sources"
  | "connect-agent"
  | "connections"
  | "page-draft";

interface NotesWelcomeProps {
  onComplete: (destination?: NotesWelcomeDestination) => void | Promise<void>;
}

/** The three ways in, in the order they are offered. Each remembers itself as
 *  the user's start path so the Wiki's checklist leads with that step. */
const START_PATHS: readonly {
  id: StartPath;
  destination: NotesWelcomeDestination;
  icon: React.ReactNode;
}[] = [
  { id: "tool", destination: "connect-agent", icon: <PlugsConnected weight="regular" /> },
  { id: "sources", destination: "sources", icon: <FolderSimplePlus weight="regular" /> },
  { id: "note", destination: "page-draft", icon: <NotePencil weight="regular" /> },
];

export function NotesWelcome({ onComplete }: NotesWelcomeProps) {
  const { t } = useTranslation();
  const titleId = useId();
  const pathIdBase = useId();
  const inFlight = useRef(false);
  const [pending, setPending] = useState(false);
  const [failed, setFailed] = useState(false);
  const [sampleOpen, setSampleOpen] = useState(false);
  const sampleTrigger = useRef<HTMLButtonElement | null>(null);
  const sampleHost = useRef<HTMLDivElement | null>(null);
  const restoreSampleFocus = useRef(false);

  useEffect(() => {
    if (sampleOpen) {
      sampleHost.current?.focus();
    } else if (restoreSampleFocus.current) {
      restoreSampleFocus.current = false;
      sampleTrigger.current?.focus();
    }
  }, [sampleOpen]);

  async function openNotes(destination?: NotesWelcomeDestination) {
    // State updates may be batched, so guard before invoking the callback.
    if (inFlight.current) return;
    inFlight.current = true;
    setPending(true);
    setFailed(false);
    try {
      if (destination === undefined) await onComplete();
      else await onComplete(destination);
    } catch {
      setFailed(true);
    } finally {
      inFlight.current = false;
      setPending(false);
    }
  }

  function backToWelcome() {
    if (inFlight.current) return;
    restoreSampleFocus.current = true;
    setFailed(false);
    setSampleOpen(false);
  }

  return (
    <div className="notes-welcome" data-testid="notes-welcome">
      <div
        data-tauri-drag-region
        style={{ height: dragStripHeight(), flexShrink: 0 }}
      />
      <main
        className={sampleOpen
          ? "notes-welcome__main notes-welcome__main--sample"
          : "notes-welcome__main"}
        aria-labelledby={sampleOpen ? undefined : titleId}
        aria-label={sampleOpen ? t("firstUse.sample.animationLabel") : undefined}
      >
        {sampleOpen ? (
          <div className="notes-welcome__sample">
            {failed && (
              <p className="notes-welcome__error" role="alert">
                {t("setup.notesFirst.openFailed")}
              </p>
            )}
            <div
              ref={sampleHost}
              tabIndex={-1}
              inert={pending ? true : undefined}
              aria-busy={pending}
              onClickCapture={(event) => {
                if (inFlight.current) {
                  event.preventDefault();
                  event.stopPropagation();
                }
              }}
            >
              <FirstUseSample
                backLabel={t("setup.notesFirst.backToWelcome")}
                onBackToGuide={backToWelcome}
                onBringData={() => void openNotes("sources")}
                onConnect={(client) => void openNotes(
                  client === "chatgpt" ? "connections" : "connect-agent",
                )}
                headerAction={(
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="notes-welcome__sample-open"
                    loading={pending}
                    disabled={pending}
                    aria-busy={pending}
                    aria-label={t("setup.notesFirst.open")}
                    onClick={() => void openNotes()}
                  >
                    {t("setup.notesFirst.open")}
                  </Button>
                )}
              />
            </div>
          </div>
        ) : (
          <div className="notes-welcome__content">
            <BookOpenText
              className="notes-welcome__icon"
              size={32}
              weight="regular"
              aria-hidden="true"
            />
            <h1 id={titleId} className="notes-welcome__title">
              {t("setup.welcomeTitle")}
            </h1>
            <p className="notes-welcome__body">{t("setup.notesFirst.question")}</p>
            {failed && (
              <p className="notes-welcome__error" role="alert">
                {t("setup.notesFirst.openFailed")}
              </p>
            )}
            <div className="notes-welcome__paths" data-testid="notes-welcome-paths">
              {START_PATHS.map((path) => (
                <button
                  key={path.id}
                  type="button"
                  className="notes-welcome__path"
                  data-testid={`notes-welcome-path-${path.id}`}
                  aria-labelledby={`${pathIdBase}-${path.id}-title`}
                  aria-describedby={`${pathIdBase}-${path.id}-body`}
                  aria-busy={pending}
                  disabled={pending}
                  onClick={() => {
                    if (inFlight.current) return;
                    writeStartPath(path.id);
                    void openNotes(path.destination);
                  }}
                >
                  <span className="notes-welcome__path-icon" aria-hidden="true">
                    {path.icon}
                  </span>
                  <span className="notes-welcome__path-copy">
                    <span
                      id={`${pathIdBase}-${path.id}-title`}
                      className="notes-welcome__path-title"
                    >
                      {t(`setup.notesFirst.paths.${path.id}.title`)}
                    </span>
                    <span
                      id={`${pathIdBase}-${path.id}-body`}
                      className="notes-welcome__path-body"
                    >
                      {t(`setup.notesFirst.paths.${path.id}.body`)}
                    </span>
                  </span>
                </button>
              ))}
            </div>
            <p className="notes-welcome__hint">{t("setup.notesFirst.pathsHint")}</p>
            <div className="notes-welcome__secondary">
              <button
                ref={sampleTrigger}
                type="button"
                className="notes-welcome__try"
                disabled={pending}
                onClick={() => {
                  if (inFlight.current) return;
                  setFailed(false);
                  setSampleOpen(true);
                }}
              >
                {t("firstUse.guide.tryExample")}
              </button>
              <button
                type="button"
                className="notes-welcome__try"
                disabled={pending}
                aria-busy={pending}
                onClick={() => void openNotes()}
              >
                {t("setup.notesFirst.lookAround")}
              </button>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
