/**
 * The reel: its beats in order, each handed its own frame. Beats alternate
 * dark and light; the next fades in over the last frames of the one before.
 */

import { useEffect, useState, type ComponentType } from "react";
import { AbsoluteFill, continueRender, delayRender, useCurrentFrame } from "remotion";
import { p, s } from "./kit";
import { Agents, AGENTS } from "./beats/agents";
import { CatchUp, CATCHUP } from "./beats/catchup";
import { Checks, CHECKS } from "./beats/checks";
import { Close, CLOSE } from "./beats/close";
import { Plan, PLAN } from "./beats/plan";
import { Problem, PROBLEM } from "./beats/problem";
import { Structure, STRUCTURE } from "./beats/structure";
import { Title, TITLE } from "./beats/title";

export type Beat = { id: string; Body: ComponentType<{ f: number }>; frames: number };

export const BEATS: Beat[] = [
  { id: "problem", Body: Problem, frames: PROBLEM },
  { id: "title", Body: Title, frames: TITLE },
  { id: "catchup", Body: CatchUp, frames: CATCHUP },
  { id: "plan", Body: Plan, frames: PLAN },
  { id: "structure", Body: Structure, frames: STRUCTURE },
  { id: "checks", Body: Checks, frames: CHECKS },
  { id: "agents", Body: Agents, frames: AGENTS },
  { id: "close", Body: Close, frames: CLOSE },
];

/** Frames two neighbours share: the fade. */
const WIPE = s(0.5);

const STARTS = BEATS.reduce<number[]>((acc, _b, i) => [...acc, i === 0 ? 0 : acc[i - 1] + BEATS[i - 1].frames - WIPE], []);
export const REEL_FRAMES = STARTS[STARTS.length - 1] + BEATS[BEATS.length - 1].frames;

/** Holds the first frame until the reel's two faces have loaded. */
export function useFonts() {
  const [handle] = useState(() => delayRender("fonts"));
  useEffect(() => {
    Promise.all(["560 92px 'Geist Variable'", "500 16px 'Geist Mono Variable'"].map((font) => document.fonts.load(font))).then(() => continueRender(handle));
  }, [handle]);
}

export function Reel() {
  const frame = useCurrentFrame();
  useFonts();
  return (
    <AbsoluteFill>
      {BEATS.map(({ id, Body, frames }, i) => {
        const local = frame - STARTS[i];
        if (local < 0 || local >= frames) return null;
        const fade = i === 0 ? 1 : p(local, 0, WIPE, "move");
        return (
          <AbsoluteFill key={id} style={{ opacity: fade }}>
            <Body f={local} />
          </AbsoluteFill>
        );
      })}
    </AbsoluteFill>
  );
}

/** One beat on its own, for working on it. */
export function single(Body: ComponentType<{ f: number }>) {
  return function Single() {
    const frame = useCurrentFrame();
    useFonts();
    return (
      <AbsoluteFill>
        <Body f={frame} />
      </AbsoluteFill>
    );
  };
}
