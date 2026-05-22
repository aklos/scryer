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
  Link,
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

/** Ids of every ancestor of `nodeId` (root-most last). Empty when at top. */
function ancestorIds(model: ScryModel, nodeId: string | null): Set<string> {
  const ids = new Set<string>();
  let current = nodeId;
  while (current) {
    ids.add(current);
    const node = model.nodes.find((n) => n.id === current);
    current = node?.parentId ?? null;
  }
  if (nodeId) ids.delete(nodeId);
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
    case "operation":
    case "model":
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

/** Outgoing links from `nodeId`. */
function outgoingLinks(model: ScryModel, nodeId: string): Link[] {
  return model.links.filter((l) => l.src === nodeId);
}

/**
 * Resolve everything outside the subtree of `parentId` that is link-connected
 * to something inside it, classified by role.
 *
 * `parentId === null` means the root surface — the in-scope subtree is the
 * entire model, so there's nothing "outside" to put on the perimeter.
 */
export function surfaceContext(
  model: ScryModel,
  parentId: string | null,
): SurfaceContext {
  const out: SurfaceContext = { persons: [], externals: [], refs: [] };
  if (parentId === null) return out;

  const subtree = subtreeIds(model, parentId);
  // Treat the parent node itself as in-scope so links to/from it count.
  subtree.add(parentId);
  const ancestors = ancestorIds(model, parentId);
  // "In our scope" = subtree + ancestors. Ancestor inclusion is how a deeper
  // view inherits the perimeter of its parent (a person who uses the parent
  // system still surrounds the inner container view).
  const inScope = new Set<string>([...subtree, ...ancestors]);

  const refIds = new Set<string>();
  // Outgoing from inScope → outside-of-scope = a ref
  for (const sid of inScope) {
    for (const l of outgoingLinks(model, sid)) {
      if (!subtree.has(l.dst) && !ancestors.has(l.dst)) refIds.add(l.dst);
    }
  }
  // Incoming from outside-of-scope → inScope = a ref
  for (const l of model.links) {
    if (subtree.has(l.src) || ancestors.has(l.src)) continue;
    if (inScope.has(l.dst)) refIds.add(l.src);
  }

  for (const id of refIds) {
    const node = findNode(model, id);
    if (!node) continue;
    let hasIncoming = false;
    let hasOutgoing = false;
    for (const l of outgoingLinks(model, id)) {
      if (inScope.has(l.dst)) {
        hasIncoming = true;
        break;
      }
    }
    outer: for (const sid of inScope) {
      for (const l of outgoingLinks(model, sid)) {
        if (l.dst === id) {
          hasOutgoing = true;
          break outer;
        }
      }
    }
    const direction: LinkDirection =
      hasIncoming && hasOutgoing
        ? "both"
        : hasIncoming
          ? "incoming"
          : "outgoing";
    out[bucketFor(node)].push({
      node,
      altitude: homeAltitude(model, id),
      direction,
    });
  }
  return out;
}
