/**
 * A placeholder for scryer itself: the fixture rendered as plain panels — the
 * landing page, the tree with its concern lens, a node page and the diagram —
 * so the composition plays end to end in Studio today. Step 4's App.tsx mount
 * replaces it, behind the same props; nothing here mirrors the product's JSX.
 */

import { Img, staticFile } from "remotion";
import type { ClaimState, DerivedClaim, Fixture, Node, ProbeMark } from "../types";
import type { View } from "./state";
import "./shell.css";

export interface ShellProps {
  fixture: Fixture;
  view: View;
  selection?: string;
  lens?: string;
  flash?: { ids: string[]; since: number };
  frame: number;
}

const MARK: Record<ClaimState, string> = { proven: "✓", stale: "↻", failing: "✗", planned: "○" };
const DOT: Record<ClaimState, string> = { proven: "#4ade80", stale: "#fbbf24", failing: "#f87171", planned: "#a78bfa" };
const PROBE: Record<ProbeMark, string> = { crosshair: "⌖", doubleCheck: "✓✓" };

/** Claim state and probe mark by claim id. */
function statusIndex(f: Fixture) {
  return new Map(f.status.claims.map((s) => [s.respId, s]));
}

function nodesById(f: Fixture) {
  return new Map(f.tree.planned.nodes.map((n) => [n.id, n]));
}

function ClaimRow({
  claim,
  f,
  flash,
  dim,
  deletable,
}: {
  claim: DerivedClaim;
  f: Fixture;
  flash: boolean;
  dim?: boolean;
  deletable?: boolean;
}) {
  const s = statusIndex(f).get(claim.id);
  const state = s?.state ?? "stale";
  return (
    <div className={`sh-claim${flash ? " sh-claim--flash" : ""}${dim ? " sh-claim--dim" : ""}`}>
      <span className={`sh-mark sh-mark--${state}`}>{MARK[state]}</span>
      <span className="sh-claim-text">
        {claim.description}
        {claim.concerns?.map((c) => (
          <span className="sh-tag" key={c}>
            {c}
          </span>
        ))}
      </span>
      {s?.probe && <span className={`sh-probe sh-probe--${s.probe}`}>{PROBE[s.probe]}</span>}
      <span className="sh-claim-at">
        {claim.file.split("/").pop()}:{claim.line}
      </span>
      {deletable && <span className="sh-del">✕</span>}
    </div>
  );
}

function TopBar({ f }: { f: Fixture }) {
  const t = f.status.totals;
  const item = (n: number, label: string, hot = false) => (
    <span className={hot && n > 0 ? "sh-hot" : undefined}>
      <b>{n}</b>
      {label}
    </span>
  );
  return (
    <div className="sh-top">
      <span className="sh-brand">
        <Img src={staticFile("logo.png")} />
        scryer <span className="sh-project">· aperture-pay</span>
      </span>
      <span className="sh-health">
        {item(t.proven, "proven")}
        {item(t.planned, "planned")}
        {item(t.stale, "stale", true)}
        {item(t.failing, "failing", true)}
        {item(t.styleViolations, "style", true)}
        {item(t.survivingMutants, "survivors", true)}
        {item(t.darkFiles, "dark")}
      </span>
    </div>
  );
}

/** The node and everything under it. */
function subtree(nodes: Node[], id: string): Set<string> {
  const out = new Set([id]);
  for (let grew = true; grew; ) {
    grew = false;
    for (const n of nodes) {
      if (n.parentId && out.has(n.parentId) && !out.has(n.id)) {
        out.add(n.id);
        grew = true;
      }
    }
  }
  return out;
}

