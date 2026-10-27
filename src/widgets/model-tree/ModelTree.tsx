/**
 * Left explorer — an IDE-style tree of the full derived hierarchy (persons,
 * systems, containers, components, symbols), with groups rendered as folders
 * that wrap their members, each level framed and titled. Every row carries
 * the mix of its subtree's claims (proven, unverified, todo, failing) as one
 * thin bar, beside its structural violation count. Type-to-filter, the claim
 * state lenses (Todo / Unverified / Failing) and the concern lens narrow it. The structure is the code's, so right-click offers only what
 * lands in code: add a claim (a todo test), or rename a container.
 */

import { Fragment, useRef, useState } from "react";
import { Braces, ChevronRight, Loader2, Pencil, Plus } from "lucide-react";
import type { StyleViolation } from "../../entities/model/health";
import { StructuralCount } from "../../entities/model/StructuralCount";
import type { ScryModel, Node, Group } from "../../entities/model/viewmodel";
import { childKindFor, concernCounts, normalizeConcernSlug } from "../../entities/model/viewmodel";
import type { Editor } from "../../entities/model/editor";
import { CLAIM_LABEL } from "../../entities/model/claimState";
import type { ClaimState, DerivedCounts, DerivedNodeStatus } from "../../entities/model/derived";
import { kindIcon } from "../../entities/model/kindIcon";
import { StyleGlyph } from "../../entities/model/styles/StyleGlyph";
import { styleTable } from "../../entities/model/health";

/** What a style or a layer IS, for the tree's tooltips — the built-in
 *  table (custom styles fall back to their name). */
const STYLE_TABLE = styleTable(null);
function styleTip(style: string): string {
  const def = STYLE_TABLE.get(style);
  return def ? `${def.name} — ${def.description}` : style;
}
import { lookupIcon } from "../../features/search/IconPicker";
import { NAME_MAX, sanitizeIdentifier } from "../../shared/ui/pagekit";
import { InlineText } from "../../shared/ui/InlineText";
import { ContextMenu, type ContextMenuItem } from "../../features/search/ContextMenu";
import { ConfirmPopover } from "../../shared/feedback/ConfirmPopover";
import { Select } from "../../shared/ui/Select";
import type { Selected } from "../../entities/model/navigation";
import { requestAddClaim } from "../../features/derived-tree/addClaim";

const INDENT = 14;
/** Room between two frames one above the other, and a frame's corner. */
const FRAME_GAP = 3;
const FRAME_RADIUS = 6;
/** Room under a level's last row, and between levels closing on one row. */
const FOOT_PAD = 6;
const FOOT_STEP = 5;
/** A frame is open on the right: its top and bottom edges fade out before
 * the panel's edge, so a level doesn't read as running on past it. The tint
 * inside stays whole. */
const OPEN_RIGHT =
  "linear-gradient(#000 0 0) padding-box, linear-gradient(to right, #000 calc(100% - 40px), transparent calc(100% - 4px)) border-box";
// Room at the row's left edge, outside the outermost level's frame.
const GUTTER = 6;
const ROW = "group/row relative flex items-center gap-1 pr-3 h-[26px] cursor-pointer select-none";

/** The order a reader looks at claim states in, and the bar draws them. */
const MIX_ORDER: readonly ClaimState[] = ["failing", "stale", "planned", "proven"];

/** A subtree's claims as one thin bar: each state's share, failing first,
 *  then unverified, todo and proven, each present state at least a sliver so
 *  one changed claim among hundreds still shows. Proven and todo sit quiet;
 *  only what needs someone now is in full colour. `empty` draws the bar's
 *  bare track for a node that could hold claims and holds none: nobody has
 *  said what it does. */
export function ClaimMix({ counts, empty }: { counts: DerivedCounts | undefined; empty: boolean }) {
  const parts = MIX_ORDER.filter((st) => (counts?.[st] ?? 0) > 0);
  const total = parts.reduce((t, st) => t + counts![st], 0);
  if (total === 0 && !empty) return <span className="w-10 shrink-0" />;
  return (
    <span
      title={total ? MIX_ORDER.map((st) => `${counts![st]} ${CLAIM_LABEL[st]}`).join(" · ") : "No claims"}
      className="flex h-[3px] w-10 shrink-0 gap-px overflow-hidden rounded-[1px]"
      style={total ? undefined : { background: "var(--claim-mix-empty)" }}
    >
      {parts.map((st) => (
        <span key={st} style={{ flexGrow: Math.max(counts![st], total * 0.08), background: `var(--claim-mix-${st})` }} />
      ))}
    </span>
  );
}

/** Alphabetical within a level, unnamed entries last. */
const byName = (a: { name: string }, b: { name: string }) =>
  (a.name || "￿").localeCompare(b.name || "￿");

/** One visible row of the flattened tree, in render order — the basis for both
 *  rendering and arrow-key navigation. */
interface TreeRow {
  kind: "node" | "group";
  id: string;
  depth: number;
  hasChildren: boolean;
  isOpen: boolean;
  parent: { kind: "node" | "group"; id: string } | null;
  // Full chain root→immediate-parent, one entry per depth. Drives the indent
  // guide rails (a rail per ancestor) and active-branch highlighting.
  ancestors: { kind: "node" | "group"; id: string }[];
  node?: Node;
  group?: Group;
}

