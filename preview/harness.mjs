/**
 * Spike harness — measures how many components render with zero LLM help.
 *
 * Starts the preview server, opens /__harness in headless Firefox (the page
 * renders every discovered component in an iframe and POSTs results back),
 * then prints a per-component table and summary counts.
 *
 * CLI: node preview/harness.mjs [projectRoot] [--no-wrapper]
 */

import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { startPreviewServer } from "./server.mjs";

const args = process.argv.slice(2);
const projectRoot = path.resolve(args.find((a) => !a.startsWith("--")) ?? process.cwd());
const useWrapper = !args.includes("--no-wrapper");

let resolveReport;
const reportPromise = new Promise((r) => (resolveReport = r));

console.error(`analyzing + starting preview server for ${projectRoot} (wrapper: ${useWrapper ? "on" : "off"})...`);
const t0 = Date.now();
const { server, url } = await startPreviewServer({
  projectRoot,
  useWrapper,
  onReport: (results) => resolveReport(results),
});
console.error(`server up at ${url} in ${((Date.now() - t0) / 1000).toFixed(1)}s`);

const profileDir = fs.mkdtempSync(path.join(os.tmpdir(), "scryer-preview-ff-"));
const firefox = spawn(
  "firefox",
  ["--headless", "--new-instance", "-profile", profileDir, `${url}/__harness`],
  { stdio: "ignore", env: { ...process.env, MOZ_HEADLESS: "1" } },
);

const timeoutMs = 10 * 60 * 1000;
const results = await Promise.race([
  reportPromise,
  new Promise((r) => setTimeout(() => r(null), timeoutMs)),
]);

firefox.kill("SIGKILL");
fs.rmSync(profileDir, { recursive: true, force: true });
await server.close();

if (!results) {
  console.error("TIMED OUT waiting for harness report");
  process.exit(1);
}

const pad = (s, n) => String(s).padEnd(n);
const width = Math.max(...results.map((r) => (r.file + ":" + r.exportName).length)) + 2;
console.log(`\n${pad("STATUS", 9)}${pad("MS", 7)}${pad("COMPONENT", width)}DETAIL`);
for (const r of results) {
  const detail = r.error ? r.error.split("\n")[0].slice(0, 120) : (r.warnings?.length ? `(${r.warnings.length} synth warnings)` : "");
  console.log(`${pad(r.status, 9)}${pad(r.ms ?? "", 7)}${pad(r.file + ":" + r.exportName, width)}${detail}`);
}

const counts = {};
for (const r of results) counts[r.status] = (counts[r.status] ?? 0) + 1;
const total = results.length;
console.log(`\n${total} components: ` + Object.entries(counts).map(([k, v]) => `${v} ${k}`).join(", "));
console.log(`render rate: ${counts.ok ?? 0}/${total} ok (${Math.round(((counts.ok ?? 0) / total) * 100)}%)`);
process.exit(0);
