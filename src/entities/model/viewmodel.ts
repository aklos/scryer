/**
 * The tree's schema (v0.3) plus derived view types.
 *
 * `ScryModel`, `Node`, `Link`, `Responsibility`, `Source`, `SchemaProperty`
 * and `Kind` mirror the Rust types in `crates/scryer-core/src/c4_model/model.rs`,
 * so the derived tree the backend serves parses straight into them. The rest
 * are pure helpers over that shape: data-shape checks, the concern lens, id
 * minting for draft claims, and the immutable edits a pending write shows.
 */

export const SCRY_VERSION = "0.3" as const;

// --- Core enums --------------------------------------------------------------

export type Kind =
  | "person"
  | "system"
  | "container"
  | "component"
  | "symbol";

// --- Responsibilities & code-level data --------------------------------------

export interface Responsibility {
  id: string;
  /** Verb-led business statement of accountability. No mechanism words.
   *  EARS-shaped (condition first, response last) and may carry display
   *  markup — `**bold**` on the keyword and response verb — which the UI
   *  renders and strips for comparison (rule 21; see `markup.tsx`). */
  statement: string;
  /** The cross-cutting concern this responsibility serves — at most ONE
   *  kebab-case slug (e.g. "auth"), referencing an entry in the model's
   *  concern registry ({@link ScryModel.concerns}). Untagged means core domain
   *  flow. Metadata beside the statement, no conformance role. Mirrors Rust
   *  `Responsibility.concern`. */
  concern?: string;
  /** Why the behaviour is this one, as markdown: its test's doc after the
   *  claim. Derived; mirrors Rust `Responsibility.decisions`. */
  decisions?: string;
}

export interface SchemaProperty {
  label: string;
  description?: string;
  /** The declared type as written in the source. Mirrors Rust `SchemaProperty.ty`. */
  type?: string;
}

export interface Source {
  pattern: string;
  comment?: string;
}

export interface SourceLocation {
  pattern: string;
  /** Durable anchor: identifier resolved to a line range on demand. */
  symbol?: string;
  line?: number;
  endLine?: number;
}

// --- Nodes & links -----------------------------------------------------------

export interface Node {
  id: string;
  kind: Kind;
  name: string;
  parentId?: string;
  external?: boolean;
  /** What this node IS as software ("Payload 3.0", "PostgreSQL 16", "S3 Bucket"). */
  technology?: string;
  /** 1–2 sentence prose about what this node is. */
  description?: string;
  /** The architectural style this node's code actually follows
   *  ("hexagonal" | "library" | "feature-sliced" | "core-shell" | "pipeline" | a project
   *  style), declared on a container (a component may override). Absent
   *  when the code has no recognisable shape — that absence is the signal.
   *  Mirrors Rust `Node.style`. */
  style?: string;
  /** The layer this component plays inside its governing style — a fixed
   *  tag from the style's layer list, required under a styled container and
   *  absent otherwise; symbols inherit it. Mirrors Rust `Node.layer`. */
  layer?: string;
  responsibilities?: Responsibility[];
  /** Field declarations, when this symbol defines a data shape; a React
   *  component's props. */
  properties?: SchemaProperty[];
  /** The claims (responsibility ids) whose tests exercise this symbol: the
   *  test's body names it, and it is defined in the test's file or a file the
   *  test imports. Derived; the claims stay on their component. Mirrors Rust
   *  `Node.exercised_by`. */
  exercisedBy?: string[];
  /** Optional lucide-react icon name override (frontend-only). */
  icon?: string;
  /** A fixed spot for this node on its parent's map surface (the node's
   *  center, in that surface's coordinate space). The app never sets one; a
   *  curated map (an export) does. Mirrors Rust `Node.position`. */
  position?: { x: number; y: number };
  /** A container declared dev tooling in structure.toml (`tooling = true`),
   *  or a component outside its container's source root (examples, benches):
   *  part of the codebase, not of the product, and held to no style. Mirrors
   *  Rust `Node.tooling`. */
  tooling?: boolean;
  /** The decisions a later reader needs, as markdown: the module doc after
   *  its first paragraph, or `ARCHITECTURE.md` after its first for the
   *  system. Derived. Mirrors Rust `Node.decisions`. */
  decisions?: string;
}

