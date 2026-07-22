/**
 * Left explorer — the definition surface. An IDE-style tree of the full node
 * hierarchy (persons, systems, containers, components, symbols) with groups
 * rendered as folders that wrap their members. Every node is reachable and
 * first-class. A change-letter gutter (A/M/D/R from the plan diff, Q/X for
 * drift) marks each row at a glance and rolls up onto collapsed branches; agent
 * activity is reflected on the rows. Right-click for
 * structural edits (add child, rename, delete, group membership).
 */

import { useRef, useState } from "react";
import { Braces, ChevronRight, Loader2, Pencil, Plus } from "lucide-react";
import type { Completeness } from "./health";
import { CompletenessPie } from "./CompletenessPie";
import type { ScryModel, Node, Group, Kind } from "./viewmodel";
import { childKindFor, concernCounts, normalizeConcernSlug } from "./viewmodel";
import type { Editor } from "./editor";
import type { ModelDiff } from "./planDiff";
import {
  collectPlanEntries,
  combineMarks,
  groupDrift,
  MARK_META,
  type Mark,
  type MarkPair,
  nodeDrift,
  resolveMark,
  rollupMarks,
} from "./changeMarks";
import { kindIcon } from "./kindIcon";
import { lookupIcon } from "./IconPicker";
import { BTN, EYEBROW, NAME_MAX, sanitizeIdentifier } from "./pagekit";
import { InlineText } from "./InlineText";
import { ContextMenu, type ContextMenuItem } from "./ContextMenu";
import { ConfirmPopover } from "./ConfirmPopover";
import { Select } from "./ui/Select";
import type { Selected } from "./NodePage";

const INDENT = 14;
// Fixed gutter at the row's left edge carrying the change letter; depth indent
// and the rails sit to the right of it, so the letters form a clean column.
const GUTTER = 16;
const ROW = "group/row relative flex items-center gap-1 rounded pr-3 h-[26px] cursor-pointer select-none";

