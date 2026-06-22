/**
 * Video render driver for the trailer. Records a film-engine Stage scene to webm
 * (Playwright), then transcodes to a GitHub-friendly H.264 mp4 under a size cap
 * (two-pass, bitrate solved from the measured duration).
 *
 *   node demo/render.mjs <scene> [maxMB]
 *
 * Needs the demo dev server up (http://localhost:5199) and ffmpeg/ffprobe on PATH.
 * The scene must set `window.__filmDone = true` when its run() resolves (Stage
 * does this) so we stop at the real end instead of a fixed timeout.
 */
import { chromium } from "playwright";
import { execFileSync } from "node:child_process";
import { mkdir, readdir, rm, stat } from "node:fs/promises";
import { join } from "node:path";

const scene = process.argv[2] ?? "refund";
const maxMB = Number(process.argv[3] ?? 10);
const BASE = process.env.DEMO_URL ?? "http://localhost:5199/demo/index.html";
const OUT = process.env.OUT_DIR ?? "/tmp/scryer-render";
const W = 1600;
const H = 1000;
const exe = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || undefined;

await mkdir(OUT, { recursive: true });
// Clear stale recordings (so we pick the right webm) but keep prior mp4s — a
// failed run shouldn't destroy a good render.
for (const f of await readdir(OUT)) {
  if (f.endsWith(".webm")) await rm(join(OUT, f), { force: true });
}

const browser = await chromium.launch({ executablePath: exe });
const context = await browser.newContext({
  viewport: { width: W, height: H },
  recordVideo: { dir: OUT, size: { width: W, height: H } },
});
const page = await context.newPage();
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));

console.log(`● recording #${scene} …`);
await page.goto(`${BASE}#${scene}`, { waitUntil: "load" });
await page.waitForFunction(() => window.__filmDone === true, undefined, { timeout: 300000 });
await page.waitForTimeout(900); // hold the final frame a beat
await context.close(); // finalizes the .webm
await browser.close();
if (errors.length) console.log("  (page errors)\n  " + errors.join("\n  "));

const webm = `${OUT}/${(await readdir(OUT)).find((f) => f.endsWith(".webm"))}`;
const mp4 = `${OUT}/${scene}.mp4`;

// Drop the leading blank before React's first paint, so the opening frame (the
// GitHub thumbnail) is the establishing shot, not a flat fill.
const TRIM = 0.45;

const rawDur = parseFloat(
  execFileSync("ffprobe", [
    "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", webm,
  ]).toString().trim(),
);
const dur = rawDur - TRIM;

// Solve a video bitrate that lands the whole file under the cap (no audio),
// leaving ~7% headroom for muxing overhead.
const bitrate = Math.floor((maxMB * 1024 * 1024 * 8 * 0.93) / dur);
console.log(`● ${dur.toFixed(1)}s → ${(bitrate / 1000) | 0} kbps (cap ${maxMB} MB)`);

const enc = (pass, extra) =>
  execFileSync(
    "ffmpeg",
    [
      "-y", "-ss", `${TRIM}`, "-i", webm,
      "-c:v", "libx264", "-pix_fmt", "yuv420p", "-preset", "slow",
      "-b:v", `${bitrate}`, "-maxrate", `${Math.floor(bitrate * 1.4)}`,
      "-bufsize", `${Math.floor(bitrate * 2)}`,
      "-an", "-pass", `${pass}`, ...extra,
    ],
    { cwd: OUT, stdio: "inherit" },
  );

enc(1, ["-f", "mp4", "/dev/null"]);
enc(2, ["-movflags", "+faststart", mp4]);

const mb = (await stat(mp4)).size / 1024 / 1024;
console.log(`✔ ${mp4}  (${mb.toFixed(2)} MB, ${dur.toFixed(1)}s)`);
if (mb > maxMB) console.log(`⚠ over cap by ${(mb - maxMB).toFixed(2)} MB`);
