/**
 * The demo's Remotion root: one composition per shot of the film
 * (plans/demo-shotlist.md), 1920×1080 at 30 fps. Each mounts the one real
 * component it needs with its state as props. Only stills so far: each
 * renders its shot's mid-point.
 */

import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "./fonts.css";
import { Composition } from "remotion";
import { Shot2 } from "./shots/Shot2";
import { Shot5 } from "./shots/Shot5";
import { Shot6 } from "./shots/Shot6";

const FPS = 30;

export function Root() {
  return (
    <>
      <Composition id="shot-2" component={Shot2} durationInFrames={5 * FPS} fps={FPS} width={1920} height={1080} defaultProps={{ lift: 1 }} />
      <Composition id="shot-5" component={Shot5} durationInFrames={6 * FPS} fps={FPS} width={1920} height={1080} defaultProps={{ reported: 2 }} />
      <Composition id="shot-6" component={Shot6} durationInFrames={6 * FPS} fps={FPS} width={1920} height={1080} defaultProps={{ misplaced: true }} />
    </>
  );
}
