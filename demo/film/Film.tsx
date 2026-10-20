/**
 * The film: eleven beats (plans/demo-brief.md), each a page set on the same
 * grid, played in order. Between two beats the page turns — the outgoing
 * page slides off to the left as the next slides in from the right, both
 * whole, running heads and all — and the incoming page's content settles
 * once it is in place. Everything is a function of the frame.
 */

import type { ComponentType } from "react";
import { AbsoluteFill, Easing, interpolate, useCurrentFrame } from "remotion";
import { Agents, AGENTS, Back, BACK, Concern, CONCERN, Probe, PROBE, Since, SINCE } from "./closing";
import { Model, MODEL, Style, STYLE, Toml, TOML } from "./middle";
import { Claim, CLAIM, Cover, COVER, Plan, PLAN, Proven, PROVEN } from "./opening";

type Beat = { Page: ComponentType<{ f: number }>; frames: number };

export const BEATS: Beat[] = [
  { Page: Cover, frames: COVER },
  { Page: Claim, frames: CLAIM },
  { Page: Plan, frames: PLAN },
  { Page: Proven, frames: PROVEN },
  { Page: Model, frames: MODEL },
  { Page: Toml, frames: TOML },
  { Page: Style, frames: STYLE },
  { Page: Concern, frames: CONCERN },
  { Page: Probe, frames: PROBE },
  { Page: Agents, frames: AGENTS },
  { Page: Since, frames: SINCE },
  { Page: Back, frames: BACK },
];

/** The page turn, in frames, centred on each boundary. */
const TURN = 16;
const turnEase = Easing.bezier(0.65, 0, 0.35, 1);

const STARTS = BEATS.reduce<number[]>((acc, _beat, i) => [...acc, i === 0 ? 0 : acc[i - 1] + BEATS[i - 1].frames], []);
export const FILM_FRAMES = STARTS[STARTS.length - 1] + BEATS[BEATS.length - 1].frames;

export function Film() {
  const frame = useCurrentFrame();
  let i = STARTS.findIndex((start, k) => frame >= start && frame < start + BEATS[k].frames);
  if (i < 0) i = BEATS.length - 1;
  const local = frame - STARTS[i];
  // Near a boundary, both pages are on the spread.
  const next = i + 1 < BEATS.length && local >= BEATS[i].frames - TURN / 2 ? i + 1 : null;
  const prev = i > 0 && local < TURN / 2 ? i - 1 : null;
  const pair = next !== null ? [i, next] : prev !== null ? [prev, i] : null;
  if (!pair) {
    const { Page } = BEATS[i];
    return <Page f={local} />;
  }
  const [a, b] = pair as [number, number];
  const boundary = STARTS[b];
  const p = turnEase(interpolate(frame, [boundary - TURN / 2, boundary + TURN / 2], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }));
  const A = BEATS[a].Page;
  const B = BEATS[b].Page;
  return (
    <AbsoluteFill style={{ background: "#0C0D10" }}>
      <AbsoluteFill style={{ transform: `translateX(${-p * 100}%)` }}>
        <A f={frame - STARTS[a]} />
      </AbsoluteFill>
      <AbsoluteFill style={{ transform: `translateX(${(1 - p) * 100}%)` }}>
        <B f={frame - STARTS[b]} />
      </AbsoluteFill>
    </AbsoluteFill>
  );
}
