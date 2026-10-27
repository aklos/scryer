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
// Each render tab mounts the whole app bundle; eight of them (the default,
// half the cores) ran a 27 GB machine out of memory. Three is plenty.
Config.setConcurrency(3);

const browser = process.env.REMOTION_BROWSER;
if (browser) {
  Config.setBrowserExecutable(browser);
}

const demo = (...parts: string[]) => path.join(process.cwd(), "demo", ...parts);
const stub = (file: string) => demo("stub", file);
const derived = demo("fixture.json");
const base = fs.existsSync(derived) ? derived : demo("fixture.ts");

/**
 * Tailwind's webpack loader registers its scan base as a context dependency,
 * and webpack then snapshots that whole tree. Left at the default (the repo
 * root) that includes target/, tens of gigabytes, and bundling alone ran the
 * machine out of memory. The app's classes all live under src/.
 */
function tailwindFromSrc<T extends { module?: { rules?: unknown[] } }>(config: T): T {
  const base = path.join(process.cwd(), "src");
  for (const rule of (config.module?.rules ?? []) as { use?: unknown }[]) {
    if (!rule || typeof rule !== "object" || !Array.isArray(rule.use)) continue;
    rule.use = rule.use.map((use: unknown) =>
      typeof use === "string" && use.includes("@tailwindcss") && use.includes("webpack") ? { loader: use, options: { base } } : use,
    );
  }
  return config;
}

Config.overrideWebpackConfig((config) =>
  tailwindFromSrc(enableTailwind({
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
  })),
);
