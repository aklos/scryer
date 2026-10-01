/**
 * Right-click context menu. Anchored to a screen position; filtered search;
 * arrow-key navigation; Enter / click runs the item; Esc or outside-click
 * dismisses.
 */

import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";

export interface ContextMenuItem {
  id: string;
  label: string;
  hint?: string;
  onSelect: () => void;
}

export interface ContextMenuProps {
  /** Screen-space anchor point (typically e.clientX/clientY). */
  x: number;
  y: number;
  items: ContextMenuItem[];
  placeholder?: string;
  /** Show the type-to-filter input. Defaults to true — useful for the long
   *  right-click menus, pointless on a short fixed list (e.g. the top-bar
   *  logo dropdown), so callers can opt out. */
  searchable?: boolean;
  onClose: () => void;
}

export function ContextMenu({
  x,
  y,
  items,
  placeholder = "Search…",
  searchable = true,
  onClose,
}: ContextMenuProps) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!searchable || !q) return items;
    return items.filter((it) => it.label.toLowerCase().includes(q));
  }, [items, query, searchable]);

  useEffect(() => {
    setActive(0);
  }, [query]);

  // Focus the input when searchable; otherwise focus the container so
  // arrow-key navigation and Enter still work on the bare list.
  useLayoutEffect(() => {
    if (searchable) inputRef.current?.focus();
    else containerRef.current?.focus();
  }, [searchable]);

  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      const el = containerRef.current;
      if (el && !el.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    // Defer attach so the right-click that opened us doesn't immediately close.
    const t = setTimeout(() => {
      window.addEventListener("pointerdown", onDown, true);
    }, 0);
    window.addEventListener("keydown", onKey);
    return () => {
      clearTimeout(t);
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  // Clamp to viewport.
  const W = 240;
  const H = 280;
  const left = Math.min(x, window.innerWidth - W - 8);
  const top = Math.min(y, window.innerHeight - H - 8);

  const onNavKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => Math.min(filtered.length - 1, a + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(0, a - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const it = filtered[active];
      if (it) {
        it.onSelect();
        onClose();
      }
    }
  };

  return createPortal(
    <div
      ref={containerRef}
      data-no-pickup
      tabIndex={-1}
      onKeyDown={searchable ? undefined : onNavKey}
      onPointerDown={(e) => e.stopPropagation()}
      onWheel={(e) => e.stopPropagation()}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
      style={{
        position: "fixed",
        top,
        left,
        width: W,
        zIndex: 1200,
      }}
      className="rounded border border-[var(--border-overlay)] bg-[var(--surface-overlay)] backdrop-blur-md shadow-xl outline-none"
    >
      {searchable && (
        <input
          ref={inputRef}
          type="text"
          placeholder={placeholder}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={onNavKey}
          className="w-full bg-transparent px-3 py-2 text-xs outline-none border-b border-[var(--border-subtle)] placeholder:text-[var(--text-ghost)]"
          style={{ color: "var(--text)" }}
        />
      )}
      <ul className="max-h-60 overflow-y-auto overscroll-contain py-1">
        {filtered.length === 0 && (
          <li className="px-3 py-2 text-xs text-[var(--text-ghost)] italic">
            No matches
          </li>
        )}
        {filtered.map((it, i) => (
          <li
            key={it.id}
            onPointerEnter={() => setActive(i)}
            onClick={(e) => {
              e.stopPropagation();
              it.onSelect();
              onClose();
            }}
            className={`flex items-center justify-between gap-3 px-3 py-1.5 text-xs cursor-pointer ${
              i === active
                ? "bg-[var(--surface-hover)] text-[var(--text)]"
                : "text-[var(--text-secondary)]"
            }`}
          >
            <span className="truncate">{it.label}</span>
            {it.hint && (
              <span className="shrink-0 text-2xs text-[var(--text-ghost)]">
                {it.hint}
              </span>
            )}
          </li>
        ))}
      </ul>
    </div>,
    document.body,
  );
}
