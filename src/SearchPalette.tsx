/**
 * Global search palette (Ctrl/Cmd+K). Unlike the left tree's name-only filter,
 * this searches every node's authored content — name, technology, description,
 * responsibilities and their directives, schema properties — plus group prose
 * and link labels, and surfaces WHERE each match landed with a highlighted
 * snippet. Name matches rank first, then content by field. Keyboard: arrows to
 * move, Enter to open, Esc to close.
 */

import { useEffect, useMemo, useRef, useState, type ComponentType } from "react";
import { createPortal } from "react-dom";
import { FolderOpen, Link2, Search, type LucideProps } from "lucide-react";
import type { ScryModel, Node, Group } from "./viewmodel";
import { kindIcon, typeTag } from "./kindIcon";
import { stripMarkup } from "./markup";
import { lookupIcon } from "./IconPicker";

const MAX_RESULTS = 50;

interface Hit {
  key: string;
  kind: "node" | "group" | "link";
  /** Target to open on select — node id (links open their source node). */
  id: string;
  name: string;
  /** Root-first ancestor names (or the endpoints, for a link), for context. */
  path: string[];
  Icon: ComponentType<LucideProps>;
  typeLabel: string;
  italic: boolean;
  /** Where the match landed; null when it matched the name itself. */
  field: string | null;
  /** The field text the match came from — the snippet source. */
  matchText: string;
  /** Lower = higher in the list. */
  rank: number;
}

/** One searchable field of a node/group, in ascending rank (best first). */
interface Field {
  label: string | null;
  rank: number;
  text: string;
}

function nodeFields(n: Node): Field[] {
  const f: Field[] = [{ label: null, rank: 0, text: n.name || "" }];
  if (n.technology) f.push({ label: "Technology", rank: 1, text: n.technology });
  if (n.description) f.push({ label: "Description", rank: 2, text: n.description });
  for (const r of n.responsibilities ?? []) {
    if (r.statement) f.push({ label: "Responsibility", rank: 3, text: stripMarkup(r.statement) });
    for (const d of r.directives ?? [])
      if (d) f.push({ label: "Directive", rank: 4, text: d });
  }
  for (const p of n.properties ?? []) {
    if (p.label) f.push({ label: "Property", rank: 5, text: p.label });
    if (p.description)
      f.push({ label: "Property", rank: 5, text: `${p.label}: ${p.description}` });
  }
  return f;
}

function groupFields(g: Group): Field[] {
  const f: Field[] = [{ label: null, rank: 0, text: g.name || "Group" }];
  if (g.description) f.push({ label: "Description", rank: 2, text: g.description });
  for (const r of g.responsibilities ?? []) {
    if (r.statement) f.push({ label: "Responsibility", rank: 3, text: stripMarkup(r.statement) });
    for (const d of r.directives ?? [])
      if (d) f.push({ label: "Directive", rank: 4, text: d });
  }
  return f;
}

/** First (lowest-rank) field whose text contains the query, or null. */
function bestMatch(fields: Field[], q: string): Field | null {
  for (const f of fields) if (f.text.toLowerCase().includes(q)) return f;
  return null;
}

/** Render `text` with every case-insensitive occurrence of `q` marked. */
function Highlighted({ text, q }: { text: string; q: string }) {
  if (!q) return <>{text}</>;
  const lower = text.toLowerCase();
  const out: React.ReactNode[] = [];
  let from = 0;
  let key = 0;
  for (;;) {
    const idx = lower.indexOf(q, from);
    if (idx < 0) {
      out.push(text.slice(from));
      break;
    }
    if (idx > from) out.push(text.slice(from, idx));
    out.push(
      <mark
        key={key++}
        className="rounded bg-blue-500/30 text-[var(--text)]"
      >
        {text.slice(idx, idx + q.length)}
      </mark>,
    );
    from = idx + q.length;
  }
  return <>{out}</>;
}

/** A window of `text` centred on the first match, with leading/trailing
 *  ellipses when it's clipped. */
function Snippet({ text, q }: { text: string; q: string }) {
  const idx = text.toLowerCase().indexOf(q);
  const start = idx < 0 ? 0 : Math.max(0, idx - 32);
  const end = idx < 0 ? text.length : Math.min(text.length, idx + q.length + 64);
  const slice = text.slice(start, end);
  return (
    <>
      {start > 0 && "…"}
      <Highlighted text={slice} q={q} />
      {end < text.length && "…"}
    </>
  );
}