function Tree({ f, selection, lens, flashOn }: { f: Fixture; selection?: string; lens?: string; flashOn: (id: string) => boolean }) {
  const nodes = f.tree.planned.nodes;
  const byId = nodesById(f);
  const containers = nodes.filter((n) => n.kind === "container");
  const claimsOf = (ids: Set<string>) => f.tree.claims.filter((c) => ids.has(c.nodeId));
  const scope = selection ? subtree(nodes, selection) : new Set(nodes.map((n) => n.id));
  const shown = claimsOf(scope);
  const groups = [...new Set(shown.map((c) => c.nodeId))];
  const status = f.status.nodes;
  return (
    <>
      <div className="sh-nav">
        <div className="sh-chips">
          {(f.tree.planned.concerns ?? []).map((c) => (
            <span className={`sh-chip${lens === c.slug ? " sh-chip--on" : ""}`} key={c.slug}>
              {c.slug}
            </span>
          ))}
        </div>
        {containers.map((c) => {
          const comps = nodes.filter((n) => n.parentId === c.id);
          const sub = status[c.id]?.subtree;
          const lit = !lens || claimsOf(subtree(nodes, c.id)).some((cl) => cl.concerns?.includes(lens));
          return (
            <div key={c.id} style={{ opacity: lit ? 1 : 0.3 }}>
              <div className={`sh-row${selection === c.id ? " sh-row--sel" : ""}`}>
                <span className="sh-dot" style={{ background: sub && sub.planned > 0 ? DOT.planned : DOT.proven }} />
                {c.name}
                <span className="sh-count">{sub?.claims ?? 0}</span>
              </div>
              {selection === c.id &&
                comps.map((k) => (
                  <div className="sh-row sh-row--child" key={k.id}>
                    {k.name}
                    <span className="sh-count">{status[k.id]?.own.claims ?? 0}</span>
                  </div>
                ))}
            </div>
          );
        })}
      </div>
      <div className="sh-main">
        {lens ? (
          <>
            <div className="sh-eyebrow">Concern · {lens}</div>
            {containers.map((c) => {
              const hits = claimsOf(subtree(nodes, c.id)).filter((cl) => cl.concerns?.includes(lens));
              if (hits.length === 0) return null;
              return (
                <div className="sh-group" key={c.id}>
                  <div className="sh-group-name">{c.name}</div>
                  {hits.map((cl) => (
                    <ClaimRow claim={cl} f={f} key={cl.id} flash={false} />
                  ))}
                </div>
              );
            })}
          </>
        ) : (
          groups.map((g) => (
            <div className="sh-group" key={g}>
              <div className="sh-group-name">{byId.get(g)?.name}</div>
              {shown
                .filter((c) => c.nodeId === g)
                .map((cl) => (
                  <ClaimRow claim={cl} f={f} key={cl.id} flash={flashOn(cl.id)} deletable={cl.planned} />
                ))}
            </div>
          ))
        )}
      </div>
    </>
  );
}

function NodePage({ f, id, flashOn }: { f: Fixture; id: string; flashOn: (id: string) => boolean }) {
  const byId = nodesById(f);
  const node = byId.get(id);
  if (!node) return null;
  const chain: string[] = [];
  for (let p = node.parentId; p; p = byId.get(p)?.parentId) chain.unshift(byId.get(p)?.name ?? p);
  const own = f.tree.claims.filter((c) => c.nodeId === id);
  const below = f.tree.planned.nodes.filter((n) => n.parentId === id);
  return (
    <div className="sh-main">
      <div className="sh-chain">{chain.join(" / ")}</div>
      <h1 className="sh-title">
        {node.name}
        {node.style && <span className="sh-style">style = "{node.style}"</span>}
      </h1>
      <p className="sh-desc">{node.description}</p>
      <div className="sh-eyebrow">Claims</div>
      <div className="sh-group">
        {own.map((c) => (
          <ClaimRow claim={c} f={f} key={c.id} flash={flashOn(c.id)} />
        ))}
      </div>
      {below.map((k) => {
        const cs = f.tree.claims.filter((c) => c.nodeId === k.id);
        if (cs.length === 0) return null;
        return (
          <div className="sh-group" key={k.id}>
            <div className="sh-group-name">
              {k.name}
              {k.layer && k.layer !== k.name ? ` · ${k.layer}` : ""}
            </div>
            {cs.map((c) => (
              <ClaimRow claim={c} f={f} key={c.id} flash={flashOn(c.id)} />
            ))}
          </div>
        );
      })}
    </div>
  );
}

