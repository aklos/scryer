/**
 * Remotion CLI config for the reel (`pnpm demo:reel`). The reel mounts the
 * real app, so the bundle compiles its Tailwind stylesheet and swaps its
 * Tauri modules for demo/reel/live, which answers every command from the
 * reel's state. The base is demo/fixture.json (`pnpm demo:derive`) when it
 * exists, else demo/fixture.ts.
 *
 * Two tabs: each mounts the whole app, and this machine has run out of
 * memory rendering beside builds. demo/reel/render.sh also caps the render's
 * memory. REMOTION_BROWSER points at a system Chromium where Remotion's own
 * cannot run (NixOS; shell.nix sets it).
 */

import fs from "node:fs";
import path from "node:path";
import { Config } from "@remotion/cli/config";
import { enableTailwind } from "@remotion/tailwind-v4";

Config.setVideoImageFormat("jpeg");
Config.setOverwriteOutput(true);
Config.setConcurrency(2);

const browser = process.env.REMOTION_BROWSER;
if (browser) {
  Config.setBrowserExecutable(browser);
}

const demo = (...parts: string[]) => path.join(process.cwd(), "demo", ...parts);
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
        "@tauri-apps/api/core": demo("reel", "live", "core.ts"),
        "@tauri-apps/api/event": demo("reel", "live", "event.ts"),
        "@tauri-apps/api/window": demo("stub", "window.ts"),
        "@tauri-apps/plugin-dialog": demo("stub", "dialog.ts"),
      },
    },
  })),
);
