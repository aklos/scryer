/**
 * Video render driver for the trailer. Captures a film-engine Stage scene as
 * lossless-grade frames via the CDP screencast (NOT Playwright's recordVideo —
 * that emits ~1 Mbps VP8 at a fixed, non-configurable quality, which destroys the
 * picture before any transcode can touch it), assembles them into a near-lossless
 * intermediate, then transcodes to a GitHub-friendly H.264 mp4 under a size cap
 * (two-pass, bitrate solved from the measured duration). H.264 plays in every
 * browser/Safari with no AV1 decode risk; GitHub Pro's 100 MB video limit gives
 * plenty of bitrate for crisp text. Pass the cap as MB:  node demo/render.mjs film 60
 *
 *   node demo/render.mjs <scene> [maxMB]
 *
 * Needs the demo dev server up (http://localhost:5199) and ffmpeg/ffprobe on PATH.
 * The scene must set `window.__filmDone = true` when its run() resolves (Stage
 * does this) so we stop at the real end instead of a fixed timeout.
 */
import { chromium } from "playwright";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdir, stat, writeFile } from "node:fs/promises";
import { mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const scene = process.argv[2] ?? "refund";
const maxMB = Number(process.argv[3] ?? 10);
const BASE = process.env.DEMO_URL ?? "http://localhost:5199/demo/index.html";
const OUT = process.env.OUT_DIR ?? "/tmp/scryer-render";
// Full 1080p, 16:9 — the standard embed shape. With GitHub Pro's 100 MB cap the
// bitrate is no longer the constraint, so we render at native resolution for the
// sharpest text rather than shrinking to save bytes.
const W = 1920;
const H = 1080;
const exe = process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE || undefined;

await mkdir(OUT, { recursive: true });

const browser = await chromium.launch({ executablePath: exe });

// Warm the dev server first (outside the recording): a cold Vite has to compile
// the whole module graph on the first request, which delays the recorded page's
// first paint by seconds and leaves a long, jittery startup flash. Compiling it
// here makes the recorded load paint fast and consistently.
const warm = await browser.newPage();
await warm.goto(`${BASE}#${scene}`, { waitUntil: "load" }).catch(() => {});
await warm.waitForSelector(".film-frame", { timeout: 60000 }).catch(() => {});
await warm.waitForTimeout(800);
await warm.close();

// deviceScaleFactor: 2 renders the page at 2× device pixels (UI rasterised at
// retina DPI); the screencast below caps frames at W×H, so each is downsampled
// 2×→1× — supersampled, crisp text and edges.
const context = await browser.newContext({
  viewport: { width: W, height: H },
  deviceScaleFactor: 2,
});
const page = await context.newPage();
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));

// Capture via the CDP screencast: the compositor's own frames at JPEG q100
// (visually lossless), each tagged with a swap timestamp so we can rebuild exact
// timing. This sidesteps recordVideo's ~1 Mbps VP8 entirely.
const framesDir = `${OUT}/frames`;
rmSync(framesDir, { recursive: true, force: true });
mkdirSync(framesDir, { recursive: true });
const client = await context.newCDPSession(page);
const frames = []; // { file, t } in capture order
const writes = [];
let n = 0;
client.on("Page.screencastFrame", (p) => {
  // Ack at once so Chrome keeps streaming; persist the JPEG off the critical path.
  client.send("Page.screencastFrameAck", { sessionId: p.sessionId }).catch(() => {});
  const file = `${framesDir}/f_${String(++n).padStart(6, "0")}.jpg`;
  frames.push({ file, t: p.metadata.timestamp });
  writes.push(writeFile(file, Buffer.from(p.data, "base64")));
});

