/**
 * Beats 5–6: the model you can read, and what an architecture style means.
 */

import type { ReactNode } from "react";
import type { StyleViolation } from "../../src/entities/model/health";
import type { Node, ScryModel } from "../../src/entities/model/viewmodel";
import { ClaimRows, TestChip, fixture, nodeId, placedModel } from "../shots/app";
import { Diagram, type SceneSpec } from "../shots/diagram";
import { C, Caption, Code, col, Figure, INK, Label, Lead, MONO, Page, span, SYNTAX as S, Terminal, tok } from "../shots/stage";
import { fadeOut, prog, revealed, s, settle } from "./motion";

const SYSTEM = nodeId("Aperture Pay");
const LEDGER = nodeId("Ledger");
const DOMAIN = nodeId("domain", LEDGER);
const APPLICATION = nodeId("application", LEDGER);
const INFRASTRUCTURE = nodeId("infrastructure", LEDGER);
const MEMORY = "film-memory-rs";

/** Ledger's level as the drawing places it: the crate root's helper
 *  functions (which no layer holds) left out, and memory.rs where a beat
 *  puts it. */
function ledgerModel(memory?: "domain" | "infrastructure"): ScryModel {
  const nodes = placedModel.nodes.filter((n) => !(n.parentId === LEDGER && n.kind === "symbol"));
  if (memory) {
    nodes.push({ id: MEMORY, kind: "component", name: "memory.rs", parentId: LEDGER, layer: memory, description: "An entry store that lives as long as the process." } as Node);
  }
  return { ...placedModel, nodes };
}

const FILM_TEXT = { title: 22, count: 18, label: 26, flag: 2.2, edge: 3 };

// --- 5. The model you can read ------------------------------------------------------

export const MODEL = s(10);

/** The drill's stages: when each figure takes over, in frames. */
const STAGE = [0, s(2.4), s(4.7), s(6.6)];
const XFADE = s(0.6);

export function Model({ f }: { f: number }) {
  /** A stage's presence: in as the previous goes, out as the next comes. */
  const presence = (i: number) => {
    const inn = i === 0 ? 1 : prog(f, STAGE[i] - XFADE / 2, XFADE);
    const out = i + 1 < STAGE.length ? 1 - prog(f, STAGE[i + 1] - XFADE / 2, XFADE) : 1;
    return Math.min(inn, out);
  };
  /** Drilling in: the outgoing figure grows a little as it fades, the incoming settles from a little small. */
  const zoom = (i: number) => {
    const inn = i === 0 ? 1 : prog(f, STAGE[i] - XFADE / 2, XFADE);
    const out = i + 1 < STAGE.length ? prog(f, STAGE[i + 1] - XFADE / 2, XFADE) : 0;
    return { transform: `scale(${(0.96 + 0.04 * inn) * (1 + 0.06 * out)})`, transformOrigin: "50% 50%" };
  };
  const FIG = { x: col(4), y: 176, w: span(8), h: 824 };
  const labels = ["Fig. 5a — Aperture Pay, context", "Fig. 5b — Aperture Pay, containers", "Fig. 5c — Ledger, hexagonal", "Fig. 5d — Ledger"];
  const pick = (i: number) => (f >= STAGE[i] + s(1.6) ? (i === 0 ? SYSTEM : i === 1 ? LEDGER : undefined) : undefined);
  return (
    <Page beat="05" subject="The model you can read">
      <Lead at={{ x: col(0), y: 140 }} width={span(4)} size={56} style={settle(f, 0)}>
        Scryer derived all of this from the repo.
      </Lead>
      <Caption at={{ x: col(0), y: 360 }} width={span(4)} style={settle(f, s(0.4))}>
        A newcomer, or an agent, reads the service without opening a file.
      </Caption>
      <div style={{ position: "absolute", left: col(0), top: 600, width: span(4), ...settle(f, STAGE[3] + s(0.3)) }}>
        <Caption at={{ x: 0, y: 0 }} width={span(4)}>
          Where a test lives sets the level its claim speaks for:
        </Caption>
        {[
          ["tests/e2e/**", "system"],
          ["crates/*/tests/**", "container"],
          ["next to the code", "component"],
        ].map(([glob, level], i) => (
          <div key={level} style={{ position: "absolute", top: 112 + i * 50, left: 0, width: span(4), display: "flex", fontFamily: MONO, fontSize: 26, letterSpacing: "-0.02em" }}>
            <span style={{ width: 330, color: INK.soft }}>{glob}</span>
            <span style={{ color: i === 1 ? INK.accent : INK.type }}>{level}</span>
          </div>
        ))}
      </div>
      <Label at={{ x: FIG.x, y: FIG.y - 40 }} style={settle(f, 0)}>
        {labels[[0, 1, 2, 3].reduce((best, i) => (presence(i) > presence(best) ? i : best), 0)]}
      </Label>
      {[0, 1, 2].map((i) =>
        presence(i) > 0 ? (
          <div key={i} style={{ position: "absolute", left: FIG.x, top: FIG.y, width: FIG.w, height: FIG.h, border: `1px solid ${INK.rule}`, borderRadius: 4, overflow: "hidden", opacity: presence(i) * (i === 0 ? prog(f, 0, s(0.8)) : 1) }}>
            <div style={zoom(i)}>
              <Diagram
                scenes={[i === 2 ? { model: ledgerModel(), focusId: LEDGER } : { model: placedModel, focusId: i === 0 ? null : SYSTEM }]}
                from={0}
                width={FIG.w}
                height={FIG.h}
                fit={i === 2 ? 50 : 80}
                extra={i === 2 ? 40 : 0}
                film={FILM_TEXT}
                selected={pick(i)}
              />
            </div>
          </div>
        ) : null,
      )}
      {presence(3) > 0 && (
        <div style={{ position: "absolute", left: FIG.x, top: FIG.y, opacity: presence(3), ...zoom(3) }}>
          <Figure at={{ x: 0, y: 0 }} width={FIG.w / 2} height={FIG.h / 2} scale={2}>
            <LedgerPage />
          </Figure>
        </div>
      )}
    </Page>
  );
}

