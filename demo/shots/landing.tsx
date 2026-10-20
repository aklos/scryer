/**
 * The app's landing page ("Since you last looked"), mounted on its own. It
 * fetches through `get_landing`; the film answers that per window (`since`)
 * with sessions built from the base's real claims, so two beats can each
 * show their own week while both are on screen during a page turn.
 */

import { useEffect, useState } from "react";
import { continueRender, delayRender } from "remotion";
import { LandingPage } from "../../src/pages/landing/LandingPage";
import { answer } from "../stub/core";
import { NOW } from "../story";
import { claim, fixture } from "./app";

/** `get_landing`'s answer, as LandingPage reads it (its types are module-private). */
interface LandingClaim {
  respId: string;
  nodeId: string;
  statement: string;
  file: string;
  line: number;
  state: "proven" | "stale" | "failing" | "planned";
  concerns?: string[];
}
interface LandingSession {
  session: string;
  title: string;
  startedAt: number;
  endedAt: number;
  newlyProven: LandingClaim[];
  newlyPlanned: LandingClaim[];
  stillPlanned: LandingClaim[];
  newlyStaleOrFailing: LandingClaim[];
  survivingMutants: number;
  newDarkFiles: string[];
  components: string[];
}
interface Landing {
  since: number;
  sessions: LandingSession[];
  shared: { nodeId: string; sessions: string[] }[];
}

const byWindow = new Map<number, Landing>();
answer("get_landing", (args) => byWindow.get(Number(args.since)) ?? null);

/** A claim of the base as the landing page lists it. */
export function landingClaim(text: string, state: LandingClaim["state"]): LandingClaim {
  const c = claim(text);
  return { respId: c.id, nodeId: c.nodeId, statement: c.description, file: c.file, line: c.line, state, concerns: c.concerns };
}

/** A session block: its id, title, when, and what moved. */
export function session(s: Partial<LandingSession> & Pick<LandingSession, "session" | "title" | "startedAt" | "endedAt">): LandingSession {
  return {
    newlyProven: [],
    newlyPlanned: [],
    stillPlanned: [],
    newlyStaleOrFailing: [],
    survivingMutants: 0,
    newDarkFiles: [],
    components: [],
    ...s,
  };
}

export const ago = (minutes: number) => NOW - minutes * 60;

/** The landing page for the window since `since`, showing `landing`. */
export function LandingFigure({ since, landing, className }: { since: number; landing: Omit<Landing, "since">; className?: string }) {
  byWindow.set(since, { since, ...landing });
  const [handle] = useState(() => delayRender("landing page"));
  useEffect(() => {
    const id = setTimeout(() => continueRender(handle), 300);
    return () => clearTimeout(id);
  }, [handle]);
  return (
    <div className={`flex h-full bg-[var(--surface)] ${className ?? ""}`}>
      <LandingPage projectPath="/demo/aperture-pay" since={since} model={fixture.tree.planned} onSelectNode={() => {}} />
    </div>
  );
}