/** What a link between two styled nodes IS. Mirrors Rust `LinkKind`. */
export type LinkKind = "implements" | "calls" | "uses" | "depends";

export interface Link {
  id: string;
  src: string;
  dst: string;
  label: string;
  method?: string;
  /** Required inside a styled container; optional prose-only links remain
   *  legal at system and container level. Mirrors Rust `Link.kind`. */
  kind?: LinkKind;
}

/** One entry in the model's concern registry: the single place a concern is
 *  named and decorated. Responsibilities reference it by `slug`. Mirrors Rust
 *  `ConcernDef`. */
export interface ConcernDef {
  /** Kebab-case identifier, e.g. "auth", "failure-handling". Displayed as-is. */
  slug: string;
  /** One line on what accountability the concern covers. */
  description?: string;
  /** Lucide icon name (PascalCase, e.g. "Shield") — the glyph that prefixes
   *  every responsibility tagged with this concern. Falls back to Tag. */
  icon?: string;
}

export interface ScryModel {
  version: typeof SCRY_VERSION;
  nodes: Node[];
  links: Link[];
  /** The concern registry — one entry per concern slug used by any
   *  responsibility. */
  concerns?: ConcernDef[];
  /** Keyed by **responsibility id** → line-precise locations (conformance
   *  numerator). Agent-produced and regenerable; never hand-authored. */
  sourceMap?: Record<string, SourceLocation[]>;
  /** Keyed by **responsibility id** → locations of the tests that back the
   *  claim (which tests are attached). A separate dimension from `sourceMap` —
   *  where a claim is implemented vs. which tests are attached. Same single-home
   *  layer rule. Agent-produced; never executed. */
  testMap?: Record<string, SourceLocation[]>;
  /** Keyed by **node id** → boundary globs (coverage denominator + extraction
   *  scope). Agent-produced and regenerable; never hand-authored. */
  boundaries?: Record<string, Source[]>;
}

export function emptyModel(): ScryModel {
  return {
    version: SCRY_VERSION,
    nodes: [],
    links: [],
    sourceMap: {},
    boundaries: {},
  };
}

/**
 * A symbol that defines a data type — it declares fields and discharges no
 * behavior. Renders with the table affordance and (like the former `schema`
 * kind) hides its incoming links, which are typically too numerous to be
 * useful. A symbol carrying both properties and responsibilities is not a pure
 * data shape and renders as a normal code node, and neither is a React
 * component, whose properties are its props.
 */
export function isDataShape(node: {
  properties?: SchemaProperty[];
  responsibilities?: Responsibility[];
  technology?: string;
}): boolean {
  return (
    (node.properties?.length ?? 0) > 0 &&
    (node.responsibilities?.length ?? 0) === 0 &&
    node.technology !== REACT_COMPONENT
  );
}

/** The technology the derivation gives a React function component, whose
 *  properties are its props, not a data shape's fields. Mirrors Rust
 *  `REACT_COMPONENT` in scryer-extract's derive. */
export const REACT_COMPONENT = "React component";

/** The anchors visible in the working view. Code-side mapping has a single
 *  home: the committed model owns every committed element's anchor; the plan
 *  overlays anchors only for the elements it *adds* (not yet committed, so they
 *  live in the draft until they fold in). Merging committed under the plan gives
 *  the effective map for display without the draft having to mirror committed —
 *  see the dedup invariant. Display-only: never written back to the plan. */
export function effectiveSourceMap(
  committed: ScryModel | null,
  working: ScryModel,
): Record<string, SourceLocation[]> {
  return { ...(committed?.sourceMap ?? {}), ...(working.sourceMap ?? {}) };
}

/** The attached tests visible in the working view — same single-home merge as
 *  {@link effectiveSourceMap}, for the attached-test dimension. Display-only. */
export function effectiveTestMap(
  committed: ScryModel | null,
  working: ScryModel,
): Record<string, SourceLocation[]> {
  return { ...(committed?.testMap ?? {}), ...(working.testMap ?? {}) };
}

// --- Concerns (the cross-cutting lens) ----------------------------------------

