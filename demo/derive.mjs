#!/usr/bin/env node
/**
 * `pnpm demo:derive [project] [--out file]` — the film's base from a real
 * project: runs `scryer-mcp export` on it (default ~/Projects/aperture-pay),
 * lays the demo's story over it (story.ts: the sessions, the verdict dates,
 * the file map for the files they name, the placements) and writes
 * demo/fixture.json, which the film then plays instead of fixture.ts.
 *
 * Fails without writing anything when the project is missing, has no
 * `.scryer/`, or exports nothing usable. Names the story or the script uses
 * that the project lacks are listed as warnings: the film falls back to
 * fixture.ts's text for those and may look wrong where they show.
 */

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const demo = path.dirname(fileURLToPath(import.meta.url));
const repo = path.dirname(demo);

function fail(message) {
  console.error(`demo:derive: ${message}`);
  process.exit(1);
}

const args = process.argv.slice(2);
let project = path.join(os.homedir(), "Projects", "aperture-pay");
let out = path.join(demo, "fixture.json");
for (let i = 0; i < args.length; i++) {
  if (args[i] === "--out" && args[i + 1]) out = path.resolve(args[++i]);
  else if (!args[i].startsWith("-")) project = path.resolve(args[i]);
  else fail(`unknown argument '${args[i]}'\nusage: pnpm demo:derive [project] [--out file]`);
}

if (!fs.existsSync(project)) {
  fail(`no project at ${project} — build it first, or pass its path. fixture.json left as it was.`);
}
if (!fs.existsSync(path.join(project, ".scryer"))) {
  fail(`${project} has no .scryer/ — open it in scryer once (or run \`scryer-mcp setup on\` there). fixture.json left as it was.`);
}

let raw;
try {
  raw = execFileSync("cargo", ["run", "-q", "-p", "scryer-mcp", "--", "export", "--project", project], {
    cwd: repo,
    encoding: "utf8",
    maxBuffer: 512 * 1024 * 1024,
    stdio: ["ignore", "pipe", "inherit"],
  });
} catch {
  fail(`scryer-mcp export failed on ${project} (its error is above). fixture.json left as it was.`);
}

let base;
try {
  base = JSON.parse(raw);
} catch (e) {
  fail(`scryer-mcp export printed something that is not JSON: ${e.message}`);
}
const claims = base?.tree?.claims;
if (!Array.isArray(claims) || claims.length === 0 || !base.status || !Array.isArray(base.tree.planned?.nodes)) {
  fail(`${project} exported no claims — is its code tested yet? fixture.json left as it was.`);
}

const { tell, LAYOUT } = await import("./story.ts");
const { resolveNode } = await import("./shapes.ts");
const { fixture, unresolved } = tell(base);

// The nodes the script names besides the placements (film/events.ts).
const named = [...Object.keys(LAYOUT), "Ledger/domain", "Ledger/infrastructure"];
const missing = [...unresolved, ...named.filter((n) => !resolveNode(fixture.tree.planned, n))];
for (const m of missing) console.warn(`demo:derive: warning: ${project} has no '${m}'`);

const tmp = `${out}.tmp`;
fs.writeFileSync(tmp, JSON.stringify(fixture));
fs.renameSync(tmp, out);
const t = fixture.status.totals;
console.log(
  `demo:derive: wrote ${path.relative(process.cwd(), out)} — ${t.claims} claims (${t.proven} proven, ${t.stale} stale, ` +
    `${t.failing} failing, ${t.planned} planned), ${fixture.tree.darkFiles.length} dark files, ` +
    `${fixture.sessions.length} sessions, ${missing.length} warnings`,
);