/** What a level of the tree is, from the kinds of its rows, in the words the
 *  map titles its levels with. */
export function levelTitle(kinds: Iterable<string>): string {
  const ks = new Set(kinds);
  if (ks.has("container")) return "Containers";
  if (ks.has("component")) return "Components";
  if (ks.size > 0 && [...ks].every((k) => k === "symbol")) return "Code";
  return "System context";
}

// A single quiet value for every kind icon — the icons are wayfinding, not
// content, so they read at one calm tier rather than a ramp that makes some
// glyphs shout and others vanish. Kind is carried by the silhouette; altitude by
// the label's weight (below).
const ICON_COLOR = "text-[var(--text-muted)]";

// Altitude ramp — the C4 tier carried by label WEIGHT alone, never by fading.
// Every row stays at a readable value (the graphite palette's lower shades are
// crushed, so dropping deep rows to --text-tertiary/-muted reads as "disabled",
// not "subordinate"); depth is already marked by indent + rail. Weight does the
// work: anchors stand up, leaves sit back, both fully legible.
//   person/system → the anchors  (semibold, full-contrast --text)
//   container     → structure     (medium,  --text-secondary)
//   component/symbol → the leaves (regular, --text-secondary — same value,
//                                  separated from container by weight only)
// `color` is applied only when the row isn't selected (selection drives its own
// foreground); `weight` applies always.
function altitudeRamp(node: Node): { weight: string; color: string } {
  if (node.kind === "person" || node.kind === "system")
    return { weight: "font-semibold", color: "text-[var(--text)]" };
  if (node.kind === "container")
    return { weight: "font-medium", color: "text-[var(--text-secondary)]" };
  return { weight: "font-normal", color: "text-[var(--text-secondary)]" };
}

