/**
 * Shot 6 — architecture the agent cannot drift from. Ledger's hexagon at
 * 2×: `memory.rs` lands in the domain, depends outward on the application,
 * and the drawing flags it; the agent moves it out. Props: `misplaced`,
 * whether the file still sits in the domain.
 */

import type { StyleViolation } from "../../src/entities/model/health";
import type { Node, ScryModel } from "../../src/entities/model/viewmodel";
import { fixture, nodeId } from "./app";
import { Hexagon } from "./hexagon";
import { Fragment, Headline, Stage, Terminal } from "./stage";

const LEDGER = nodeId("Ledger");
const MEMORY = "shot-memory-rs";

/** Ledger's level as the misplacing branch derives it: its three layers and
 *  the file that landed in the domain (the crate root's helpers, which the
 *  drawing does not place, are left out). */
function ledgerModel(layer: "domain" | "infrastructure"): ScryModel {
  const planned = fixture.tree.planned;
  const memory: Node = {
    id: MEMORY,
    kind: "component",
    name: "memory.rs",
    parentId: LEDGER,
    layer,
    description: "An entry store that lives as long as the process.",
  } as Node;
  return {
    ...planned,
    nodes: [...planned.nodes.filter((n) => !(n.parentId === LEDGER && n.kind === "symbol")), memory],
  };
}

export function Shot6({ misplaced }: { misplaced: boolean }) {
  const model = ledgerModel(misplaced ? "domain" : "infrastructure");
  const application = fixture.tree.planned.nodes.find((n) => n.parentId === LEDGER && n.name === "application")!.id;
  const violations: StyleViolation[] = misplaced
    ? [
        {
          kind: "forbidden_link",
          node: MEMORY,
          other: application,
          file: "crates/ledger/src/domain/memory.rs",
          container: LEDGER,
          detail: fixture.violation?.detail ?? "domain may depend on domain",
        } as StyleViolation,
      ]
    : [];
  return (
    <Stage light={{ x: 1300, y: 540 }}>
      <Headline at={{ x: 120, y: 330 }} width={600}>
        Styles the agent builds to.
      </Headline>
      <Terminal
        at={{ x: 124, y: 640 }}
        width={620}
        lines={[
          { kind: "tool", name: "move", arg: "memory.rs", running: true },
          { kind: "note", text: "  → infrastructure/" },
        ]}
      />
      <Fragment at={{ x: 700, y: 40 }} width={1160} height={1000} scale={1}>
        <Hexagon
          model={model}
          focusId={LEDGER}
          violations={violations}
          width={1160}
          height={1000}
          fit={40}
          film={{ title: 22, count: 20, label: 28, flag: 2.2, edge: 3 }}
          tint={misplaced ? { layer: "domain", color: "#f87171" } : undefined}
        />
      </Fragment>
    </Stage>
  );
}
