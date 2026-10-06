// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useId, useRef, useState } from "react";
import { BookOpenText } from "@phosphor-icons/react";
import { useTranslation } from "react-i18next";
import { dragStripHeight } from "../../lib/windowChrome";
import { Button } from "../memory/settings/primitives";
import { FirstUseSample } from "./FirstUseSample";
import "./notesWelcome.css";

export type NotesWelcomeDestination =
  | "sources"
  | "connect-agent"
  | "connections";

interface NotesWelcomeProps {
  onComplete: (destination?: NotesWelcomeDestination) => void | Promise<void>;
}

export function NotesWelcome({ onComplete }: NotesWelcomeProps) {
  const { t } = useTranslation();
  const titleId = useId();
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
            <p className="notes-welcome__body">{t("setup.notesFirst.body")}</p>
            <p className="notes-welcome__hint">{t("setup.notesFirst.hint")}</p>
            {failed && (
              <p className="notes-welcome__error" role="alert">
                {t("setup.notesFirst.openFailed")}
              </p>
            )}
            <Button
              type="button"
              variant="primary"
              className="notes-welcome__open"
              style={{ minHeight: 44, height: "auto" }}
              loading={pending}
              disabled={pending}
              aria-busy={pending}
              aria-label={t("setup.notesFirst.open")}
              onClick={() => void openNotes()}
            >
              {t("setup.notesFirst.open")}
            </Button>
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
          </div>
        )}
      </main>
    </div>
  );
}
