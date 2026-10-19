/**
 * Remotion CLI config for the demo (`pnpm demo:studio`, `pnpm demo:render`).
 * Static files (the logo) come from the repo's `public/`, Remotion's default.
 *
 * REMOTION_BROWSER points the renderer at a Chrome/Chromium already on the
 * machine, for systems where Remotion's downloaded headless shell cannot run
 * (NixOS, for one).
 */

import { Config } from "@remotion/cli/config";

Config.setVideoImageFormat("jpeg");
Config.setOverwriteOutput(true);
Config.setConcurrency(null);

const browser = process.env.REMOTION_BROWSER;
if (browser) {
  Config.setBrowserExecutable(browser);
}
