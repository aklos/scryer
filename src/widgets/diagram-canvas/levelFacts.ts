/**
 * What a map level is, at a glance in its corner: the focus's kind and
 * technology, the architecture style it is held to (its layers, outermost
 * first, and what the style is for), whether it is dev tooling, and how many
 * cards it holds, how many sit outside it, and how many links break its
 * style. And, in the other corner, the card selected on it: its kind,
 * technology, layer and what that layer may import, where it lives when it
 * sits outside the level, its arrows in and out here, and the first line of
 * its description. Read off the level as drawn, nothing more.
 */

import type { ScryModel } from "../../entities/model/viewmodel";
import type { DiagramScene } from "./diagramLayout";

export interface LevelFacts {
  kind: string;
  technology?: string;
  style?: { name: string; layers: string[]; purpose: string };
  tooling: boolean;
  /** A container drawn without a style: none is declared. */
  unstyled: boolean;
  inside: number;
  /** What the cards inside are, plural. */
  insideWord: string;
  outside: number;
  violations: number;
}

const INSIDE: Record<string, string> = { system: "containers", container: "components", component: "symbols" };

const KIND: Record<string, string> = {
  system: "Software system",
  container: "Container",
  component: "Component",
};

/** The facts for the level focused on `focusId`; none at the top level,
 *  where the systems and people are the whole story. */
export function levelFacts(
  model: Pick<ScryModel, "nodes">,
  focusId: string | null,
  scene: Pick<DiagramScene, "nodes" | "edges" | "styled"> | null,
): LevelFacts | null {
  if (focusId === null || !scene) return null;
  const focus = model.nodes.find((n) => n.id === focusId);
  if (!focus) return null;
  const outside = scene.nodes.filter((n) => n.reference).length;
  return {
    kind: KIND[focus.kind] ?? focus.kind,
    technology: focus.technology || undefined,
    style: scene.styled && { name: scene.styled.name, layers: scene.styled.layers, purpose: scene.styled.purpose },
    tooling: !!focus.tooling,
    unstyled: focus.kind === "container" && !scene.styled && !focus.tooling,
    inside: scene.nodes.length - outside,
    insideWord: INSIDE[focus.kind] ?? "nodes",
    outside,
    violations: scene.edges.filter((e) => e.violations?.length).length,
  };
}

export interface NodeFacts {
  name: string;
  kind: string;
  technology?: string;
  /** Its layer in the level's style, and what that layer may import. */
  layer?: { name: string; mayImport: string[] };
  /** Drawn here as a reference: the node that holds it, elsewhere. */
  livesIn?: string;
  arrowsIn: number;
  arrowsOut: number;
  violations: number;
  /** The description's first paragraph. */
  summary?: string;
}

/** The facts for the card selected on this level; none when nothing is, or
 *  the selection lives on another level. */
export function nodeFacts(
  model: Pick<ScryModel, "nodes">,
  scene: Pick<DiagramScene, "nodes" | "edges" | "styled"> | null,
  selectedId: string | null,
): NodeFacts | null {
  const card = selectedId ? scene?.nodes.find((n) => n.id === selectedId) : undefined;
  if (!scene || !card) return null;
  const parent = model.nodes.find((n) => n.id === model.nodes.find((m) => m.id === card.id)?.parentId);
  const touching = scene.edges.filter((e) => e.source === card.id || e.target === card.id);
  const layer = card.layer && scene.styled?.layers.includes(card.layer) ? card.layer : undefined;
  return {
    name: card.name,
    kind: card.external ? "External system" : card.kind === "person" ? "Person" : (KIND[card.kind] ?? "Symbol"),
    technology: card.technology || undefined,
    layer: layer ? { name: layer, mayImport: scene.styled?.mayImport[layer] ?? [] } : undefined,
    livesIn: card.reference && parent ? parent.name : undefined,
    arrowsIn: touching.filter((e) => e.target === card.id).length,
    arrowsOut: touching.filter((e) => e.source === card.id).length,
    violations: touching.filter((e) => e.violations?.length).length,
    summary: card.description?.split(/\n\s*\n/)[0].trim() || undefined,
  };
}
