/**
 * The app's real components, mounted one at a time for the film: its
 * stylesheet and dark theme, the film's base (demo/fixture.json derived from
 * aperture-pay, else fixture.ts), and claim rows as the node page draws them.
 */

import "../../src/app/index.css";
import "./shots.css";
import base from "@film/base";
import { FlaskConical } from "lucide-react";
import { applyTheme, loadTheme } from "../../src/shared/theme/theme";
import type { ClaimTestStatus, ClaimProbeStatus } from "../../src/entities/model/health";
import type { Responsibility } from "../../src/entities/model/viewmodel";
import { ResponsibilitiesSection } from "../../src/widgets/node-page-sections/ResponsibilitiesSection";
import { GAUGE_CHIP } from "../../src/widgets/node-page-sections/kit";
import { REFUND_STUBS } from "../fixture";
import type { DerivedClaim, Fixture } from "../types";

applyTheme({ ...loadTheme(), colorMode: "dark" });

export const fixture = base as Fixture;

/** A claim of the base by its wording — or, for a claim the film writes and
 *  deletes before it ever reaches the repo, the fixture's own stub. */
export function claim(text: string): DerivedClaim {
  const c = fixture.tree.claims.find((c) => c.description === text) ?? REFUND_STUBS.find((c) => c.description === text);
  if (!c) throw new Error(`no claim "${text}" in the film's base`);
  return c;
}

/** The context map's placements: the people, the platform, and its three
 *  external systems (in aperture-pay the Payments DB is one, beside the
 *  banks) in three close columns, so the map fills its figure. */
const FILM_PLACEMENTS: Record<string, { x: number; y: number }> = {
  Cardholder: { x: 100, y: 110 },
  Merchant: { x: 100, y: 400 },
  "Aperture Pay": { x: 450, y: 255 },
  "Acquiring Bank": { x: 800, y: 20 },
  "Card Networks": { x: 800, y: 255 },
  "Payments DB": { x: 800, y: 490 },
};

/** The base's model with the story's placements (by node name) pinned on
 *  the context and container levels, as the app would read them. */
export const placedModel = {
  ...fixture.tree.planned,
  nodes: fixture.tree.planned.nodes.map((n) => {
    const at = n.kind === "component" || n.kind === "symbol" ? undefined : (FILM_PLACEMENTS[n.name] ?? fixture.layout[n.id] ?? fixture.layout[n.name]);
    return at ? { ...n, position: at } : n;
  }),
};

/** A node of the base by name (`Ledger`), skipping code symbols. */
export function nodeId(name: string, parent?: string): string {
  const n = fixture.tree.planned.nodes.find((n) => n.name === name && n.kind !== "symbol" && (!parent || n.parentId === parent));
  if (!n) throw new Error(`no node "${name}" in the film's base`);
  return n.id;
}

/** How a row stands: planned (a todo test, no verdict yet), proven, or
 *  proven with a probe's mark. */
export type RowState = "planned" | "proven" | "crosshair" | "doubleCheck";
export type Row = { claim: DerivedClaim; state: RowState; css?: string };

const RECORDED = 1_792_400_000;

/** The film's grade for claim rows, by row id: proven is the full state
 *  (off-white sentence, filled green flask), planned recedes with no change
 *  mark — the app's emerald "+" for a planned claim is a product choice that
 *  reads backwards on screen, where green means proven. A row's own `css`
 *  (its motion) goes last. */
function filmGrade(rows: Row[]): string {
  return rows
    .map(({ claim: c, state, css }) => {
      const row = `#resp-${CSS.escape(c.id)}`;
      const grade =
        state === "planned"
          ? `${row} { opacity: 0.42; }
             ${row} > :first-child { visibility: hidden; }
             ${row} .font-mono span { color: var(--text) !important; }`
          : `${row} .font-mono span { color: #ece9e2 !important; }
             ${row} span.absolute.right-1 { color: var(--color-emerald-400) !important; }
             ${row} .lucide-flask-conical { fill: rgb(52 211 153 / 0.35); }`;
      return css ? `${grade}\n${row} { ${css} }` : grade;
    })
    .join("\n");
}

/**
 * Claim rows exactly as the node page draws them, read-only. A planned row
 * is one the committed layer lacks — the page marks it added and gives it
 * no test lane yet; every other row carries its test and verdict.
 */
export function ClaimRows({ host, rows }: { host: string; rows: Row[] }) {
  const resps: Responsibility[] = rows.map(({ claim: c }) => ({ id: c.id, statement: c.description, concern: c.concerns?.[0] }));
  const prev = resps.filter((_, i) => rows[i].state !== "planned");
  const proven = rows.filter((r) => r.state !== "planned");
  const testMap = Object.fromEntries(proven.map(({ claim: c }) => [c.id, [{ pattern: c.file, symbol: c.identifier, line: c.line }]]));
  const testVerdicts: Record<string, ClaimTestStatus> = Object.fromEntries(
    proven.map(({ claim: c }) => [c.id, { respId: c.id, outcome: "passed", cases: 1, stale: false, recordedAt: RECORDED }]),
  );
  const probeResults: Record<string, ClaimProbeStatus> = Object.fromEntries(
    rows
      .filter((r) => r.state === "crosshair" || r.state === "doubleCheck")
      .map(({ claim: c, state }) => [
        c.id,
        {
          respId: c.id,
          probes: 1,
          survived: state === "crosshair" ? 1 : 0,
          survivors: state === "crosshair" ? ["doubling the cap went unnoticed"] : [],
          stale: false,
          recordedAt: RECORDED,
        },
      ]),
  );
  return (
    <>
      <style>{filmGrade(rows)}</style>
      <ResponsibilitiesSection
        host="node"
        hostId={host}
        resps={resps}
        prevResps={prev}
        plannedHosts={new Map()}
        concerns={fixture.tree.planned.concerns ?? []}
        sourceMap={{}}
        testMap={testMap}
        testStates={{}}
        testVerdicts={testVerdicts}
        probeResults={probeResults}
        projectPath={null}
        leafHost={false}
        codeBackedHost
        mintId={() => "draft"}
        editor={undefined}
        editing={false}
        onToggle={() => {}}
      />
    </>
  );
}

/** The node page's test chip: flask, covered/testable, toned by verdicts. */
export function TestChip({ covered, testable }: { covered: number; testable: number }) {
  const tone = covered === testable ? "text-emerald-600 dark:text-emerald-400" : "";
  return (
    <span className={`${GAUGE_CHIP} ${tone}`}>
      <FlaskConical className="h-3 w-3" />
      {covered}/{testable}
    </span>
  );
}
