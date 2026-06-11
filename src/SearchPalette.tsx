/**
 * Jump-to-node palette (Ctrl/Cmd+K). Filters every node and group by name,
 * shows the ancestor chain so same-named symbols are tellable apart, and opens
 * the picked page. Keyboard: arrows to move, Enter to open, Esc to close.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { FolderOpen, Search } from "lucide-react";
import type { ScryModel } from "./viewmodel";
import { kindIcon, typeTag } from "./kindIcon";
import { lookupIcon } from "./IconPicker";
import { effectiveNodeStatus } from "./rollup";
import { StatusTag } from "./pagekit";

const MAX_RESULTS = 50;

interface Hit {
  key: string;
  kind: "node" | "group";
  id: string;
  name: string;
  /** Root-first ancestor names, for disambiguation. */
  path: string[];
  row: React.ReactNode;
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
      if (el && !el.contains(e.target as Node)) onClose();
    };
    window.addEventListener("pointerdown", onDown, true);
    return () => window.removeEventListener("pointerdown", onDown, true);
  }, [onClose]);

  const hits = useMemo<Hit[]>(() => {
    const q = query.trim().toLowerCase();
    const byId = new Map(model.nodes.map((n) => [n.id, n]));
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
      const name = n.name || "Untitled";
      if (q && !name.toLowerCase().includes(q)) continue;
      const Icon = lookupIcon(n.icon) ?? kindIcon(n);
      out.push({
        key: `n:${n.id}`,
        kind: "node",
        id: n.id,
        name,
        path: chain(n.parentId),
        row: (
          <>
            <Icon className="h-3.5 w-3.5 shrink-0 text-[var(--text-muted)]" />
            <span className="truncate text-sm">{name}</span>
            <span className="shrink-0 text-2xs text-[var(--text-muted)]">{typeTag(n).type}</span>
            <span className="flex-1" />
            <StatusTag status={effectiveNodeStatus(n)} />
          </>
        ),
      });
      if (out.length >= MAX_RESULTS) return out;
    }
    for (const g of model.groups) {
      const name = g.name || "Group";
      if (q && !name.toLowerCase().includes(q)) continue;
      const container =
        g.parentNodeId ?? byId.get(g.memberIds[0] ?? "")?.parentId ?? undefined;
      out.push({
        key: `g:${g.id}`,
        kind: "group",
        id: g.id,
        name,
        path: container ? chain(container) : [],
        row: (
          <>
            <FolderOpen className="h-3.5 w-3.5 shrink-0 text-[var(--text-muted)]" />
            <span className="truncate text-sm italic">{name}</span>
            <span className="shrink-0 text-2xs text-[var(--text-muted)]">Group</span>
          </>
        ),
      });
      if (out.length >= MAX_RESULTS) break;
    }
    return out;
  }, [model, query]);

  useEffect(() => {
    setActive(0);
  }, [query]);

  // Keep the active row in view while arrowing through the list.
  useEffect(() => {
    listRef.current
      ?.children[active]?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const pick = (hit: Hit) => {
    if (hit.kind === "node") onSelectNode(hit.id);
    else onSelectGroup(hit.id);
    onClose();
  };

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
            placeholder="Jump to a node or group…"
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
        <ul ref={listRef} className="overflow-y-auto py-1">
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
                className={`flex w-full flex-col gap-0.5 px-3 py-1.5 text-left cursor-pointer ${
                  i === active ? "bg-[var(--surface-hover)]" : ""
                }`}
              >
                <span className="flex items-center gap-2 text-[var(--text-secondary)]">
                  {hit.row}
                </span>
                {hit.path.length > 0 && (
                  <span className="truncate pl-[22px] text-2xs text-[var(--text-muted)]">
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
