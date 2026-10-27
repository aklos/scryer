/**
 * Reel v2: its beats in order, each handed its own frames, all on the dark
 * stage. The next beat fades in over the last frames of the one before.
 */

import type { ComponentType } from "react";
import { AbsoluteFill, useCurrentFrame } from "remotion";
import { p, s } from "../kit";
import { useFonts } from "../Reel";
import { Agents2, AGENTS2 } from "./beats/agents";
import { CatchUp2, CATCHUP2 } from "./beats/catchup";
import { Checks2, CHECKS2 } from "./beats/checks";
import { Close2, CLOSE2 } from "./beats/close";
import { Plan2, PLAN2 } from "./beats/plan";
import { Problem2, PROBLEM2 } from "./beats/problem";
import { Structure2, STRUCTURE2 } from "./beats/structure";
import { Title2, TITLE2 } from "./beats/title";
import { BG } from "./dressing";

export type Beat = { id: string; Body: ComponentType<{ f: number }>; frames: number };

export const BEATS2: Beat[] = [
  { id: "v2-problem", Body: Problem2, frames: PROBLEM2 },
  { id: "v2-title", Body: Title2, frames: TITLE2 },
  { id: "v2-catchup", Body: CatchUp2, frames: CATCHUP2 },
  { id: "v2-plan", Body: Plan2, frames: PLAN2 },
  { id: "v2-structure", Body: Structure2, frames: STRUCTURE2 },
  { id: "v2-checks", Body: Checks2, frames: CHECKS2 },
  { id: "v2-agents", Body: Agents2, frames: AGENTS2 },
  { id: "v2-close", Body: Close2, frames: CLOSE2 },
];

/** Frames two neighbours share: the fade. */
const WIPE = s(0.6);

const STARTS = BEATS2.reduce<number[]>((acc, _b, i) => [...acc, i === 0 ? 0 : acc[i - 1] + BEATS2[i - 1].frames - WIPE], []);
export const REEL2_FRAMES = STARTS[STARTS.length - 1] + BEATS2[BEATS2.length - 1].frames;

export function Reel2() {
  const frame = useCurrentFrame();
  useFonts();
  return (
    <AbsoluteFill style={{ background: BG }}>
      {BEATS2.map(({ id, Body, frames }, i) => {
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