export function SearchPalette({
  model,
  onSelectNode,
  onSelectGroup,
  onClose,
}: {
  model: ScryModel;
  onSelectNode: (id: string) => void;
  onSelectGroup: (id: string) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const listRef = useRef<HTMLUListElement | null>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      const el = containerRef.current;
      if (el && !el.contains(e.target as globalThis.Node)) onClose();
    };
    window.addEventListener("pointerdown", onDown, true);
    return () => window.removeEventListener("pointerdown", onDown, true);
  }, [onClose]);

  const hits = useMemo<Hit[]>(() => {
    const q = query.trim().toLowerCase();
    const byId = new Map(model.nodes.map((n) => [n.id, n]));
    const nameOf = (id: string) => byId.get(id)?.name || "Untitled";
    const chain = (nodeId: string | undefined): string[] => {
      const out: string[] = [];
      const seen = new Set<string>();
      let cur = nodeId ? byId.get(nodeId) : undefined;
      while (cur && !seen.has(cur.id)) {
        seen.add(cur.id);
        out.unshift(cur.name || "Untitled");
        cur = cur.parentId ? byId.get(cur.parentId) : undefined;
      }
      return out;
    };

    const out: Hit[] = [];
    for (const n of model.nodes) {
      // Empty query: name-only listing, like a jump palette.
      const match = q ? bestMatch(nodeFields(n), q) : { label: null, rank: 0, text: n.name || "" };
      if (!match) continue;
      out.push({
        key: `n:${n.id}`,
        kind: "node",
        id: n.id,
        name: n.name || "Untitled",
        path: chain(n.parentId),
        Icon: lookupIcon(n.icon) ?? kindIcon(n),
        typeLabel: typeTag(n).type,
        italic: false,
        field: match.label,
        matchText: match.text,
        rank: match.rank,
      });
    }
    for (const g of model.groups) {
      const match = q ? bestMatch(groupFields(g), q) : { label: null, rank: 0, text: g.name || "Group" };
      if (!match) continue;
      const container =
        g.parentNodeId ?? byId.get(g.memberIds[0] ?? "")?.parentId ?? undefined;
      out.push({
        key: `g:${g.id}`,
        kind: "group",
        id: g.id,
        name: g.name || "Group",
        path: container ? chain(container) : [],
        Icon: FolderOpen,
        typeLabel: "Group",
        italic: true,
        field: match.label,
        matchText: match.text,
        rank: match.rank,
      });
    }
    // Links carry no page of their own — a label hit opens the source node.
    if (q) {
      for (const l of model.links) {
        if (!l.label || !l.label.toLowerCase().includes(q)) continue;
        out.push({
          key: `l:${l.id}`,
          kind: "link",
          id: l.src,
          name: l.label,
          path: [`${nameOf(l.src)} → ${nameOf(l.dst)}`],
          Icon: Link2,
          typeLabel: "Link",
          italic: false,
          // The title IS the label — the type tag + endpoints below say the
          // rest, so no duplicate snippet.
          field: null,
          matchText: l.label,
          rank: 6,
        });
      }
    }

    // Stable sort by rank — name matches first, then content by field.
    out.sort((a, b) => a.rank - b.rank);
    return out.slice(0, MAX_RESULTS);
  }, [model, query]);

  useEffect(() => {
    setActive(0);
  }, [query]);

  // Keep the active row in view while arrowing through the list.
  useEffect(() => {
    listRef.current?.children[active]?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const pick = (hit: Hit) => {
    if (hit.kind === "group") onSelectGroup(hit.id);
    else onSelectNode(hit.id);
    onClose();
  };

  const q = query.trim().toLowerCase();

  return createPortal(
    <div className="fixed inset-0 z-[1000] flex justify-center bg-black/30 pt-[12vh]">
      <div
        ref={containerRef}
        className="flex h-fit max-h-[60vh] w-[480px] max-w-[90vw] flex-col overflow-hidden rounded-lg border border-[var(--border-strong)] bg-[var(--surface-raised)] shadow-2xl"
      >
        <div className="flex items-center gap-2 border-b border-[var(--border-subtle)] px-3">
          <Search className="h-3.5 w-3.5 shrink-0 text-[var(--text-muted)]" />
          <input
            ref={inputRef}
            type="text"
            placeholder="Search names, descriptions, responsibilities…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.preventDefault();
                onClose();
              } else if (e.key === "ArrowDown") {
                e.preventDefault();
                setActive((a) => Math.min(hits.length - 1, a + 1));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setActive((a) => Math.max(0, a - 1));
              } else if (e.key === "Enter") {
                e.preventDefault();
                const hit = hits[active];
                if (hit) pick(hit);
              }
            }}
            className="min-w-0 flex-1 bg-transparent py-2.5 text-sm outline-none placeholder:text-[var(--text-ghost)]"
            style={{ color: "var(--text)" }}
          />
        </div>
        <ul ref={listRef} className="overflow-y-auto overscroll-contain py-1">
          {hits.length === 0 && (
            <li className="px-3 py-2 text-xs italic text-[var(--text-muted)]">
              No matches
            </li>
          )}
          {hits.map((hit, i) => (
            <li key={hit.key}>
              <button
                type="button"
                onPointerEnter={() => setActive(i)}
                onClick={() => pick(hit)}
                className={`flex w-full flex-col gap-0.5 px-3 py-1.5 text-left ${
                  i === active ? "bg-[var(--accent-soft)]" : ""
                }`}
              >
                <span className="flex items-center gap-2 text-[var(--text-secondary)]">
                  <hit.Icon className="h-3.5 w-3.5 shrink-0 text-[var(--text-muted)]" />
                  <span className={`truncate text-sm ${hit.italic ? "italic" : ""}`}>
                    <Highlighted text={hit.name} q={q} />
                  </span>
                  <span className="ml-auto shrink-0 text-xs text-[var(--text-muted)]">
                    {hit.typeLabel}
                  </span>
                </span>
                {/* Content match: show which field hit and a snippet of it. The
                    name match needs no snippet — it's already highlighted above. */}
                {hit.field && (
                  <span className="flex gap-1.5 truncate pl-[22px] text-xs text-[var(--text-muted)]">
                    <span className="shrink-0 font-medium uppercase tracking-[0.07em] text-[var(--text-tertiary)]">
                      {hit.field}
                    </span>
                    <span className="truncate text-[var(--text-secondary)]">
                      <Snippet text={hit.matchText} q={q} />
                    </span>
                  </span>
                )}
                {hit.path.length > 0 && (
                  <span className="truncate pl-[22px] text-xs text-[var(--text-muted)]">
                    {hit.path.join(" › ")}
                  </span>
                )}
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>,
    document.body,
  );
}
