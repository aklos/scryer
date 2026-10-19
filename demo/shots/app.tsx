/**
 * The app's real components, mounted one at a time for a shot: its
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
import type { DerivedClaim, Fixture } from "../types";

applyTheme({ ...loadTheme(), colorMode: "dark" });

export const fixture = base as Fixture;

/** A claim of the base by its wording. */
export function claim(text: string): DerivedClaim {
  const c = fixture.tree.claims.find((c) => c.description === text);
  if (!c) throw new Error(`no claim "${text}" in the film's base`);
  return c;
}

/** A node of the base by name (`Ledger`), and its container-level id. */
export function nodeId(name: string): string {
  const n = fixture.tree.planned.nodes.find((n) => n.name === name && n.kind !== "symbol");
  if (!n) throw new Error(`no node "${name}" in the film's base`);
  return n.id;
}

/** How a row stands: planned (a todo test, no verdict yet), proven, or
 *  proven with a probe's mark. */
export type RowState = "planned" | "proven" | "crosshair" | "doubleCheck";

const RECORDED = 1_792_400_000;

/**
 * Claim rows exactly as the node page draws them, read-only. A planned row
 * is one the committed layer lacks — the page marks it added and gives it
 * no test lane yet; every other row carries its test and verdict.
 */
export function ClaimRows({ host, rows }: { host: string; rows: { claim: DerivedClaim; state: RowState }[] }) {
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
          probes: 2,
          survived: state === "crosshair" ? 1 : 0,
          survivors: state === "crosshair" ? ["doubling the cap went unnoticed"] : [],
          stale: false,
          recordedAt: RECORDED,
        },
      ]),
  );
  return (
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
