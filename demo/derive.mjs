#!/usr/bin/env node
/**
 * `pnpm demo:derive [project] [--out file] [--violation-branch name]` —
 * the film's base from a real project: runs `scryer-mcp export` on it
 * (default ~/Projects/aperture-pay), lays the demo's story over it (story.ts:
 * the sessions, the verdict dates, the file map for the files they name, the
 * placements) and writes demo/fixture.json, which the film then plays
 * instead of fixture.ts.
 *
 * The style act's violation comes from the project too: the branch that
 * misplaces a file (default `demo/misplaced-file`) is checked out in a
 * temporary worktree, exported, and the worktree removed; the violation it
 * derives that the project's own head does not is the one the film shows.
 * Without that branch the film keeps fixture.ts's.
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
let branch = "demo/misplaced-file";
for (let i = 0; i < args.length; i++) {
  if (args[i] === "--out" && args[i + 1]) out = path.resolve(args[++i]);
  else if (args[i] === "--violation-branch" && args[i + 1]) branch = args[++i];
  else if (!args[i].startsWith("-")) project = path.resolve(args[i]);
  else fail(`unknown argument '${args[i]}'\nusage: pnpm demo:derive [project] [--out file] [--violation-branch name]`);
}

if (!fs.existsSync(project)) {
  fail(`no project at ${project} — build it first, or pass its path. fixture.json left as it was.`);
}
if (!fs.existsSync(path.join(project, ".scryer"))) {
  fail(`${project} has no .scryer/ — open it in scryer once (or run \`scryer-mcp setup on\` there). fixture.json left as it was.`);
}

/** `scryer-mcp export` of `dir`, parsed. */
function exportOf(dir) {
  let raw;
  try {
    raw = execFileSync("cargo", ["run", "-q", "-p", "scryer-mcp", "--", "export", "--project", dir], {
      cwd: repo,
      encoding: "utf8",
      maxBuffer: 512 * 1024 * 1024,
      stdio: ["ignore", "pipe", "inherit"],
    });
  } catch {
    fail(`scryer-mcp export failed on ${dir} (its error is above). fixture.json left as it was.`);
  }
  try {
    return JSON.parse(raw);
  } catch (e) {
    fail(`scryer-mcp export printed something that is not JSON: ${e.message}`);
  }
}

const git = (...a) => execFileSync("git", ["-C", project, ...a], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });

/** A node as the script names it: its name, under its container's. */
function namePath(model, id) {
  const byId = new Map(model.nodes.map((n) => [n.id, n]));
  const names = [];
  for (let n = byId.get(id); n; n = n.parentId ? byId.get(n.parentId) : undefined) {
    names.unshift(n.name);
    if (n.kind === "container") break;
  }
  return names.length ? names.join("/") : undefined;
}

/** The violation `branch` derives that the head does not, nodes by name;
 *  the other end is the declared link's, when the detail names one. */
function violationFrom(head) {
  try {
    git("rev-parse", "--verify", "--quiet", `${branch}^{commit}`);
  } catch {
    console.warn(`demo:derive: warning: ${project} has no branch '${branch}'; the style act keeps fixture.ts's violation`);
    return undefined;
  }
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "scryer-demo-"));
  const worktree = path.join(tmp, "worktree");
  let doc;
  try {
    git("worktree", "add", "--detach", worktree, branch);
    doc = exportOf(worktree);
  } finally {
    try {
      git("worktree", "remove", "--force", worktree);
    } catch {
      console.warn(`demo:derive: warning: could not remove the worktree at ${worktree}`);
    }
    fs.rmSync(tmp, { recursive: true, force: true });
  }
  const known = new Set(head.tree.styleViolations.map((v) => `${v.file}|${v.kind}`));
  const v = doc.tree.styleViolations.find((v) => !known.has(`${v.file}|${v.kind}`));
  if (!v) {
    console.warn(`demo:derive: warning: '${branch}' derives no violation the head lacks; the style act keeps fixture.ts's`);
    return undefined;
  }
  const model = doc.tree.planned;
  const link = (model.links ?? []).find((l) => v.detail.includes(`link ${l.id}:`));
  const other = link && (link.src === v.nodeId ? link.dst : link.src);
  return { ...v, nodeId: namePath(model, v.nodeId), other: other && namePath(model, other) };
}

const base = exportOf(project);
const claims = base?.tree?.claims;
if (!Array.isArray(claims) || claims.length === 0 || !base.status || !Array.isArray(base.tree.planned?.nodes)) {
  fail(`${project} exported no claims — is its code tested yet? fixture.json left as it was.`);
}

const { tell, LAYOUT } = await import("./story.ts");
const { resolveNode } = await import("./shapes.ts");
const { fixture, unresolved } = tell(base);
const violation = violationFrom(base);
if (violation) fixture.violation = violation;

// The nodes the script names besides the placements (film/events.ts).
const named = [
  ...Object.keys(LAYOUT),
  "Ledger/domain",
  "Ledger/infrastructure",
  ...(violation ? [violation.nodeId, violation.other].filter(Boolean) : []),
];
const missing = [...unresolved, ...named.filter((n) => !resolveNode(fixture.tree.planned, n))];
for (const m of missing) console.warn(`demo:derive: warning: ${project} has no '${m}'`);

const tmp = `${out}.tmp`;
fs.writeFileSync(tmp, JSON.stringify(fixture));
fs.renameSync(tmp, out);
const t = fixture.status.totals;
console.log(
  `demo:derive: wrote ${path.relative(process.cwd(), out)} — ${t.claims} claims (${t.proven} proven, ${t.stale} stale, ` +
    `${t.failing} failing, ${t.planned} planned), ${fixture.tree.darkFiles.length} dark files, ` +
    `${fixture.sessions.length} sessions, ${violation ? `the violation from '${branch}'` : "no derived violation"}, ` +
    `${missing.length} warnings`,
);