console.log(`● recording #${scene} …`);
await page.goto(`${BASE}#${scene}`, { waitUntil: "load" });
await client.send("Page.startScreencast", {
  format: "jpeg",
  quality: 100,
  maxWidth: W,
  maxHeight: H,
  everyNthFrame: 1,
});
const tStart = Date.now();
await page.waitForFunction(() => window.__filmDone === true, undefined, { timeout: 300000 });
await page.waitForTimeout(900); // hold the final frame a beat
// Screencast emits only on repaint, so a static final beat sends no frames — the
// last captured frame would otherwise default to a single tick. Hold it for the
// real remaining wall-time so the ending plays full-length.
const tEnd = Date.now();
await client.send("Page.stopScreencast").catch(() => {});
await Promise.all(writes);
await context.close();
await browser.close();
if (errors.length) console.log("  (page errors)\n  " + errors.join("\n  "));
if (frames.length < 2) throw new Error(`screencast captured ${frames.length} frames`);
console.log(`● ${frames.length} frames captured`);

// Assemble the frames into a near-lossless intermediate at a constant fps, using
// each frame's swap timestamp for true timing — a static hold becomes one frame
// held for the gap; motion keeps the frames as fast as they painted. The existing
// transcode (poster cut + two-pass) then runs on THIS crisp source.
const FPS = 30;
// The last frame holds for the real remaining wall-time (total capture minus the
// span the timestamps already cover), so a static ending isn't clipped.
const span = frames[frames.length - 1].t - frames[0].t;
const tailHold = Math.max(1 / FPS, (tEnd - tStart) / 1000 - span);
const concat =
  frames
    .map((f, i) => {
      const dur = i + 1 < frames.length ? Math.max(0.001, frames[i + 1].t - f.t) : tailHold;
      return `file '${f.file}'\nduration ${dur.toFixed(4)}`;
    })
    .join("\n") + `\nfile '${frames[frames.length - 1].file}'\n`;
const listFile = `${OUT}/frames.txt`;
writeFileSync(listFile, concat);
const webm = `${OUT}/source.mp4`; // the crisp intermediate (the rest of the pipeline reads `webm`)
console.log("● assembling intermediate …");
execFileSync(
  "ffmpeg",
  [
    "-y", "-f", "concat", "-safe", "0", "-i", listFile,
    "-fps_mode", "cfr", "-r", `${FPS}`,
    // Chrome's screencast JPEGs are BT.601, FULL-range. Browsers (the GitHub embed
    // target) ignore the full-range flag and assume limited BT.709, so a full-range
    // file washes out there. Convert ONCE to the web standard — BT.601→709 matrix +
    // full→limited range — so it decodes correctly in browsers AND local players.
    "-vf", "colorspace=ispace=smpte170m:iprimaries=bt709:itrc=bt709:irange=pc:space=bt709:primaries=bt709:trc=bt709:range=tv:format=yuv420p",
    "-c:v", "libx264", "-crf", "12", "-preset", "veryfast", "-pix_fmt", "yuv420p",
    "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709", "-color_range", "tv",
    webm,
  ],
  { stdio: "inherit" },
);
const mp4 = `${OUT}/${scene}.mp4`;

// Drop the leading flat fill before the app's first real paint, so frame 1 (the
// GitHub thumbnail) is the poster — the finished Ledger — not a startup flash.
// That fill is the browser's pre-paint background; it can be WHITE or black and
// its length swings with dev-server warmth, so we don't guess — we measure the
// leading run and trim just past it. White is found by negating first (white →
// black); the dark poster doesn't trip either, so detection stops at the seam.
function leadingFlatEnd(file) {
  const run = (vf) => {
    const r = spawnSync("ffmpeg", ["-i", file, "-vf", vf, "-an", "-f", "null", "-"], { encoding: "utf8" });
    for (const m of `${r.stderr ?? ""}`.matchAll(/black_start:([0-9.]+)\s+black_end:([0-9.]+)/g)) {
      if (parseFloat(m[1]) < 0.2) return parseFloat(m[2]); // anchored at the top of the clip
    }
    return null;
  };
  const white = run("negate,blackdetect=d=0.05:pic_th=0.98");
  if (white != null) return white;
  // A leading black run can bleed into the (very dark) poster, so cap it tight.
  const black = run("blackdetect=d=0.05:pic_th=0.98");
  return black != null && black < 1.2 ? black : null;
}

