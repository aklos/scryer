/**
 * Remotion CLI config for the demo (`pnpm demo:studio`, `pnpm demo:render`).
 * Static files (the logo) come from the repo's `public/`, Remotion's default.
 *
 * The film mounts the real app, so the bundle compiles its Tailwind
 * stylesheet and swaps its four Tauri modules for demo/stub, which answers
 * every command from the film's state.
 *
 * REMOTION_BROWSER points the renderer at a Chrome/Chromium already on the
 * machine, for systems where Remotion's downloaded headless shell cannot run
 * (NixOS, for one; shell.nix sets it).
 */

import path from "node:path";
import { Config } from "@remotion/cli/config";
import { enableTailwind } from "@remotion/tailwind-v4";

Config.setVideoImageFormat("jpeg");
Config.setOverwriteOutput(true);
Config.setConcurrency(null);

const browser = process.env.REMOTION_BROWSER;
if (browser) {
  Config.setBrowserExecutable(browser);
}

const stub = (file: string) => path.join(process.cwd(), "demo", "stub", file);

Config.overrideWebpackConfig((config) =>
  enableTailwind({
    ...config,
    resolve: {
      ...config.resolve,
      alias: {
        ...(config.resolve?.alias as Record<string, string> | undefined),
        "@tauri-apps/api/core": stub("core.ts"),
        "@tauri-apps/api/event": stub("event.ts"),
        "@tauri-apps/api/window": stub("window.ts"),
        "@tauri-apps/plugin-dialog": stub("dialog.ts"),
      },
    },
  }),
);
