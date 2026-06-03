/**
 * Reference resolution — the "context" elements that surround the current
 * surface. A reference is a node link-connected to something inside this
 * surface's subtree but living outside it.
 *
 * Buckets:
 *   - `persons`     — actors (perimeter, top)
 *   - `externals`   — out-of-scope systems/containers (grid, dashed styling)
 *   - `refs`        — in-scope siblings/ancestors (perimeter, sides, dimmed)
 *
 * Persons and externals propagate across every altitude — a component that
 * talks to an external service shows that service in its own context, not
 * only on the system surface.
 */

import type {
  Altitude,
  Kind,
  Node,
  ScryModel,
} from "./viewmodel";

export type LinkDirection = "incoming" | "outgoing" | "both";

export interface ContextEntry {
  node: Node;
  altitude: Altitude;
  direction: LinkDirection;
}

export interface SurfaceContext {
  persons: ContextEntry[];
  externals: ContextEntry[];
  refs: ContextEntry[];
}

/** A link pointing into a target node — used for rendering incoming pills. */
export interface IncomingLink {
  from: Node;
  label: string;
  method?: string;
}

export function findNode(model: ScryModel, id: string): Node | null {
  return model.nodes.find((n) => n.id === id) ?? null;
}

/** Ids of `nodeId` plus every descendant. */
function subtreeIds(model: ScryModel, nodeId: string | null): Set<string> {
  const ids = new Set<string>();
  const stack: (string | null)[] = [nodeId];
  while (stack.length) {
    const id = stack.pop();
    for (const n of model.nodes) {
      if ((n.parentId ?? null) === id) {
        if (ids.has(n.id)) continue;
        ids.add(n.id);
        stack.push(n.id);
      }
    }
  }
  return ids;
}

function bucketFor(node: Node): keyof SurfaceContext {
  if (node.kind === "person") return "persons";
  if (node.external) return "externals";
  return "refs";
}

/** Altitude where `nodeId` lives, based on its own kind. */
function homeAltitude(model: ScryModel, nodeId: string): Altitude {
  const node = model.nodes.find((n) => n.id === nodeId);
  return altitudeForKind(node?.kind ?? "system");
}

function altitudeForKind(kind: Kind): Altitude {
  switch (kind) {
    case "system":
    case "person":
      return "system";
    case "container":
      return "container";
    case "component":
      return "component";
    case "symbol":
      return "code";
  }
}

/** Every link pointing at `nodeId`. */
export function incomingLinks(model: ScryModel, nodeId: string): IncomingLink[] {
  const out: IncomingLink[] = [];
  for (const l of model.links) {
    if (l.dst !== nodeId) continue;
    const from = findNode(model, l.src);
    if (!from) continue;
    out.push({ from, label: l.label, method: l.method });
  }
  return out;
}

/**
 * The reference cards surrounding the view of `parentId`'s children: every node
 * OUTSIDE this subtree that a *direct child* links to, in either direction,
 * classified by role.
 *
 * A reference earns its place here only by connecting to a node visible AT THIS
 * LEVEL — a direct child. The parent's own links, or an ancestor's, do NOT
 * surface a reference here: that relationship lives at the higher level where it
 * actually connects. A reference that appears on a level it doesn't connect to
 * is an invalid model, never something to render — so this projection cannot
 * produce one, and `validate_model` flags the underlying gap (a relationship
 * stated at one level but never traced down to the child that realizes it).
 *
 * `parentId === null` is the root surface — every node is in scope, so there is
 * nothing outside to put on the perimeter.
 */
export function surfaceContext(
  model: ScryModel,
  parentId: string | null,
): SurfaceContext {
  const out: SurfaceContext = { persons: [], externals: [], refs: [] };
  if (parentId === null) return out;

  const subtree = subtreeIds(model, parentId);
  subtree.add(parentId);
  // Visible at THIS level = the parent's direct children. Only their links pull
  // a reference onto this surface.
  const owned = new Set<string>(
    model.nodes
      .filter((n) => (n.parentId ?? null) === parentId)
      .map((n) => n.id),
  );

  // Direction is from the surface's point of view: a child→ref link is the
  // surface reaching OUT (outgoing); ref→child is the ref reaching IN (incoming).
  const dir = new Map<string, { in: boolean; out: boolean }>();
  const mark = (id: string, key: "in" | "out") => {
    const e = dir.get(id) ?? { in: false, out: false };
    e[key] = true;
    dir.set(id, e);
  };
  for (const l of model.links) {
    if (owned.has(l.src) && !subtree.has(l.dst)) mark(l.dst, "out");
    if (owned.has(l.dst) && !subtree.has(l.src)) mark(l.src, "in");
  }

  for (const [id, e] of dir) {
    const node = findNode(model, id);
    if (!node) continue;
    const direction: LinkDirection =
      e.in && e.out ? "both" : e.in ? "incoming" : "outgoing";
    out[bucketFor(node)].push({
      node,
      altitude: homeAltitude(model, id),
      direction,
    });
  }
  return out;
}
