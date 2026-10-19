/**
 * Remotion CLI config for the demo (`pnpm demo:studio`, `pnpm demo:render`).
 * Static files (the logo) come from the repo's `public/`, Remotion's default.
 *
 * The film mounts the real app, so the bundle compiles its Tailwind
 * stylesheet and swaps its four Tauri modules for demo/stub, which answers
 * every command from the film's state. The film's base is
 * demo/fixture.json when a derivation from a real project wrote it, else the
 * hand-written demo/fixture.ts; both are the same shape (`Fixture`).
 *
 * REMOTION_BROWSER points the renderer at a Chrome/Chromium already on the
 * machine, for systems where Remotion's downloaded headless shell cannot run
 * (NixOS, for one; shell.nix sets it).
 */

import fs from "node:fs";
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

const demo = (...parts: string[]) => path.join(process.cwd(), "demo", ...parts);
const stub = (file: string) => demo("stub", file);
const derived = demo("fixture.json");
const base = fs.existsSync(derived) ? derived : demo("fixture.ts");

Config.overrideWebpackConfig((config) =>
  enableTailwind({
    ...config,
    resolve: {
      ...config.resolve,
      alias: {
        ...(config.resolve?.alias as Record<string, string> | undefined),
        "@film/base": base,
        "@tauri-apps/api/core": stub("core.ts"),
        "@tauri-apps/api/event": stub("event.ts"),
        "@tauri-apps/api/window": stub("window.ts"),
        "@tauri-apps/plugin-dialog": stub("dialog.ts"),
      },
    },
  }),
);