const flatEnd = leadingFlatEnd(webm);
// +0.12s clears the dissolve so the poster frame is settled; guard a runaway
// measurement and fall back to a sane fixed cut.
const posterStart = flatEnd != null && flatEnd < 4 ? flatEnd + 0.12 : 0.45;

const fps = (() => {
  const [n, d] = execFileSync("ffprobe", [
    "-v", "error", "-select_streams", "v", "-show_entries", "stream=avg_frame_rate", "-of", "csv=p=0", webm,
  ]).toString().trim().split("/").map(Number);
  return d ? n / d : 25;
})();

// The scene flashes the finished-Ledger poster, then rewinds to the empty picker.
// GitHub only uses frame 1 as the thumbnail, so we keep a SINGLE poster frame and
// cut straight to the picker — the poster never lingers in playback. Find that
// cut by luma: the poster sits bright; the rewind darkens it.
function detectPickerCut(file, start, rate) {
  const dir = join(OUT, "_scan");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  spawnSync("ffmpeg", ["-v", "error", "-ss", `${start}`, "-i", file, "-t", "1.6", "-vf", `fps=${rate}`, join(dir, "s_%03d.png")]);
  const luma = readdirSync(dir).filter((f) => f.endsWith(".png")).sort().map((f) => {
    const r = spawnSync("ffprobe", ["-v", "error", "-f", "lavfi", "-i", `movie=${join(dir, f)},signalstats`,
      "-show_entries", "frame_tags=lavfi.signalstats.YAVG", "-of", "csv=p=0"], { encoding: "utf8" });
    return parseFloat(`${r.stdout}`.trim());
  });
  const base = luma[0];
  for (let i = 1; i < luma.length; i++) {
    // First frame the rewind darkens the poster; +3 frames clears the hard cut's
    // codec smear so we splice onto the settled picker.
    if (base - luma[i] > 2.5) return start + Math.min(i + 3, luma.length - 1) / rate;
  }
  return start + 0.4; // fallback: skip a typical poster hold
}

const pickerCut = detectPickerCut(webm, posterStart, fps);
const frameDur = 1 / fps;
// Exactly one poster frame (the thumbnail), then everything from the picker
// onward — the ~0.3s poster hold and rewind smear in between are dropped.
const splice =
  `[0:v]trim=start=${posterStart.toFixed(4)},select=eq(n\\,0),setpts=PTS-STARTPTS[p];` +
  `[0:v]trim=start=${pickerCut.toFixed(4)},setpts=PTS-STARTPTS[b];` +
  `[p][b]concat=n=2:v=1[v]`;
console.log(`● poster@${posterStart.toFixed(2)}s · picker cut@${pickerCut.toFixed(2)}s (flat ${flatEnd == null ? "none" : flatEnd.toFixed(2) + "s"}, ${fps | 0}fps)`);

const rawDur = parseFloat(
  execFileSync("ffprobe", [
    "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", webm,
  ]).toString().trim(),
);
const dur = rawDur - pickerCut + frameDur;

// Solve a video bitrate that lands the whole file under the cap (no audio),
// leaving ~5% headroom for muxing overhead.
const bitrate = Math.floor((maxMB * 1024 * 1024 * 8 * 0.95) / dur);
console.log(`● ${dur.toFixed(1)}s → ${(bitrate / 1000) | 0} kbps (cap ${maxMB} MB)`);

const enc = (pass, extra) =>
  execFileSync(
    "ffmpeg",
    [
      "-y", "-i", webm,
      "-filter_complex", splice, "-map", "[v]",
      "-c:v", "libx264", "-pix_fmt", "yuv420p", "-preset", "slow",
      // Intermediate is already BT.709/limited; just preserve and tag it.
      "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709", "-color_range", "tv",
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
