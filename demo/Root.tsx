/**
 * The demo's Remotion root: the film, 1920×1080 at 30 fps, as long as its
 * beats (plans/demo-brief.md).
 */

import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "@fontsource-variable/space-grotesk";
import "./fonts.css";
import { Composition } from "remotion";
import { Film, FILM_FRAMES } from "./film/Film";
import { FPS } from "./film/motion";

export function Root() {
  return <Composition id="film" component={Film} durationInFrames={FILM_FRAMES} fps={FPS} width={1920} height={1080} />;
}
