/**
 * Screenshot / capture driver for the trailer harness. Walks every scene
 * (`--scene <id>` for one, otherwise all) and grabs a PNG per scene so we can
 * eyeball the lifted components on fixtures.
 */
import { chromium } from "playwright";

const BASE = process.env.DEMO_URL ?? "http://localhost:5199/demo/index.html";
const OUT_DIR = process.env.OUT_DIR ?? "/tmp/scryer-demo";
const SCENES = process.argv.includes("--scene")
  ? [process.argv[process.argv.indexOf("--scene") + 1]]
  : ["cold", "node", "powerline", "diagram", "drift", "close"];

// On NixOS we drive the Nix-built chromium (shell.nix sets the path); Playwright's
// own download is skipped. Elsewhere, fall back to Playwright's bundled browser.
const executablePath = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || undefined;
const browser = await chromium.launch({ executablePath });
const page = await browser.newPage({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 2 });

const errors = [];
page.on("pageerror", (e) => errors.push("pageerror: " + String(e)));
page.on("response", (r) => { if (r.status() >= 400) errors.push(`http ${r.status()}: ${r.url()}`); });
page.on("requestfailed", (r) => errors.push(`reqfailed: ${r.url()} — ${r.failure()?.errorText}`));

import { mkdir } from "node:fs/promises";
await mkdir(OUT_DIR, { recursive: true });

let failed = false;
for (const scene of SCENES) {
  errors.length = 0;
  await page.goto(`${BASE}#${scene}`, { waitUntil: "networkidle" });
  await page.waitForTimeout(2200); // let async scene build + layout settle
  const out = `${OUT_DIR}/${scene}.png`;
  await page.screenshot({ path: out });
  if (errors.length) { failed = true; console.log(`✘ ${scene}\n  ` + errors.join("\n  ")); }
  else console.log(`✔ ${scene} → ${out}`);
}

await browser.close();
process.exit(failed ? 1 : 0);
