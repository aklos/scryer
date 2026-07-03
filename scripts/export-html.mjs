#!/usr/bin/env node
/**
 * Export a Scryer model's diagrams as a single, self-contained HTML file.
 *
 * The output is the same navigational diagram the desktop app renders — pan,
 * zoom, and double-click into containers — but as one static `.html` with the
 * model and all assets inlined, so it opens in any browser and drops straight
 * into Slack, email, or a wiki. No backend, no install.
 *
 * Usage:
 *   node scripts/export-html.mjs [projectDir] [-o out.html] [--planned]
 *
 *   projectDir   Project whose model to export (default: current directory).
 *                Must contain a .scryer/ with model.scry.
 *   -o, --out    Output HTML path (default: ./scryer-diagram.html).
 *   --planned    Export the editable plan (planned.scry) instead of the
 *                committed model (model.scry).
 */

import { build } from "vite";
import { mkdtemp, rm, readFile, writeFile, readdir, stat } from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..");

function parseArgs(argv) {
  const args = { project: ".", out: "scryer-diagram.html", planned: false };
  const rest = [];
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "-o" || a === "--out") args.out = argv[++i];
    else if (a === "--planned") args.planned = true;
    else if (a === "-h" || a === "--help") args.help = true;
    else rest.push(a);
  }
  if (rest[0]) args.project = rest[0];
  return args;
}

const HELP = `Export a Scryer model's diagrams as a single self-contained HTML file.

Usage:
  node scripts/export-html.mjs [projectDir] [-o out.html] [--planned]

  projectDir   Project whose model to export (default: current directory).
  -o, --out    Output HTML path (default: ./scryer-diagram.html).
  --planned    Export the plan (planned.scry) instead of committed (model.scry).
`;

/** First *.html anywhere under dir (the single-file build emits exactly one). */
async function findHtml(dir) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      const found = await findHtml(full);
      if (found) return found;
    } else if (entry.name.endsWith(".html")) {
      return full;
    }
  }
  return null;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    process.stdout.write(HELP);
    return;
  }

  const projectDir = path.resolve(args.project);
  const fileName = args.planned ? "planned.scry" : "model.scry";
  const modelFile = path.join(projectDir, ".scryer", fileName);
  if (!existsSync(modelFile)) {
    throw new Error(
      `No model found at ${modelFile}\n` +
        `Pass a project directory containing a .scryer/${fileName}.`,
    );
  }

  const outPath = path.resolve(args.out);
  const tmp = await mkdtemp(path.join(tmpdir(), "scryer-export-"));

  console.log(`Exporting ${path.relative(process.cwd(), modelFile)} …`);
  try {
    process.env.SCRYER_MODEL_FILE = modelFile;
    process.env.SCRYER_OUT_DIR = tmp;
    await build({
      configFile: path.join(repoRoot, "export-viewer", "vite.config.ts"),
      logLevel: "warn",
    });

    const html = await findHtml(tmp);
    if (!html) throw new Error("Build produced no HTML output");
    await writeFile(outPath, await readFile(html));

    const { size } = await stat(outPath);
    console.log(
      `\n✓ ${path.relative(process.cwd(), outPath)} (${(size / 1024).toFixed(0)} KB) — open it in any browser.`,
    );
  } finally {
    await rm(tmp, { recursive: true, force: true });
  }
}

main().catch((err) => {
  console.error(`\n✗ ${err.message ?? err}`);
  process.exit(1);
});