/** The seeded standard concern vocabulary: slug → {description, icon}. Mirrors
 *  Rust `STANDARD_CONCERNS` — keep the two in lockstep. */
export const STANDARD_CONCERNS: ReadonlyMap<string, { description: string; icon: string }> =
  new Map([
    ["auth", { description: "Identity, authentication, and access control", icon: "Shield" }],
    ["persistence", { description: "Durable storage and retrieval of data", icon: "Database" }],
    [
      "failure-handling",
      { description: "Detecting, capturing, and recovering from failures", icon: "AlertTriangle" },
    ],
    ["idempotency", { description: "Making retries and duplicate deliveries safe", icon: "Repeat" }],
    [
      "validation",
      { description: "Checking inputs against expected shape and rules", icon: "CheckCircle" },
    ],
    [
      "observability",
      { description: "Logging, metrics, and tracing for runtime insight", icon: "Activity" },
    ],
    ["performance", { description: "Speed, capacity, and resource efficiency", icon: "Gauge" }],
    [
      "compliance",
      { description: "Satisfying external policy, legal, or platform rules", icon: "Scale" },
    ],
  ]);

/** Per-node subtree tally for one concern: node id → how many responsibilities
 *  tagged `slug` live on the node or anywhere beneath it. Nodes with zero
 *  matches are absent — `counts.has(id)` IS the lens predicate ("does this
 *  concern live here?"). */
export function concernCounts(model: ScryModel, slug: string): Map<string, number> {
  const own = new Map<string, number>();
  for (const node of model.nodes) {
    const n = (node.responsibilities ?? []).filter((r) => r.concern === slug).length;
    if (n > 0) own.set(node.id, n);
  }

  // Roll each node's own count up its ancestor chain.
  const byId = new Map(model.nodes.map((n) => [n.id, n]));
  const out = new Map<string, number>();
  for (const [id, count] of own) {
    let cur: Node | undefined = byId.get(id);
    const seen = new Set<string>();
    while (cur && !seen.has(cur.id)) {
      seen.add(cur.id);
      out.set(cur.id, (out.get(cur.id) ?? 0) + count);
      cur = cur.parentId ? byId.get(cur.parentId) : undefined;
    }
  }
  return out;
}

export function updateNode(
  model: ScryModel,
  nodeId: string,
  patch: Partial<Node>,
): ScryModel {
  return {
    ...model,
    nodes: model.nodes.map((n) => (n.id === nodeId ? { ...n, ...patch } : n)),
  };
}

// --- ID generation -----------------------------------------------------------

// Ids are `<prefix>-<6 random chars>` — the backend's `mint_id` twin. They
// used to be `<prefix>-<max+1>`, which two branches or sessions minting
// against the same snapshot are guaranteed to repeat. A random draw checked
// against every id in sight makes parallel minting safe with no coordination.
// Older sequential ids stay valid: an id is only ever compared for equality.
const ID_ALPHABET = "0123456789abcdefghjkmnpqrstvwxyz";
const ID_SUFFIX_LEN = 6;

function drawSuffix(): string {
  const bytes = new Uint8Array(ID_SUFFIX_LEN);
  if (typeof crypto !== "undefined" && crypto.getRandomValues) crypto.getRandomValues(bytes);
  else for (let i = 0; i < bytes.length; i++) bytes[i] = Math.floor(Math.random() * 256);
  let out = "";
  for (const b of bytes) out += ID_ALPHABET[b & 31];
  return out;
}

/** Mint `<prefix>-<suffix>`, redrawing until it is in none of `existing`. */
export function mintId(prefix: string, existing: Iterable<string>): string {
  const taken = new Set(existing);
  for (;;) {
    const id = `${prefix}-${drawSuffix()}`;
    if (!taken.has(id)) return id;
  }
}

// Claim ids are unique model-wide, so a draft claim's id clears every node's
// claims in every layer. `draft` covers editor rows not yet written.
export function nextResponsibilityId(
  draft: Responsibility[],
  ...models: (ScryModel | null | undefined)[]
): string {
  const ids = draft.map((r) => r.id);
  for (const m of models) {
    if (!m) continue;
    for (const n of m.nodes) for (const r of n.responsibilities ?? []) ids.push(r.id);
  }
  return mintId("resp", ids);
}