function Landing({ f }: { f: Fixture }) {
  const status = statusIndex(f);
  const byId = nodesById(f);
  const containerOf = (file: string) => byId.get(file.split("/")[0])?.name;
  // A session is live while it touched something in the last hour; two live
  // sessions in one container light it as shared.
  const now = Math.max(...f.sessions.flatMap((s) => s.entries.map((e) => e.at)));
  const live = (i: number) => f.sessions[i].entries.some((e) => e.event === "touch" && e.at >= now - 3600);
  const touchedBy = f.sessions.map((s, i) =>
    live(i) ? new Set(s.entries.flatMap((e) => (e.event === "touch" ? [containerOf(e.file)] : []))) : new Set<string | undefined>(),
  );
  const sharedWith = (i: number) =>
    [...touchedBy[i]].filter((c): c is string => !!c && touchedBy.some((t, j) => j !== i && t.has(c)));
  const pick = (want: (c: DerivedClaim) => boolean) => f.tree.claims.filter(want).slice(0, 6);
  const planned = pick((c) => status.get(c.id)?.state === "planned");
  const attention = pick((c) => ["stale", "failing"].includes(status.get(c.id)?.state ?? "") || status.get(c.id)?.probe === "crosshair");
  const proven = pick((c) => status.get(c.id)?.state === "proven" && c.nodeId.startsWith("ledger")).slice(0, 5);
  return (
    <div className="sh-landing">
      <div className="sh-eyebrow">Since you last looked</div>
      <div className="sh-sessions">
        {f.sessions.map((s) => {
          const task = s.entries.find((e) => e.event === "task");
          const stubs = s.entries.filter((e) => e.event === "stub").length;
          const touched = new Set(s.entries.flatMap((e) => (e.event === "touch" ? [e.file] : []))).size;
          const reached = s.entries.flatMap((e) => (e.event === "reached" ? e.ids : [])).length;
          const lit = sharedWith(f.sessions.indexOf(s));
          return (
            <div className="sh-card" key={s.id}>
              <div className="sh-card-task">{task && task.event === "task" ? task.text : s.id}</div>
              <div className="sh-card-meta">
                {s.id.slice(0, 8)} · {touched} files touched · {stubs} todo test{stubs === 1 ? "" : "s"} · reaches {reached} claims
              </div>
              {lit.map((c) => (
                <span className="sh-shared" key={c}>
                  shared with another session: {c}
                </span>
              ))}
            </div>
          );
        })}
      </div>
      <div className="sh-columns">
        <div>
          <div className="sh-eyebrow">Planned</div>
          {planned.map((c) => (
            <ClaimRow claim={c} f={f} key={c.id} flash={false} />
          ))}
        </div>
        <div>
          <div className="sh-eyebrow">Needs a look</div>
          {attention.map((c) => (
            <ClaimRow claim={c} f={f} key={c.id} flash={false} />
          ))}
        </div>
        <div>
          <div className="sh-eyebrow">Proven in Ledger</div>
          {proven.map((c) => (
            <ClaimRow claim={c} f={f} key={c.id} flash={false} />
          ))}
        </div>
      </div>
    </div>
  );
}

