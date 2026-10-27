/**
 * The reel's Remotion root, apart from the old film's: the reel, and each
 * beat on its own for working on one.
 */

import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import { Composition } from "remotion";
import { FPS } from "./kit";
import { BEATS, Reel, REEL_FRAMES, single } from "./Reel";
import { BEATS2, Reel2, REEL2_FRAMES } from "./v2/Reel2";

export function Root() {
  return (
    <>
      <Composition id="reel" component={Reel} durationInFrames={REEL_FRAMES} fps={FPS} width={1920} height={1080} />
      <Composition id="reel2" component={Reel2} durationInFrames={REEL2_FRAMES} fps={FPS} width={1920} height={1080} />
      {[...BEATS, ...BEATS2].map((b) => (
        <Composition key={b.id} id={b.id} component={single(b.Body)} durationInFrames={b.frames} fps={FPS} width={1920} height={1080} />
      ))}
    </>
  );
}