export function ModelTree({
  model,
  selected,
  expanded,
  onSelectNode,
  onSelectGroup,
  onToggle,
  editor,
  activeNodeIds,
  activeLevel,
  concernLens,
  onSetConcernLens,
  previewable,
  structural,
  claimStatus,
}: {
  model: ScryModel;
  /** Symbol ids the preview sidecar can render — derived from its export list,
   *  never stored on the node; drives the preview glyph. */
  previewable: ReadonlySet<string>;
  selected: Selected | null;
  expanded: ReadonlySet<string>;
  onSelectNode: (id: string) => void;
  onSelectGroup: (id: string) => void;
  onToggle: (id: string, expand?: boolean) => void;
  editor: Editor | undefined;
  activeNodeIds: ReadonlySet<string>;
  /** The map's current level — the parent whose children the diagram is showing
   *  (null = top level). Rows at this level get a faint band so the tree mirrors
   *  the map. `undefined` (i.e. not in map view) disables the tint. */
  activeLevel: string | null | undefined;
  /** The active concern lens (a registry slug, or null). Rows whose subtree
   *  holds no responsibility tagged with it DIM — they never hide, because "the
   *  concern lives nowhere here" is exactly what the lens exists to show. */
  concernLens?: string | null;
  onSetConcernLens?: (slug: string | null) => void;
  /** Structural violations charged inside each node's subtree, keyed by node
   *  id — from {@link structuralBySubtree}. Shown on every row, lens or not:
   *  it is the trail from a flagged system down to the file to fix. */
  structural?: ReadonlyMap<string, readonly StyleViolation[]>;
  /** Claim states per node (own and subtree), from the derived status —
   *  drives the gutter letter. */
  claimStatus?: Record<string, DerivedNodeStatus>;
}) {
  const [width, setWidth] = useState(() => {
    const saved = Number(localStorage.getItem("scryer:treeWidth"));
    return saved >= 220 ? saved : 300;
  });
  const [menu, setMenu] = useState<{ x: number; y: number; items: ContextMenuItem[] } | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [confirmDel, setConfirmDel] = useState<
    { rect: DOMRect; run: () => void; label: string } | null
  >(null);

  // Symbols are first-class pages and the page no longer re-lists children,
  // so the tree shows the full hierarchy by default. The altitude toggle
  // remains for reading the model at architecture height.
  const [showSymbols, setShowSymbols] = useState(
    () => localStorage.getItem("scryer:treeSymbols") !== "0",
  );
  const toggleSymbols = () =>
    setShowSymbols((s) => {
      localStorage.setItem("scryer:treeSymbols", s ? "0" : "1");
      return !s;
    });

  // Nothing is dragged in a tree read from the code.
  const dropKey: string | null = null;
  // The ancestor whose thread-rail is hovered — lights that one line (and only
  // that line) along its whole length, Reddit-style. Never a mass.
  const [hoverRail, setHoverRail] = useState<string | null>(null);

  const startResize = (e: React.PointerEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = width;
    const onMove = (ev: PointerEvent) =>
      setWidth(Math.min(560, Math.max(220, startW + (ev.clientX - startX))));
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      setWidth((w) => {
        localStorage.setItem("scryer:treeWidth", String(w));
        return w;
      });
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  // --- filter + lenses --------------------------------------------------------
  // Type-to-filter narrows by name; the lenses narrow by mark: "changes" (the
  // plan — A/M/D/R, the model→code work queue), "drift" (Q/X — model↔code
  // mismatch awaiting a verdict), and "tests" (claims the test dimension wants
  // a look at — untested, failing, or a break the test let through). A branch
  // stays visible when anything below it matches; matching auto-expands.

  const [filter, setFilter] = useState("");
  // A lens narrows the tree to the subtrees holding claims in one state.
  const [lens, setLens] = useState<"all" | "planned" | "stale" | "failing">("all");
  // Inline rename of the ACTIVE concern (the pencil next to the lens picker).
  const [renamingConcern, setRenamingConcern] = useState(false);

  const childIndex = new Map<string | null, Node[]>();
  for (const n of model.nodes) {
    const k = n.parentId ?? null;
    const arr = childIndex.get(k);
    if (arr) arr.push(n);
    else childIndex.set(k, [n]);
  }

  // Concern lens: subtree tallies for the active slug. A node id absent from
  // the map means "this concern lives nowhere below here" — its row dims (it
  // never hides: the dark rows ARE the finding). Registry totals label the
  // picker options.
  const concernTally = concernLens ? concernCounts(model, concernLens) : null;
  const concernTotals = new Map<string, number>();
  {
    const tally = (rs?: { concern?: string }[]) => {
      for (const r of rs ?? [])
        if (r.concern) concernTotals.set(r.concern, (concernTotals.get(r.concern) ?? 0) + 1);
    };
    for (const n of model.nodes) tally(n.responsibilities);
    for (const g of model.groups) tally(g.responsibilities);
  }
  const groupLit = (g: Group) =>
    !concernTally ||
    g.memberIds.some((m) => concernTally.has(m)) ||
    (g.responsibilities ?? []).some((r) => r.concern === concernLens);

  const q = filter.trim().toLowerCase();
  const filterActive = q !== "" || lens !== "all";
  // Branches the user closed while a filter or lens has the tree open; the
  // tree's own expanded state is left as it was, and a new filter starts open.
  const [closedInFilter, setClosedInFilter] = useState<ReadonlySet<string>>(new Set());
  const [filterKey, setFilterKey] = useState(`${q}\0${lens}`);
  if (filterKey !== `${q}\0${lens}`) {
    setFilterKey(`${q}\0${lens}`);
    setClosedInFilter(new Set());
  }
  const toggle = (id: string, expand?: boolean) => {
    if (!filterActive) return onToggle(id, expand);
    setClosedInFilter((prev) => {
      const next = new Set(prev);
      if (expand ?? next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };
  let visibleIds: ReadonlySet<string> | null = null;
  if (filterActive) {
    // A lens matches a row whose subtree holds a claim in its state.
    const lensMatch = (id: string) => lens === "all" || (claimStatus?.[id]?.subtree?.[lens] ?? 0) > 0;
    const matchSelf = (n: Node) =>
      (q === "" || (n.name || "").toLowerCase().includes(q)) && lensMatch(n.id);
    const set = new Set<string>();
    const walk = (n: Node): boolean => {
      let inc = matchSelf(n);
      for (const c of childIndex.get(n.id) ?? []) if (walk(c)) inc = true;
      if (inc) set.add(n.id);
      return inc;
    };
    for (const n of childIndex.get(null) ?? []) walk(n);
    visibleIds = set;
  }

  // --- level derivation -----------------------------------------------------

  // A text search overrides the symbol-altitude toggle — search reaches
  // everything, and a match is a match. The concern lens surfaces LIT symbols
  // the same way: a tagged claim on a symbol must not hide behind the toggle.
  // The claim lenses don't: they narrow the tree at the altitude it is read
  // at, a symbol's claims showing on the component that holds it.
  const visible = (n: Node) =>
    (q !== "" || showSymbols || n.kind !== "symbol" || (concernTally !== null && concernTally.has(n.id))) &&
    (!visibleIds || visibleIds.has(n.id));

  const childNodes = (parentId: string | null) =>
    model.nodes
      .filter((n) => (n.parentId ?? null) === parentId && visible(n))
      .sort(byName);

  const groupsAtLevel = (parentId: string | null): Group[] =>
    model.groups
      .filter((g) => {
        if (g.memberIds.length === 0) return (g.parentNodeId ?? null) === parentId;
        return g.memberIds.some((m) => {
          const n = model.nodes.find((nd) => nd.id === m);
          return n && (n.parentId ?? null) === parentId && visible(n);
        });
      })
      .sort(byName);

  // --- structural edits -----------------------------------------------------

  const addChild = (parent: Node) => {
    if (!editor) return;
    const kind = childKindFor(parent.kind);
    const id = editor.addNode({ kind, parentId: parent.id });
    // A symbol added while the altitude toggle hides symbols would land
    // invisibly — surface them so the new node is where the eye expects it.
    if (kind === "symbol") setShowSymbols(true);
    toggle(parent.id, true);
    onSelectNode(id);
    setRenaming(id);
  };

  // The tree's structure is the code's: nothing is added at the root.
  const openRootMenu = (_e: React.MouseEvent) => {};


  // The structure is the code's, so a node offers only what lands in the
  // code: add a claim (a todo test), rename a container (structure.toml).
  const nodeMenu = (e: React.MouseEvent, node: Node) => {
    if (!editor) return;
    e.preventDefault();
    e.stopPropagation();
    const items: ContextMenuItem[] = [];
    if (node.kind === "system" || node.kind === "container" || node.kind === "component") {
      items.push({
        id: "add-claim",
        label: "Add claim",
        onSelect: () => {
          onSelectNode(node.id);
          requestAddClaim(node.id);
        },
      });
    }
    if (node.kind === "container")
      items.push({ id: "rename", label: "Rename", onSelect: () => setRenaming(node.id) });
    if (items.length > 0) setMenu({ x: e.clientX, y: e.clientY, items });
  };


  // Groups are not derived; nothing to offer.
  const groupMenu = (_e: React.MouseEvent, _group: Group) => {};


  // --- flattened visible rows -----------------------------------------------
  // One traversal produces the rows in render order; rendering maps over it and
  // arrow-key navigation walks it. Indentation is padding, so no nesting needed.

  const rows: TreeRow[] = [];
  {
    const pushNode = (
      node: Node,
      depth: number,
      ancestors: TreeRow["ancestors"],
    ) => {
      const hasChildren =
        childNodes(node.id).length > 0 || groupsAtLevel(node.id).length > 0;
      // While filtering, every surviving branch opens — the match is the
      // point — until the user closes it. The concern lens opens every LIT
      // branch the same way: "where does auth live" must answer itself, not
      // wait for a manual drill-down.
      const isOpen = filterActive
        ? hasChildren && !closedInFilter.has(node.id)
        : expanded.has(node.id) || (concernTally !== null && concernTally.has(node.id));
      const parent = ancestors[ancestors.length - 1] ?? null;
      rows.push({ kind: "node", id: node.id, depth, hasChildren, isOpen, parent, ancestors, node });
      if (isOpen && hasChildren)
        pushLevel(node.id, depth + 1, [...ancestors, { kind: "node", id: node.id }]);
    };
    const pushLevel = (
      parentId: string | null,
      depth: number,
      ancestors: TreeRow["ancestors"],
    ) => {
      const groups = groupsAtLevel(parentId);
      const grouped = new Set<string>();
      for (const g of groups)
        for (const m of g.memberIds)
          if ((model.nodes.find((n) => n.id === m)?.parentId ?? null) === parentId)
            grouped.add(m);
      for (const n of childNodes(parentId).filter((n) => !grouped.has(n.id)))
        pushNode(n, depth, ancestors);
      for (const g of groups) {
        const members = g.memberIds
          .map((id) => model.nodes.find((n) => n.id === id))
          .filter((n): n is Node => n != null && (n.parentId ?? null) === parentId && visible(n))
          .sort(byName);
        const isOpen = filterActive
          ? !closedInFilter.has(g.id)
          : (concernTally !== null && groupLit(g)) || expanded.has(g.id);
        const parent = ancestors[ancestors.length - 1] ?? null;
        rows.push({
          kind: "group", id: g.id, depth,
          hasChildren: members.length > 0, isOpen, parent, ancestors, group: g,
        });
        if (isOpen)
          for (const m of members)
            pushNode(m, depth + 1, [...ancestors, { kind: "group", id: g.id }]);
      }
    };
    pushLevel(null, 0, []);
  }

  // Levels: every level of the tree is a frame, titled with what its rows are
  // in the map's words (System context, Containers, Components, Code), as
  // the map titles the level it draws. The root level is the outermost
  // frame; an open node's children are a frame inside its own, opening with
  // the title under the node's row and closing under its last row inside.
  // Frames nest, their tints adding up, so each level down reads a step
  // deeper. The level the map is showing is drawn in the map's boundary ink. A frame's left edge is a thread: hovering it lights it (and
  // the owner's chevron, which keys off the same id), clicking it collapses
  // the level.
  const ROOT = "";
  const framesOf = (row: TreeRow) => [ROOT, ...row.ancestors.map((a) => a.id)];
  const closingAt = rows.map((row, i) => {
    const next = new Set(rows[i + 1] ? framesOf(rows[i + 1]) : []);
    return new Set(framesOf(row).filter((id) => !next.has(id)));
  });
  const onMap = (id: string) => activeLevel !== undefined && id === (activeLevel ?? ROOT);
  const levelName = (row: TreeRow): string =>
    levelTitle(
      rows.filter((r) => (r.parent?.id ?? ROOT) === (row.parent?.id ?? ROOT)).flatMap((r) => (r.node ? [r.node.kind] : [])),
    );
  // A strip of each frame a row (or a level's title) sits in, back to front.
  const frameStrips = (ids: string[], opts: { head?: string; foot?: ReadonlyMap<string, number> }) =>
    ids.map((id, depth) => {
      const head = id === opts.head;
      const footAt = opts.foot?.get(id);
      const foot = footAt !== undefined;
      const hot = hoverRail === id && id !== ROOT;
      const ink = hot ? "var(--text-muted)" : onMap(id) ? "var(--tree-boundary)" : "var(--tree-rail)";
      const left = GUTTER + 1 + depth * INDENT;
      return (
        <Fragment key={id || "root"}>
          <span
            aria-hidden
            className="pointer-events-none absolute -z-10 bg-[var(--tree-frame)] transition-colors"
            style={{
              left,
              right: 0,
              top: head ? FRAME_GAP : 0,
              bottom: footAt ?? 0,
              borderStyle: "solid",
              borderColor: ink,
              borderWidth: `${head ? 1 : 0}px 0 ${foot ? 1 : 0}px ${hot ? 2 : 1}px`,
              borderTopLeftRadius: head ? FRAME_RADIUS : 0,
              borderBottomLeftRadius: foot ? FRAME_RADIUS : 0,
              WebkitMask: OPEN_RIGHT,
              mask: OPEN_RIGHT,
            }}
          />
          {id !== ROOT && (
            <span
              title="Collapse"
              onMouseEnter={() => setHoverRail(id)}
              onMouseLeave={() => setHoverRail((h) => (h === id ? null : h))}
              onClick={(e) => {
                e.stopPropagation();
                toggle(id, false);
              }}
              className="absolute top-0 z-10 h-full w-2 cursor-pointer"
              style={{ left: left - 4 }}
            />
          )}
        </Fragment>
      );
    });
  const renderFrames = (row: TreeRow) => frameStrips(framesOf(row), {});
  // The strip closing the levels whose last row this is, so the last row
  // keeps room under it as a title gives the first. Levels ending on the
  // same row close one under another, the innermost first.
  const renderLevelFoot = (row: TreeRow, index: number) => {
    const closing = framesOf(row).filter((id) => closingAt[index].has(id));
    if (closing.length === 0) return null;
    const foot = new Map(closing.map((id, k) => [id, FRAME_GAP + k * FOOT_STEP]));
    return (
      <div
        key={`foot:${row.id}`}
        aria-hidden
        className="relative"
        style={{ height: FOOT_PAD + FRAME_GAP + (closing.length - 1) * FOOT_STEP }}
      >
        {frameStrips(framesOf(row), { foot })}
      </div>
    );
  };
  // The title opening a level: above the first row of the root, and of each
  // open node's children.
  const renderLevelTitle = (row: TreeRow, index: number) => {
    const opens = index === 0 || (row.parent !== null && rows[index - 1]?.id === row.parent.id);
    if (!opens) return null;
    const ids = framesOf(row);
    const own = ids[ids.length - 1];
    return (
      <div key={`level:${own || "root"}`} aria-hidden className="relative flex h-[22px] items-end select-none">
        {frameStrips(ids, { head: own })}
        <span
          className={`flex w-full items-center justify-between pb-[3px] pr-3 text-[10px] font-semibold uppercase leading-none tracking-[0.12em] ${
            onMap(own) ? "text-[var(--text-secondary)]" : "text-[var(--text-muted)]"
          }`}
          style={{ paddingLeft: GUTTER + 9 + (ids.length - 1) * INDENT }}
        >
          {levelName(row)}
        </span>
      </div>
    );
  };

  // Nothing moves in a tree read from the code.
  const dropProps = (_row: TreeRow) => ({});


  const dragProps = (_node: Node) => ({});

  // --- keyboard navigation ----------------------------------------------------

  const containerRef = useRef<HTMLDivElement>(null);

  const focusRow = (row: TreeRow) => {
    if (row.kind === "node") onSelectNode(row.id);
    else onSelectGroup(row.id);
    requestAnimationFrame(() => {
      containerRef.current
        ?.querySelector(`[data-rk="${row.kind}:${row.id}"]`)
        ?.scrollIntoView({ block: "nearest" });
    });
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    const t = e.target as HTMLElement;
    if (t.tagName === "INPUT" || t.tagName === "TEXTAREA") return; // renaming
    if (rows.length === 0) return;
    const idx = rows.findIndex(
      (r) => selected && r.kind === selected.kind && r.id === selected.id,
    );
    const cur = idx >= 0 ? rows[idx] : null;
    switch (e.key) {
      case "ArrowDown": {
        e.preventDefault();
        const next = idx < 0 ? rows[0] : rows[Math.min(rows.length - 1, idx + 1)];
        focusRow(next);
        break;
      }
      case "ArrowUp": {
        e.preventDefault();
        const prev = idx < 0 ? rows[0] : rows[Math.max(0, idx - 1)];
        focusRow(prev);
        break;
      }
      case "ArrowRight": {
        e.preventDefault();
        if (cur?.hasChildren && !cur.isOpen) toggle(cur.id, true);
        else if (cur && idx < rows.length - 1) focusRow(rows[idx + 1]);
        break;
      }
      case "ArrowLeft": {
        e.preventDefault();
        if (cur?.isOpen) toggle(cur.id, false);
        else if (cur?.parent) {
          const p = rows.find(
            (r) => r.kind === cur.parent!.kind && r.id === cur.parent!.id,
          );
          if (p) focusRow(p);
        }
        break;
      }
      case "F2": {
        // Rename the selected row — the editor-standard key, alongside the
        // context menu's Rename.
        e.preventDefault();
        if (cur && editor) setRenaming(cur.id);
        break;
      }
    }
  };

  // --- rendering ------------------------------------------------------------

  const renderNode = (row: TreeRow): React.ReactNode => {
    const node = row.node!;
    const isSel = selected?.kind === "node" && selected.id === node.id;
    const ramp = altitudeRamp(node);
    const Icon = lookupIcon(node.icon) ?? kindIcon(node, previewable.has(node.id));
    const active = activeNodeIds.has(node.id);
    const isDrop = dropKey === `node:${node.id}`;

    return (
      <div
        key={`node:${node.id}`}
        data-rk={`node:${node.id}`}
        style={{ paddingLeft: GUTTER + 6 + row.depth * INDENT }}
        onClick={() => onSelectNode(node.id)}
        onDoubleClick={() => row.hasChildren && toggle(node.id)}
        onContextMenu={(e) => nodeMenu(e, node)}
        {...dragProps(node)}
        {...dropProps(row)}
        className={`${ROW} ${
          isSel
            ? "bg-[var(--accent-soft)] text-[var(--text)]"
            : "text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
        } ${isDrop ? "ring-1 ring-inset ring-[var(--border-strong)] bg-[var(--surface-hover)]" : ""} ${
          concernTally && !concernTally.has(node.id) && !isSel
            ? "opacity-40 transition-opacity"
            : ""
        }`}
      >
        {renderFrames(row)}
        <Chevron
          has={row.hasChildren}
          open={row.isOpen}
          sel={isSel}
          hot={hoverRail === node.id}
          onHover={(h) => setHoverRail((cur) => (h ? node.id : cur === node.id ? null : cur))}
          onClick={() => toggle(node.id)}
        />
        {/* A styled container's icon IS its style — the shape you will see
            when you drill in — unless the user picked an icon. */}
        {node.kind === "container" && node.style && !node.icon ? (
          <span className={`ml-1.5 flex h-3.5 w-3.5 shrink-0 items-center ${ICON_COLOR}`} title={styleTip(node.style)}>
            <StyleGlyph style={node.style} />
          </span>
        ) : (
          <Icon className={`ml-1.5 h-3.5 w-3.5 shrink-0 ${ICON_COLOR}`} />
        )}
        <span className={`min-w-0 flex-1 truncate text-sm ${ramp.weight} ${isSel ? "" : ramp.color}`}>
          {renaming === node.id && editor ? (
            <InlineText
              value={node.name}
              autoEdit
              placeholder="name"
              // Symbol names are code identifiers (shape, not length); other
              // kinds are human titles with a length cap. Mirrors the page header.
              maxLength={node.kind === "symbol" ? undefined : NAME_MAX}
              sanitize={node.kind === "symbol" ? sanitizeIdentifier : undefined}
              onCommit={(v) => editor.updateNode(node.id, { name: v })}
              onClose={() => setRenaming(null)}
            />
          ) : (
            // Double-click on the name renames (F2 and the context menu too);
            // double-click elsewhere on the row keeps toggling expand.
            <span
              onDoubleClick={
                editor
                  ? (e) => {
                      e.stopPropagation();
                      setRenaming(node.id);
                    }
                  : undefined
              }
            >
              {node.name || <span className="italic text-[var(--text-ghost)]">Untitled</span>}
            </span>
          )}
        </span>
        {editor && node.kind !== "symbol" && node.kind !== "person" && renaming !== node.id && (
          <button
            type="button"
            title={`Add ${childKindFor(node.kind)}`}
            onClick={(e) => {
              e.stopPropagation();
              addChild(node);
            }}
            className="shrink-0 rounded p-0.5 text-[var(--text-ghost)] opacity-0 transition-opacity group-hover/row:opacity-100 hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)]"
          >
            <Plus className="h-3 w-3" />
          </button>
        )}
        {concernTally && concernTally.has(node.id) && (
          <span
            className="shrink-0 rounded bg-[var(--accent-soft)] px-1 font-mono text-2xs tabular-nums text-[var(--accent)]"
            title={`${concernTally.get(node.id)} responsibilit${concernTally.get(node.id) === 1 ? "y" : "ies"} tagged "${concernLens}" in this subtree`}
          >
            {concernTally.get(node.id)}
          </span>
        )}
        <StructuralCount violations={structural?.get(node.id)} />
        {active && (
          <Loader2 className="h-3 w-3 shrink-0 animate-spin text-violet-500 dark:text-violet-400" />
        )}
        {/* The mix of the subtree's claims. A box that holds code and no
            claims draws an empty bar; a person or an external system holds
            none by nature and draws nothing. */}
        <ClaimMix
          counts={claimStatus?.[node.id]?.subtree}
          empty={(node.kind === "system" && !node.external) || node.kind === "container" || node.kind === "component"}
        />
      </div>
    );
  };

  const renderGroup = (row: TreeRow): React.ReactNode => {
    const group = row.group!;
    const isSel = selected?.kind === "group" && selected.id === group.id;
    // The folder's claims: its members' subtrees together.
    const groupCounts = group.memberIds.reduce<DerivedCounts | undefined>((sum, m) => {
      const c = claimStatus?.[m]?.subtree;
      if (!c) return sum;
      if (!sum) return { ...c };
      for (const st of MIX_ORDER) sum[st] += c[st];
      return sum;
    }, undefined);
    // Declared member count, shown (quietly) only when collapsed.
    const memberCount = row.isOpen ? 0 : group.memberIds.length;

    return (
      <div
        key={`group:${group.id}`}
        data-rk={`group:${group.id}`}
        style={{ paddingLeft: GUTTER + 6 + row.depth * INDENT }}
        onClick={() => onSelectGroup(group.id)}
        onDoubleClick={() => row.hasChildren && toggle(group.id)}
        onContextMenu={(e) => groupMenu(e, group)}
        {...dropProps(row)}
        className={`${ROW} ${
          isSel
            ? "bg-[var(--accent-soft)] text-[var(--text)]"
            : "text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]"
        } ${dropKey === `group:${group.id}` ? "ring-1 ring-inset ring-[var(--border-strong)] bg-[var(--surface-hover)]" : ""} ${
          !groupLit(group) && !isSel ? "opacity-40 transition-opacity" : ""
        }`}
      >
        {renderFrames(row)}
        <Chevron
          has={row.hasChildren}
          open={row.isOpen}
          sel={isSel}
          hot={hoverRail === group.id}
          onHover={(h) => setHoverRail((cur) => (h ? group.id : cur === group.id ? null : cur))}
          onClick={() => toggle(group.id)}
        />
        {/* No folder glyph — the uppercase label is signal enough; an empty
            spacer the width of a node icon keeps group labels in the same
            column as node names so groups read as headers, not a foreign kind. */}
        <span className="ml-1.5 h-3.5 w-3.5 shrink-0" />
        <span className="min-w-0 flex-1 truncate text-2xs font-medium uppercase tracking-[0.07em] text-[var(--text-tertiary)]">
          {renaming === group.id && editor ? (
            <InlineText
              value={group.name}
              autoEdit
              placeholder="group name"
              maxLength={NAME_MAX}
              onCommit={(v) => editor.updateGroup(group.id, { name: v })}
              onClose={() => setRenaming(null)}
            />
          ) : (
            <span
              onDoubleClick={
                editor
                  ? (e) => {
                      e.stopPropagation();
                      setRenaming(group.id);
                    }
                  : undefined
              }
            >
              {group.name || <span className="text-[var(--text-ghost)]">Group</span>}
            </span>
          )}
        </span>
        {memberCount > 0 && (
          <span
            className="shrink-0 font-mono text-2xs tabular-nums text-[var(--text-ghost)]"
            title={`${memberCount} member${memberCount === 1 ? "" : "s"}`}
          >
            {memberCount}
          </span>
        )}
        <ClaimMix counts={groupCounts} empty={false} />
      </div>
    );
  };

  return (
    <div
      ref={containerRef}
      style={{ width }}
      tabIndex={0}
      onKeyDown={onKeyDown}
      className="relative flex h-full max-w-full shrink-0 flex-col border-r border-[var(--border)] bg-[var(--surface)] outline-none"
    >
      {/* Type-to-filter, the symbols toggle, and the lenses: the same tree,
          narrowed — by name, by altitude (symbols or not), or by what is left
          to do (todo claims) and what is unproven (tests). */}
      <div className="flex flex-col gap-1.5 p-2">
        <div className="flex items-center gap-1">
          <input
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                setFilter("");
                e.currentTarget.blur();
              }
            }}
            placeholder="Filter"
            className="min-w-0 flex-1 rounded-md border border-[var(--border)] bg-[var(--surface-field)] px-2 py-1 text-xs text-[var(--text)] outline-none transition-colors placeholder:text-[var(--text-ghost)] focus:border-[var(--accent)] focus:ring-1 focus:ring-[var(--accent)]"
          />
          <button
            type="button"
            aria-pressed={showSymbols}
            aria-label={showSymbols ? "Hide symbols" : "Show symbols"}
            title={
              showSymbols
                ? "Hide symbols — read the tree at architecture altitude"
                : "Show symbols in the tree"
            }
            onClick={toggleSymbols}
            className={`shrink-0 self-stretch rounded-md border border-[var(--border)] px-1.5 transition-colors ${
              showSymbols
                ? "bg-[var(--surface-active)] text-[var(--text-secondary)]"
                : "text-[var(--text-muted)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)]"
            }`}
          >
            <Braces className="h-3.5 w-3.5" />
          </button>
        </div>
        {/* Same joined-segment idiom as the top bar's Wiki/Map toggle — one
            control language across the chrome: single border, hairline
            dividers, the active cell filled. */}
        <div className="flex items-stretch overflow-hidden rounded-md border border-[var(--border)] divide-x divide-[var(--border)]">
          {/* Lenses are a FILTER, not a readout — the standing counts live in
              the powerline (pending work), where they survive the sidebar being narrowed or closed. */}
          {(
            [
              { id: "all", label: "All", title: "Show the whole model" },
              { id: "planned", label: "Todo", title: "Where todo claims are: work planned, not built or not tested yet" },
              { id: "stale", label: "Unverified", title: "Where claims changed since their test last passed, or never ran" },
              { id: "failing", label: "Failing", title: "Where claims are failing" },
            ] as const
          ).map((opt) => (
            <button
              key={opt.id}
              type="button"
              title={opt.title}
              onClick={() => setLens(opt.id)}
              className={`flex flex-1 items-center justify-center gap-1.5 py-1 text-xs transition-colors ${
                lens === opt.id
                  ? "bg-[var(--surface-active)] text-[var(--text)]"
                  : "text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
              }`}
            >
              {opt.label}
            </button>
          ))}
        </div>
        {/* Concern lens — the cross-cutting axis. Picking a concern dims every
            row whose subtree doesn't serve it (tree and map together); rows
            never hide, because "nothing lit" is the answer the lens exists to
            give. Options come from the model's concern registry; the pencil
            renames the ACTIVE concern everywhere (registry + every tagged
            responsibility) — the registry entry is the concept. */}
        {onSetConcernLens && (model.concerns ?? []).length > 0 && (
          <div className="flex items-center gap-1">
            {renamingConcern && concernLens && editor ? (
              <span className="min-w-0 flex-1 rounded-md border border-[var(--accent)] bg-[var(--surface-field)] px-2 py-1 font-mono text-xs text-[var(--text)]">
                <InlineText
                  value={concernLens}
                  autoEdit
                  placeholder="concern slug"
                  onCommit={(v) => {
                    const slug = normalizeConcernSlug(v);
                    if (slug && slug !== concernLens) {
                      editor.renameConcern(concernLens, slug);
                      onSetConcernLens(slug);
                    }
                  }}
                  onClose={() => setRenamingConcern(false)}
                />
              </span>
            ) : (
              <div
                className="flex min-w-0 flex-1"
                title="Concern lens — light up where a cross-cutting concern (auth, idempotency, …) lives; everything else dims"
              >
                <Select
                  value={concernLens ?? ""}
                  onChange={(v) => onSetConcernLens(v || null)}
                  active={!!concernLens}
                  options={[
                    { value: "", label: "Concern lens" },
                    ...(model.concerns ?? []).map((c) => ({
                      value: c.slug,
                      label: `${c.slug} · ${concernTotals.get(c.slug) ?? 0}`,
                    })),
                  ]}
                />
              </div>
            )}
            {concernLens && editor && !renamingConcern && (
              <button
                type="button"
                title={`Rename "${concernLens}" everywhere — the registry entry and every tagged responsibility`}
                onClick={() => setRenamingConcern(true)}
                className="shrink-0 rounded p-1 text-[var(--text-muted)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)]"
              >
                <Pencil className="h-3 w-3" />
              </button>
            )}
          </div>
        )}
      </div>
      <div
        className="isolate flex-1 overflow-y-auto pb-4"
        style={{ scrollbarGutter: "stable" }}
        onContextMenu={openRootMenu}
      >
        {model.nodes.length === 0 ? (
          <div className="px-4 py-6 text-xs text-[var(--text-muted)]">
            No packages found: the tree is read from the project's manifests and tests.
          </div>
        ) : rows.length === 0 && filterActive ? (
          <div className="px-4 py-6 text-xs text-[var(--text-muted)]">
            {q !== ""
              ? "No matches."
              : lens === "planned"
                ? "No todo claims."
                : lens === "stale"
                  ? "Nothing unverified: every claim's verdict is current."
                  : "Nothing failing."}
          </div>
        ) : (
          rows.map((row, i) => (
            <Fragment key={`${row.kind}:${row.id}`}>
              {renderLevelTitle(row, i)}
              {row.kind === "node" ? renderNode(row) : renderGroup(row)}
              {renderLevelFoot(row, i)}
            </Fragment>
          ))
        )}
      </div>

      <div
        onPointerDown={startResize}
        className="group/resize absolute right-0 top-0 z-20 flex h-full w-2 translate-x-1/2 cursor-col-resize items-stretch"
      >
        <span className="m-auto h-full w-px bg-transparent transition-colors group-hover/resize:bg-[var(--border-strong)]" />
      </div>

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={menu.items}
          onClose={() => setMenu(null)}
        />
      )}
      {confirmDel && (
        <ConfirmPopover
          anchorRect={confirmDel.rect}
          label={confirmDel.label}
          onConfirm={() => {
            confirmDel.run();
            setConfirmDel(null);
          }}
          onCancel={() => setConfirmDel(null)}
        />
      )}
    </div>
  );
}

function Chevron({
  has,
  open,
  onClick,
  hot,
  sel,
  onHover,
}: {
  has: boolean;
  open: boolean;
  onClick: () => void;
  // Lit when its thread-rail is hovered, so the rail and its toggle read as one
  // control; hovering the chevron lights the rail in turn (onHover).
  hot?: boolean;
  // On the selected row the rails are hidden and the active surface swallows the
  // ghost caret, so brighten it to stay legible.
  sel?: boolean;
  onHover?: (hovering: boolean) => void;
}) {
  if (!has) return <span className="h-3.5 w-3.5 shrink-0" />;
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      onMouseEnter={() => onHover?.(true)}
      onMouseLeave={() => onHover?.(false)}
      className={`flex h-3.5 w-3.5 shrink-0 items-center justify-center transition-colors ${
        hot || sel ? "text-[var(--text-secondary)]" : "text-[var(--text-ghost)] hover:text-[var(--text-secondary)]"
      }`}
    >
      <ChevronRight
        className={`h-3 w-3 transition-transform ${open ? "rotate-90" : ""}`}
      />
    </button>
  );
}