/** Ledger's page: its header, its description, its claims with their test
 *  files, and what uses it — all derived from the repo. */
function LedgerPage() {
  const node = fixture.tree.planned.nodes.find((n) => n.id === LEDGER)!;
  const claims = fixture.tree.claims.filter((c) => c.nodeId === LEDGER && !c.planned);
  const shown = ["When the service restarts, keeps every posted entry", "Posts every money movement as balanced entries", "When a refund exceeds the captured amount, rejects it"]
    .map((t) => claims.find((c) => c.description === t))
    .filter((c): c is NonNullable<typeof c> => !!c);
  const usedBy = fixture.tree.planned.links
    .filter((l) => l.dst === LEDGER)
    .map((l) => ({ name: fixture.tree.planned.nodes.find((n) => n.id === l.src)?.name ?? l.src, label: l.label }));
  return (
    <div className="shot-grade shot-claims with-tests h-full overflow-hidden bg-[var(--surface)] px-6 pt-5 text-[var(--text)]">
      <div className="text-[11px] text-[var(--text-muted)]">Aperture Pay</div>
      <div className="mt-0.5 flex items-center gap-3">
        <span className="text-[22px] font-semibold tracking-tight text-[var(--text)]">Ledger</span>
        <span className="flex items-center gap-1.5 text-[11px] text-[var(--text-tertiary)]">
          Container · Rust · <span className="rounded border border-[var(--border)] px-1 font-mono">hexagonal</span>
        </span>
        <span className="ml-auto">
          <TestChip covered={claims.length} testable={claims.length} />
        </span>
      </div>
      <p className="mt-2.5 text-[13px] leading-relaxed text-[var(--text-secondary)]">{node.description}</p>
      <ClaimRows host={LEDGER} rows={shown.map((c) => ({ claim: c, state: "proven" }))} />
      {usedBy.length > 0 && (
        <div className="mt-3 text-[12px] text-[var(--text-muted)]">
          <span className="mr-2 font-mono text-[10px] uppercase tracking-wider">Used by</span>
          {usedBy.map((u) => (
            <span key={u.name} className="mr-3">
              <span className="text-[var(--accent)]">{u.name}</span>
              {u.label ? ` — ${u.label}` : ""}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

// --- 6a. Where a style is declared --------------------------------------------------

export const TOML = s(4.4);

const header = (s: string) => tok(S.type, s);
const kv = (key: string, value: string) => (
  <>
    {tok(S.plain, `${key} `)}
    {tok(S.punct, "= ")}
    {value.split(/("[^"]*")/).map((part, i) => (
      <span key={i}>{part.startsWith('"') ? tok(S.string, part) : tok(S.punct, part)}</span>
    ))}
  </>
);

/** aperture-pay's .scryer/structure.toml, its [levels] and Ledger's table. */
const STRUCTURE: ReactNode[] = [
  header("[levels]"),
  kv("e2e", '["tests/e2e/**"]'),
  kv("integration", '["crates/*/tests/**", "dashboard/tests/**"]'),
  "",
  header("[containers.ledger]"),
  kv("name", '"Ledger"'),
  kv("style", '"hexagonal"'),
  kv("layers", '["domain", "application", "infrastructure"]'),
];

export function Toml({ f }: { f: number }) {
  const marked = f >= s(1.6);
  return (
    <Page beat="06" subject="What an architecture style means">
      <Lead at={{ x: col(0), y: 140 }} width={span(7)} style={settle(f, 0)}>
        Each container's style is one line of its config.
      </Lead>
      <Caption at={{ x: col(8), y: 150 }} width={span(4)} style={settle(f, s(0.3))}>
        Scryer reads <C>structure.toml</C> next to the code; the agent never writes it. <C>[levels]</C> sets which tests speak for which level.
      </Caption>
      <Code
        at={{ x: col(0), y: 470 }}
        width={span(9)}
        label="Fig. 6 — .scryer/structure.toml"
        lines={STRUCTURE}
        mark={marked ? 6 : undefined}
        dim={marked ? [0, 1, 2, 4, 5, 7] : []}
        style={settle(f, s(0.5))}
      />
    </Page>
  );
}

// --- 6b–d. The hexagon, the import, the fix -----------------------------------------

export const STYLE = s(12);

/** Sub-beats on this page: the rule, the import, the fix. */
const RULE = 0;
const IMPORT = s(3.7);
const FIX = s(7.5);

const VIOLATION: StyleViolation = {
  kind: "forbidden_link",
  node: MEMORY,
  other: APPLICATION,
  file: "crates/ledger/src/domain/memory.rs",
  container: LEDGER,
  detail: fixture.violation?.detail ?? "domain may depend on domain",
} as StyleViolation;

const HEX_SCENES: SceneSpec[] = [
  { model: ledgerModel(), focusId: LEDGER },
  { model: ledgerModel("domain"), focusId: LEDGER, violations: [VIOLATION] },
  { model: ledgerModel("infrastructure"), focusId: LEDGER },
];

/** The hexagonal style's layer matrix, for the layers Ledger declares. */
const MATRIX: ReactNode[] = [
  <>
    {tok(S.plain, "domain          ")}
    {tok(S.punct, "→ ")}
    {tok(S.string, "domain")}
  </>,
  <>
    {tok(S.plain, "application     ")}
    {tok(S.punct, "→ ")}
    {tok(S.string, "application, domain")}
  </>,
  <>
    {tok(S.plain, "infrastructure  ")}
    {tok(S.punct, "→ ")}
    {tok(S.string, "infrastructure,")}
  </>,
  tok(S.string, "                  application, domain"),
];

/** `scryer-mcp check` on the misplacing branch, verbatim, wrapped to the column. */
const CHECK = [
  "scryer check: 1 finding(s)",
  "- structural: declared link link-7dwh7pzf:",
  "  'domain' (domain) → 'application'",
  "  (application) is illegal in style",
  "  'hexagonal': domain may depend on domain",
  "  (crates/ledger/src/domain/memory.rs)",
];

/** The hexagon takes the page's right; the left column keeps what the
 *  terminal needs. */
const HEX_X = col(5) + 72;
const W_HEX = col(11) + span(1) - HEX_X;
const LEFT_W = HEX_X - 48 - col(0);

export function Style({ f }: { f: number }) {
  // The drawing's step: S0 (no file) → S1 (in domain) → S2 (moved out).
  const appear = prog(f, IMPORT + s(1.0), s(0.7), "move");
  const move = prog(f, FIX + s(2.2), s(1.0), "move");
  const step = f < FIX + s(2.2) ? { from: 0, to: 1, t: appear } : { from: 1, to: 2, t: move };
  const violationIn = prog(f, IMPORT + s(1.8), s(0.5));
  const arrows = [
    [APPLICATION, DOMAIN],
    [INFRASTRUCTURE, APPLICATION],
    [INFRASTRUCTURE, DOMAIN],
  ];
  const left = (at: number, until: number) => {
    const out = until < STYLE ? fadeOut(f, until - s(0.25), s(0.25)) : 1;
    return { ...settle(f, at), opacity: Number(settle(f, at).opacity) * out };
  };
  const checkShown = revealed(f, FIX + s(0.2), 3, CHECK.length);
  const fixLines = [
    ...CHECK.map((text, i) => ({ kind: "out" as const, text, tone: i === 0 ? undefined : ("bad" as const) })),
    { kind: "run" as const, text: "git mv crates/ledger/src/domain/memory.rs \\" },
    { kind: "out" as const, text: "    crates/ledger/src/infrastructure/" },
  ];
  const fixShown = f >= FIX + s(1.5) ? CHECK.length + 2 : checkShown;
  return (
    <Page beat="06" subject="What an architecture style means">
      {f < IMPORT && (
        <div style={left(RULE, IMPORT)}>
          <Lead at={{ x: col(0), y: 140 }} width={LEFT_W}>
            Hexagonal: imports point inward.
          </Lead>
          <Caption at={{ x: col(0), y: 300 }} width={LEFT_W - 40}>
            Scryer checks the real import graph against this table. It is deterministic; no LLM is involved.
          </Caption>
          <Code at={{ x: col(0), y: 560 }} width={LEFT_W} size={26} label="hexagonal — what each layer may import" lines={MATRIX} />
        </div>
      )}
      {f >= IMPORT && f < FIX && (
        <div style={left(IMPORT, FIX)}>
          <Lead at={{ x: col(0), y: 140 }} width={LEFT_W}>
            Scryer flags the import. It does not stop the agent.
          </Lead>
          <Caption at={{ x: col(0), y: 300 }} width={LEFT_W - 40}>
            The agent dropped an adapter into domain and imported from application.
          </Caption>
          <Terminal
            at={{ x: col(0), y: 520 }}
            width={LEFT_W}
            size={26}
            label="The agent's session"
            lines={[
              { kind: "tool", name: "write", arg: "crates/ledger/src/domain/memory.rs" },
              { kind: "diff", op: "+", text: "use crate::application::store::EntryStore;" },
              { kind: "diff", op: "+", text: "impl EntryStore for MemoryStore {" },
            ]}
            shown={revealed(f, IMPORT + s(0.3), 6, 3)}
          />
        </div>
      )}
      {f >= FIX && (
        <div style={left(FIX, STYLE)}>
          <Lead at={{ x: col(0), y: 140 }} width={LEFT_W}>
            The fix is moving the file.
          </Lead>
          <Caption at={{ x: col(0), y: 230 }} width={LEFT_W - 40}>
            The style lives in config the agent doesn't write; the finding stays until the code moves.
          </Caption>
          <Terminal at={{ x: col(0), y: 470 }} width={LEFT_W} size={26} label="scryer-mcp check, then the fix" lines={fixLines} shown={fixShown} />
        </div>
      )}
      <Figure at={{ x: HEX_X, y: 176 }} width={W_HEX} height={824} scale={1} label="Fig. 7 — Ledger, hexagonal" style={settle(f, s(0.2))}>
        <Diagram
          scenes={HEX_SCENES}
          from={step.from}
          to={step.to}
          t={step.t}
          width={W_HEX}
          height={824}
          fit={40}
          extra={30}
          clipLeft={-270}
          film={FILM_TEXT}
          hideImplied
          selected={f >= FIX + s(3.2) ? MEMORY : undefined}
          tint={f >= IMPORT + s(2.0) && f < FIX + s(2.6) ? { layer: "domain", color: "var(--color-red-400)" } : undefined}
          edgeReveal={(e) => {
            if (e.violation) return step.from === 1 ? Math.max(0, 1 - 2 * move) : violationIn;
            if (!e.implied) return undefined;
            const k = arrows.findIndex(([a, b]) => e.source === a && e.target === b);
            if (k < 0 || f >= IMPORT) return k < 0 ? undefined : Math.max(0, 1 - prog(f, IMPORT, s(0.4)));
            return prog(f, s(0.9) + k * s(0.45), s(0.5));
          }}
        />
      </Figure>
    </Page>
  );
}