// The change letter for one row — a fixed-width, centered, mono cell pinned to
// the row's left edge so the letters line up regardless of depth. Renders an
// empty cell when there's no mark, keeping the label column aligned. A rolled
// mark (`dim`) is a DESCENDANT's change showing through a collapsed branch —
// same letter and hue, dimmed so it never reads as the row's own edit.
function ChangeGutter({ mark, dim }: { mark: Mark | null; dim?: boolean }) {
  return (
    <span
      aria-hidden={!mark}
      title={mark ? (dim ? `${MARK_META[mark].label} — inside this branch` : MARK_META[mark].label) : undefined}
      className={`pointer-events-none absolute inset-y-0 left-0 flex items-center justify-center font-mono text-2xs font-bold ${
        mark ? MARK_META[mark].color : ""
      } ${dim ? "opacity-50" : ""}`}
      style={{ width: GUTTER }}
    >
      {mark}
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
  planDiff,
  committed,
  selected,
  expanded,
  onSelectNode,
  onSelectGroup,
  onToggle,
  editor,
  activeNodeIds,
  activeLevel,
  completeness,
  concernLens,
  onSetConcernLens,
}: {
  model: ScryModel;
  /** Live `diff(committed, planned)` — drives the change-letter gutter. */
  planDiff: ModelDiff;
  /** The committed layer — parents for deleted elements (their D rolls up the
   *  surviving branch) and endpoints for dropped links. */
  committed: ScryModel | null;
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
  /** Per-node build completeness, keyed by node id — drives the row's % +
   *  anchorage badge. Absent until the health report loads. */
  completeness?: Record<string, Completeness>;
  /** The active concern lens (a registry slug, or null). Rows whose subtree
   *  holds no responsibility tagged with it DIM — they never hide, because "the
   *  concern lives nowhere here" is exactly what the lens exists to show. */
  concernLens?: string | null;
  onSetConcernLens?: (slug: string | null) => void;
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

  // Drag-to-move: nodes re-parent by dropping onto a valid parent (kind
  // hierarchy, no cycles), or join a group by dropping onto its folder.
  const [dragId, setDragId] = useState<string | null>(null);
  const [dropKey, setDropKey] = useState<string | null>(null);
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
  // plan — A/M/D/R, the model→code work queue) and "drift" (Q/X — model↔code
  // mismatch awaiting a verdict). A branch stays visible when anything below it
  // matches; matching auto-expands.

  const [filter, setFilter] = useState("");
  const [lens, setLens] = useState<"all" | "changes" | "drift">("all");
  // Inline rename of the ACTIVE concern (the pencil next to the lens picker).
  const [renamingConcern, setRenamingConcern] = useState(false);

  const childIndex = new Map<string | null, Node[]>();
  for (const n of model.nodes) {
    const k = n.parentId ?? null;
    const arr = childIndex.get(k);
    if (arr) arr.push(n);
    else childIndex.set(k, [n]);
  }

  // Per-element plan marks from the SHARED plan-entry computation — the same
  // one the Changes page renders, so the gutter, the lens count, and the page
  // can never disagree. Link changes mark their source node; deleted elements
  // (no row of their own) surface through the roll-up below.
  const planEntries = collectPlanEntries(planDiff, model, committed);
  const entryMark = new Map(planEntries.map((e) => [e.id, e.mark] as const));
  const markOf = new Map<string, MarkPair>();
  for (const n of model.nodes)
    markOf.set(n.id, { plan: entryMark.get(n.id) ?? null, drift: nodeDrift(n) });
  // Descendant marks bubbled per ancestor — shown on collapsed branches.
  const rolledOf = rollupMarks(model, committed, planEntries);
  const hasPlan = (id: string) => markOf.get(id)?.plan != null;
  const hasDrift = (id: string) => markOf.get(id)?.drift != null;

  // Lens counts on the segmented control. "Changes" counts plan CARRIERS —
  // exactly the entries the Changes page lists (deleted elements and changed
  // links included) — not just the rows that happen to exist in the tree.
  const changeCount = planEntries.length;
  let driftCount = 0;
  for (const n of model.nodes) {
    if (hasDrift(n.id)) driftCount++;
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
  let visibleIds: ReadonlySet<string> | null = null;
  if (filterActive) {
    // A lens matches a row for its OWN mark or anything rolled up beneath it,
    // so a plan holding only a deletion still lights the surviving branch.
    const lensMatch = (id: string) =>
      lens === "all" ||
      (lens === "changes"
        ? hasPlan(id) || rolledOf.get(id)?.plan != null
        : hasDrift(id) || rolledOf.get(id)?.drift != null);
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

  // A filter/lens overrides the symbol-altitude toggle — search reaches
  // everything, and a match is a match. The concern lens surfaces LIT symbols
  // the same way: a tagged claim on a symbol must not hide behind the toggle.
  const visible = (n: Node) =>
    visibleIds
      ? visibleIds.has(n.id)
      : showSymbols ||
        n.kind !== "symbol" ||
        (concernTally !== null && concernTally.has(n.id));

  const childNodes = (parentId: string | null) =>
    model.nodes
      .filter((n) => (n.parentId ?? null) === parentId && visible(n))
      .sort(byName);

  /** Symbol children hidden by the altitude filter — surfaced as a quiet count
   *  so a collapsed component still reads as "has content". */
  const hiddenSymbolCount = (nodeId: string) =>
    showSymbols
      ? 0
      : model.nodes.filter((n) => n.parentId === nodeId && n.kind === "symbol").length;

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

  const groupOfNode = (nodeId: string) =>
    model.groups.find((g) => g.memberIds.includes(nodeId));

  const descendantCount = (nodeId: string): number => {
    let n = 0;
    const stack = [nodeId];
    while (stack.length) {
      const id = stack.pop()!;
      for (const c of model.nodes) {
        if (c.parentId === id) {
          n += 1;
          stack.push(c.id);
        }
      }
    }
    return n;
  };

  // --- structural edits -----------------------------------------------------

  const addChild = (parent: Node) => {
    if (!editor) return;
    const kind = childKindFor(parent.kind);
    const id = editor.addNode({ kind, parentId: parent.id });
    // A symbol added while the altitude toggle hides symbols would land
    // invisibly — surface them so the new node is where the eye expects it.
    if (kind === "symbol") setShowSymbols(true);
    onToggle(parent.id, true);
    onSelectNode(id);
    setRenaming(id);
  };

  // Top-level nodes have no parent to "add child" from, so they're seeded here.
  const addRoot = (kind: Kind, external: boolean) => {
    if (!editor) return;
    const id = editor.addNode({ kind, external: external || undefined });
    onSelectNode(id);
    setRenaming(id);
  };
  const rootMenuItems = (): ContextMenuItem[] => [
    { id: "sys", label: "Add system", onSelect: () => addRoot("system", false) },
    { id: "person", label: "Add person", onSelect: () => addRoot("person", false) },
    { id: "ext", label: "Add external system", onSelect: () => addRoot("system", true) },
  ];
  // Right-clicking the empty tree surface adds a top-level node — the root is
  // the "parent" of systems, mirroring right-click-to-add-child on a node.
  const openRootMenu = (e: React.MouseEvent) => {
    if (!editor) return;
    e.preventDefault();
    setMenu({ x: e.clientX, y: e.clientY, items: rootMenuItems() });
  };

  const nodeMenu = (e: React.MouseEvent, node: Node) => {
    if (!editor) return;
    e.preventDefault();
    e.stopPropagation();
    const items: ContextMenuItem[] = [];
    const canHaveChildren = node.kind !== "symbol" && node.kind !== "person";
    if (canHaveChildren) {
      const childKind = childKindFor(node.kind);
      items.push({
        id: "add",
        label: `Add ${childKind}`,
        onSelect: () => addChild(node),
      });
    }
    items.push({ id: "rename", label: "Rename", onSelect: () => setRenaming(node.id) });

    const level = node.parentId ?? null;
    const current = groupOfNode(node.id);
    if (current) {
      items.push({
        id: "remove-group",
        label: `Remove from ${current.name || "group"}`,
        onSelect: () => editor.setNodeGroup(node.id, null),
      });
    }
    for (const g of groupsAtLevel(level)) {
      if (g.id === current?.id) continue;
      items.push({
        id: `addto-${g.id}`,
        label: `Add to ${g.name || "group"}`,
        onSelect: () => editor.setNodeGroup(node.id, g.id),
      });
    }
    items.push({
      id: "new-group",
      label: "New group from this",
      onSelect: () => {
        const gid = editor.addGroup({ parentNodeId: level, memberIds: [node.id] });
        onSelectGroup(gid);
        setRenaming(gid);
      },
    });
    items.push({
      id: "delete",
      label: "Delete",
      onSelect: () => {
        const kids = descendantCount(node.id);
        setConfirmDel({
          rect: new DOMRect(e.clientX, e.clientY, 0, 0),
          label: `Delete ${node.name || "node"}${
            kids > 0 ? ` and ${kids} descendant${kids === 1 ? "" : "s"}` : ""
          }?`,
          run: () => editor.deleteNode(node.id),
        });
      },
    });
    setMenu({ x: e.clientX, y: e.clientY, items });
  };

  const groupMenu = (e: React.MouseEvent, group: Group) => {
    if (!editor) return;
    e.preventDefault();
    e.stopPropagation();
    setMenu({
      x: e.clientX,
      y: e.clientY,
      items: [
        { id: "rename", label: "Rename", onSelect: () => setRenaming(group.id) },
        {
          id: "delete",
          label: "Delete group",
          onSelect: () =>
            setConfirmDel({
              rect: new DOMRect(e.clientX, e.clientY, 0, 0),
              label: `Delete group ${group.name || ""}? Members are kept.`,
              run: () => editor.deleteGroup(group.id),
            }),
        },
      ],
    });
  };

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
      // While filtering, every surviving branch is open — the match is the
      // point. The concern lens opens every LIT branch the same way: "where
      // does auth live" must answer itself, not wait for a manual drill-down.
      const isOpen = filterActive
        ? hasChildren
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
        const isOpen =
          filterActive || (concernTally !== null && groupLit(g)) || expanded.has(g.id);
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

  // Indent guide rails — one continuous vertical thread-line per ancestor depth,
  // aligned to that ancestor's chevron centre (gutter + base pad 6 + half the
  // 14px chevron = GUTTER + 13), sitting to the right of the change-letter
  // gutter. Reddit-style: each line is its own hoverable, clickable thread — the
  // whole line lights when hovered (only that one, never the selected spine),
  // and clicking it collapses that ancestor's subtree. The visible line is 1px;
  // a wider transparent hit-strip around it makes it easy to grab without
  // reaching into the row's content.
  // Rails for one row: one full-height thread-line per ancestor depth, centred at
  // that ancestor's chevron x (gutter + base pad 6 + half the 14px chevron =
  // GUTTER + 13). Each line is its own hoverable, clickable thread — hovering any
  // segment lights the whole line for that ancestor (and its chevron, which keys
  // off the same id), only that one, never the selected spine; clicking collapses
  // it. The 8px hit-strip is even-width so the 1px line centres exactly on x.
  const renderRails = (row: TreeRow) =>
    row.ancestors.map((anc, i) => {
      const hot = hoverRail === anc.id;
      return (
        <span
          key={i}
          title="Collapse"
          onMouseEnter={() => setHoverRail(anc.id)}
          onMouseLeave={() => setHoverRail((h) => (h === anc.id ? null : h))}
          onClick={(e) => {
            e.stopPropagation();
            onToggle(anc.id, false);
          }}
          className="absolute top-0 z-10 h-full w-2 cursor-pointer"
          style={{ left: GUTTER + 13 + i * INDENT - 4 }}
        >
          <span
            className={`absolute inset-y-0 left-1/2 -translate-x-1/2 transition-colors ${
              hot ? "w-0.5 bg-[var(--text-muted)]" : "w-px bg-[var(--border)]"
            }`}
          />
        </span>
      );
    });

  // --- drag-to-move ------------------------------------------------------------

  const draggedNode = dragId ? model.nodes.find((n) => n.id === dragId) : null;

  // Cross-container reparenting is disabled: a node's dependency links are plain
  // (src,dst) pairs that moveNode leaves untouched, so re-homing a node would
  // silently leave its links pointing across container boundaries — an invariant
  // we don't yet reconcile. Until we do, nodes can't be dropped onto other nodes.
  // Drag-into-group still works (it only changes group membership, never links).
  const canDropOnNode = (_target: Node): boolean => false;

  /** Groups accept siblings of their members (same level), nothing else. */
  const canDropOnGroup = (group: Group): boolean => {
    if (!editor || !draggedNode) return false;
    if (group.memberIds.includes(draggedNode.id)) return false;
    const level =
      group.parentNodeId ??
      model.nodes.find((n) => n.id === group.memberIds[0])?.parentId ??
      null;
    return (draggedNode.parentId ?? null) === (level ?? null);
  };

  const dropProps = (row: TreeRow) => {
    if (!editor) return {};
    const key = `${row.kind}:${row.id}`;
    const valid =
      row.kind === "node" ? canDropOnNode(row.node!) : canDropOnGroup(row.group!);
    return {
      onDragOver: (e: React.DragEvent) => {
        if (!valid) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = "move";
        setDropKey(key);
      },
      onDragLeave: () => setDropKey((k) => (k === key ? null : k)),
      onDrop: (e: React.DragEvent) => {
        if (!valid || !draggedNode) return;
        e.preventDefault();
        if (row.kind === "node") editor.moveNode(draggedNode.id, row.id);
        else editor.setNodeGroup(draggedNode.id, row.id);
        onToggle(row.id, true);
        setDropKey(null);
        setDragId(null);
      },
    };
  };

  const dragProps = (node: Node) =>
    editor && renaming !== node.id
      ? {
          draggable: true,
          onDragStart: (e: React.DragEvent) => {
            e.dataTransfer.effectAllowed = "move";
            e.dataTransfer.setData("text/plain", node.id);
            setDragId(node.id);
          },
          onDragEnd: () => {
            setDragId(null);
            setDropKey(null);
          },
        }
      : {};

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
        if (cur?.hasChildren && !cur.isOpen) onToggle(cur.id, true);
        else if (cur && idx < rows.length - 1) focusRow(rows[idx + 1]);
        break;
      }
      case "ArrowLeft": {
        e.preventDefault();
        if (cur?.isOpen) onToggle(cur.id, false);
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

  // --- active-level tint ----------------------------------------------------
  // `activeLevel` (a prop) is the map's current level — the parent whose
  // children the diagram is showing. Every row at that level gets a faint band
  // so the tree mirrors the map. It tracks the level you're VIEWING, not what
  // you've selected within it, and is undefined outside map view (no tint).
  // The band is suppressed on the selected row (it owns the accent band) and
  // yields to hover.
  const levelTint = (atLevel: boolean, isSel: boolean) =>
    atLevel && activeLevel !== undefined && !isSel ? "bg-[var(--surface-tint)]" : "";

  // --- rendering ------------------------------------------------------------

  const renderNode = (row: TreeRow): React.ReactNode => {
    const node = row.node!;
    const isSel = selected?.kind === "node" && selected.id === node.id;
    const ramp = altitudeRamp(node);
    const Icon = lookupIcon(node.icon) ?? kindIcon(node);
    const active = activeNodeIds.has(node.id);
    const hiddenSyms =
      !filterActive && node.kind === "component" ? hiddenSymbolCount(node.id) : 0;
    const isDrop = dropKey === `node:${node.id}`;
    const marks = markOf.get(node.id) ?? { plan: null, drift: null };
    // The gutter shows this node's OWN mark; when the branch's children are
    // not on screen (collapsed, or hidden by the altitude toggle), a
    // descendant's mark rolls up dimmed so the change is never invisible.
    const ownMark = resolveMark(marks);
    const childrenOnScreen = row.hasChildren && row.isOpen;
    const rolledMark =
      ownMark || childrenOnScreen
        ? null
        : resolveMark(rolledOf.get(node.id) ?? { plan: null, drift: null });

    return (
      <div
        key={`node:${node.id}`}
        data-rk={`node:${node.id}`}
        style={{ paddingLeft: GUTTER + 6 + row.depth * INDENT }}
        onClick={() => onSelectNode(node.id)}
        onDoubleClick={() => row.hasChildren && onToggle(node.id)}
        onContextMenu={(e) => nodeMenu(e, node)}
        {...dragProps(node)}
        {...dropProps(row)}
        className={`${ROW} ${
          isSel
            ? "bg-[var(--accent-soft)] text-[var(--text)]"
            : `text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] ${levelTint(
                (node.parentId ?? null) === activeLevel,
                isSel,
              )}`
        } ${isDrop ? "ring-1 ring-inset ring-[var(--border-strong)] bg-[var(--surface-hover)]" : ""} ${
          concernTally && !concernTally.has(node.id) && !isSel
            ? "opacity-40 transition-opacity"
            : ""
        }`}
      >
        <ChangeGutter mark={ownMark ?? rolledMark} dim={!ownMark && !!rolledMark} />
        {/* Rails are suppressed on the selected row so the ancestor lines pass
            behind the selection band instead of ticking across it. */}
        {!isSel && renderRails(row)}
        <Chevron
          has={row.hasChildren}
          open={row.isOpen}
          sel={isSel}
          hot={hoverRail === node.id}
          onHover={(h) => setHoverRail((cur) => (h ? node.id : cur === node.id ? null : cur))}
          onClick={() => onToggle(node.id)}
        />
        <Icon className={`ml-1.5 h-3.5 w-3.5 shrink-0 ${ICON_COLOR}`} />
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
        {hiddenSyms > 0 && (
          <span
            className="shrink-0 font-mono text-2xs tabular-nums text-[var(--text-ghost)]"
            title={`${hiddenSyms} symbol${hiddenSyms === 1 ? "" : "s"} hidden at this altitude`}
          >
            {hiddenSyms}
          </span>
        )}
        {(() => {
          // Arch tiers only — a symbol's completeness is near-binary noise.
          if (node.kind === "symbol" || node.kind === "person") return null;
          const c = completeness?.[node.id];
          // Cleanliness = completeness: a fully built subtree (and an unmeasured
          // bare box) shows nothing — only outstanding build work earns a pie.
          if (!c || c.total === 0 || c.pct === undefined || c.pct >= 100) return null;
          return (
            <span
              className="shrink-0"
              title={`${c.pct}% of this subtree's claims read through to code`}
            >
              <CompletenessPie c={c} size={12} />
            </span>
          );
        })()}
        {concernTally && concernTally.has(node.id) && (
          <span
            className="shrink-0 rounded bg-[var(--accent-soft)] px-1 font-mono text-2xs tabular-nums text-[var(--accent)]"
            title={`${concernTally.get(node.id)} responsibilit${concernTally.get(node.id) === 1 ? "y" : "ies"} tagged "${concernLens}" in this subtree`}
          >
            {concernTally.get(node.id)}
          </span>
        )}
        {active && (
          <Loader2 className="h-3 w-3 shrink-0 animate-spin text-indigo-500 dark:text-indigo-400" />
        )}
      </div>
    );
  };

  const renderGroup = (row: TreeRow): React.ReactNode => {
    const group = row.group!;
    const isSel = selected?.kind === "group" && selected.id === group.id;
    const gMark = resolveMark({
      plan: entryMark.get(group.id) ?? null,
      drift: groupDrift(group),
    });
    // Collapsed folder: members' marks (own + their subtrees) roll up dimmed.
    const gRolled =
      gMark || row.isOpen
        ? null
        : resolveMark(
            combineMarks(
              group.memberIds.flatMap((m) => [markOf.get(m), rolledOf.get(m)]),
            ),
          );
    // Declared member count, shown (quietly) only when collapsed.
    const memberCount = row.isOpen ? 0 : group.memberIds.length;
    const groupLevel =
      group.parentNodeId ??
      model.nodes.find((n) => n.id === group.memberIds[0])?.parentId ??
      null;

    return (
      <div
        key={`group:${group.id}`}
        data-rk={`group:${group.id}`}
        style={{ paddingLeft: GUTTER + 6 + row.depth * INDENT }}
        onClick={() => onSelectGroup(group.id)}
        onDoubleClick={() => row.hasChildren && onToggle(group.id)}
        onContextMenu={(e) => groupMenu(e, group)}
        {...dropProps(row)}
        className={`${ROW} ${
          isSel
            ? "bg-[var(--accent-soft)] text-[var(--text)]"
            : `text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] ${levelTint(
                groupLevel === activeLevel,
                isSel,
              )}`
        } ${dropKey === `group:${group.id}` ? "ring-1 ring-inset ring-[var(--border-strong)] bg-[var(--surface-hover)]" : ""} ${
          !groupLit(group) && !isSel ? "opacity-40 transition-opacity" : ""
        }`}
      >
        <ChangeGutter mark={gMark ?? gRolled} dim={!gMark && !!gRolled} />
        {!isSel && renderRails(row)}
        <Chevron
          has={row.hasChildren}
          open={row.isOpen}
          sel={isSel}
          hot={hoverRail === group.id}
          onHover={(h) => setHoverRail((cur) => (h ? group.id : cur === group.id ? null : cur))}
          onClick={() => onToggle(group.id)}
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
      </div>
    );
  };

  return (
    <div
      ref={containerRef}
      style={{ width }}
      tabIndex={0}
      onKeyDown={onKeyDown}
      className="relative flex h-full shrink-0 flex-col border-r border-[var(--border)] bg-[var(--surface)] outline-none"
    >
      <div className="flex items-center justify-between px-3 py-2">
        <span className={`${EYEBROW} flex items-baseline gap-1.5`}>
          Model
          {model.nodes.length > 0 && (
            <span className="font-mono text-2xs font-normal normal-case tracking-normal text-[var(--text-ghost)]">
              {model.nodes.length}
            </span>
          )}
        </span>
        <div className="flex items-center gap-0.5">
          <button
            type="button"
            title={
              showSymbols
                ? "Hide symbols — read the tree at architecture altitude"
                : "Show symbols in the tree"
            }
            onClick={toggleSymbols}
            className={`rounded p-0.5 ${
              showSymbols
                ? "bg-[var(--surface-active)] text-[var(--text-secondary)]"
                : "text-[var(--text-muted)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)]"
            }`}
          >
            <Braces className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
      {/* Type-to-filter + lenses. The lenses are the tree-native plan/drift
          views: the same model, narrowed by mark instead of re-projected.
          "Changes" = the plan (A/M/D/R); "Drift" = model↔code mismatch (Q/X). */}
      <div className="flex flex-col gap-1.5 px-2 pb-2">
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
        {/* Same joined-segment idiom as the top bar's Wiki/Map toggle — one
            control language across the chrome: single border, hairline
            dividers, the active cell filled. */}
        <div className="flex items-stretch overflow-hidden rounded-md border border-[var(--border)] divide-x divide-[var(--border)]">
          {(
            [
              { id: "all", label: "All", count: null, countColor: "" },
              { id: "changes", label: "Changes", count: changeCount, countColor: MARK_META.M.color },
              { id: "drift", label: "Drift", count: driftCount, countColor: MARK_META.Q.color },
            ] as const
          ).map((opt) => (
            <button
              key={opt.id}
              type="button"
              title={
                opt.id === "changes"
                  ? "Lens: the plan — added / modified / deleted / relocated since the committed model"
                  : opt.id === "drift"
                    ? "Lens: drift — undescribed (Q) or stale (X) claims where code and model disagree"
                    : "Show the whole model"
              }
              onClick={() => setLens(opt.id)}
              className={`flex flex-1 items-center justify-center gap-1.5 py-1 text-xs transition-colors ${
                lens === opt.id
                  ? "bg-[var(--surface-active)] text-[var(--text)]"
                  : "text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text)]"
              }`}
            >
              {opt.label}
              {opt.count != null && opt.count > 0 && (
                <span className={`font-mono text-2xs tabular-nums ${opt.countColor}`}>{opt.count}</span>
              )}
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
        className="flex-1 overflow-y-auto pb-4"
        style={{ scrollbarGutter: "stable" }}
        onContextMenu={openRootMenu}
      >
        {model.nodes.length === 0 ? (
          <div className="flex flex-col items-start gap-3 px-4 py-6 text-xs text-[var(--text-muted)]">
            <span>Empty model. Generate from the codebase, or start one here:</span>
            {editor && (
              <div className="flex gap-2">
                <button
                  type="button"
                  onClick={() => addRoot("system", false)}
                  className={BTN}
                >
                  <Plus className="h-3 w-3" /> System
                </button>
                <button
                  type="button"
                  onClick={() => addRoot("person", false)}
                  className={BTN}
                >
                  <Plus className="h-3 w-3" /> Person
                </button>
              </div>
            )}
          </div>
        ) : rows.length === 0 && filterActive ? (
          <div className="px-4 py-6 text-xs text-[var(--text-muted)]">
            {q !== ""
              ? "No matches."
              : lens === "changes"
                ? "No pending changes — the plan matches the committed model."
                : "No drift — the model and code agree."}
          </div>
        ) : (
          rows.map((row) => (row.kind === "node" ? renderNode(row) : renderGroup(row)))
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
