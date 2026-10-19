/**
 * The demo's Remotion root: one composition, `film`, 1920×1080 at 30 fps,
 * running as long as the script's event list.
 */

import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "@fontsource-variable/space-grotesk";
import "./fonts.css";
import { Composition } from "remotion";
import { FILM_FRAMES, FPS } from "./film/events";
import { Film } from "./film/Film";

export function Root() {
  return <Composition id="film" component={Film} durationInFrames={FILM_FRAMES} fps={FPS} width={1920} height={1080} />;
}