/** A box per node at its stored placement; links between them. */
function Diagram({ f, level, focus }: { f: Fixture; level: "context" | "containers" | "component"; focus?: string }) {
  const nodes = f.tree.planned.nodes;
  const shown = nodes.filter((n) =>
    level === "context"
      ? !n.parentId && n.kind !== "symbol"
      : level === "containers"
        ? n.kind === "container"
        : n.parentId === focus && n.kind === "component",
  );
  const ids = new Set(shown.map((n) => n.id));
  const pos = (id: string) => f.layout[id] ?? { x: 0, y: 0 };
  const W = 230;
  const H = 96;
  const focusNode = nodes.find((n) => n.id === focus);
  const hex = focusNode?.style === "hexagonal";
  const red = new Set(f.tree.styleViolations.map((v) => v.nodeId));
  const links = f.tree.planned.links.filter((l) => ids.has(l.src) && ids.has(l.dst));
  if (hex && red.has("ledger.domain")) {
    links.push({ id: "violation", src: "ledger.domain", dst: "ledger.adapters", label: "" });
  }
  return (
    <div className="sh-diagram">
      {level === "component" && focusNode && (
        <div className="sh-annot">
          [containers.{focusNode.id}] <b>style = "{focusNode.style}"</b>
        </div>
      )}
      <svg width="100%" height="100%" viewBox="0 0 1160 760">
        {links.map((l) => {
          const a = pos(l.src);
          const b = pos(l.dst);
          const bad = l.id === "violation";
          return (
            <line
              key={l.id}
              x1={a.x + W / 2}
              y1={a.y + H / 2}
              x2={b.x + W / 2}
              y2={b.y + H / 2}
              stroke={bad ? "#f87171" : "#52525b"}
              strokeWidth={bad ? 4 : 2}
              strokeDasharray={bad ? "10 6" : undefined}
            />
          );
        })}
        {shown.map((n) => {
          const p = pos(n.id);
          const status = f.status.nodes[n.id]?.subtree;
          const bad = red.has(n.id);
          const shape = hex
            ? `M${p.x + 30},${p.y} L${p.x + W - 30},${p.y} L${p.x + W},${p.y + H / 2} L${p.x + W - 30},${p.y + H} L${p.x + 30},${p.y + H} L${p.x},${p.y + H / 2} Z`
            : null;
          const fill = n.external ? "#27272a" : n.kind === "person" ? "#1e293b" : "#1f2937";
          return (
            <g key={n.id}>
              {shape ? (
                <path d={shape} fill={fill} stroke={bad ? "#f87171" : "#3f3f46"} strokeWidth={bad ? 3 : 1.5} />
              ) : (
                <rect x={p.x} y={p.y} width={W} height={H} rx={n.kind === "person" ? 48 : 12} fill={fill} stroke={bad ? "#f87171" : "#3f3f46"} strokeWidth={bad ? 3 : 1.5} />
              )}
              <text x={p.x + W / 2} y={p.y + 40} textAnchor="middle" fill="#e4e4e7" fontSize="19" fontWeight="600">
                {n.name}
              </text>
              <text x={p.x + W / 2} y={p.y + 66} textAnchor="middle" fill="#8b8b94" fontSize="13">
                {n.layer ? `layer · ${n.layer}` : n.technology ?? (n.kind === "person" ? "person" : n.external ? "external" : "")}
                {status && status.claims > 0 ? ` · ${status.proven}/${status.claims} proven` : ""}
              </text>
            </g>
          );
        })}
      </svg>
    </div>
  );
}

export function Shell({ fixture: f, view, selection, lens, flash, frame }: ShellProps) {
  const flashOn = (id: string) => !!flash && flash.ids.includes(id) && frame - flash.since < 24;
  return (
    <div className="sh">
      <TopBar f={f} />
      <div className="sh-body">
        {view.kind === "landing" && <Landing f={f} />}
        {view.kind === "tree" && <Tree f={f} selection={selection} lens={lens} flashOn={flashOn} />}
        {view.kind === "node" && <NodePage f={f} id={view.node} flashOn={flashOn} />}
        {view.kind === "diagram" && <Diagram f={f} level={view.level} focus={view.focus} />}
      </div>
    </div>
  );
}
