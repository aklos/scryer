import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { C4Node, C4Edge, C4Kind, Contract } from "./types";

// Track user collapse intent across remounts so panels stay collapsed after navigation
let _guideCollapsed = true;
let _flowGuideCollapsed = false;

/** Expand the guide panel (e.g. when creating a new blank model in the UI). */
export function expandGuidePanel() { _guideCollapsed = false; }

interface GuidePanelProps {
  nodes: C4Node[];
  edges: C4Edge[];
  visibleNodes: C4Node[];
  currentParentId: string | undefined;
  parentKind: C4Kind | undefined;
  parentName?: string;
}

type Level = "context" | "container" | "component" | "operation";

interface CheckItem {
  label: string;
  done: boolean;
}

function levelGuidance(level: Level, parentName?: string): string {
  switch (level) {
    case "context":
      return "**System Context** — Who uses your software, and what systems are involved? Add the people (users, admins) and the systems they interact with. External systems (payment providers, email services) go here too.";
    case "container":
      return `**Container Diagram** — What are the major technical building blocks inside *${parentName ?? "this system"}*? Containers are distinct runtime responsibilities: a web app, an API, a database, a message queue. Multiple containers can share a deployment unit.`;
    case "component":
      return `**Component Diagram** — What are the key structural pieces inside *${parentName ?? "this container"}*? Components map to code: a module, a service class, a package.`;
    case "operation":
      return `**Code Level** — What does *${parentName ?? "this component"}* actually do? Define the operations (functions, handlers) and processes (workflows) that implement this component.`;
  }
}

function detectLevel(parentKind: C4Kind | undefined): Level | null {
  if (parentKind === undefined) return "context";
  if (parentKind === "system") return "container";
  if (parentKind === "container") return "component";
  if (parentKind === "component") return "operation";
  return null;
}

function computeChecklist(
  level: Level,
  childNodes: C4Node[],
  levelEdges: C4Edge[],
  refNodes: C4Node[],
  allEdges: C4Edge[],
  groupBoxIds: Set<string>,
): CheckItem[] {
  const items: CheckItem[] = [];

  switch (level) {
    case "context": {
      const persons = childNodes.filter((n) => n.data.kind === "person");
      const internalSystems = childNodes.filter((n) => n.data.kind === "system" && !n.data.external);
      items.push({ label: "Exactly 1 internal system", done: internalSystems.length === 1 });
      items.push({ label: "At least 1 person", done: persons.length >= 1 });
      if (childNodes.length >= 2) {
        items.push({ label: "Relationships between nodes", done: levelEdges.length >= 1 });
      }
      break;
    }
    case "container": {
      const containers = childNodes.filter((n) => n.data.kind === "container");
      items.push({ label: "At least 1 container", done: containers.length >= 1 });
      if (containers.length >= 1) {
        const everyTech = containers.every((n) => (n.data.technology ?? "").trim().length > 0);
        items.push({ label: "Technology on every container", done: everyTech });
      }
      if (containers.length >= 2) {
        items.push({ label: "Containers connected", done: levelEdges.length >= 1 });
      }
      if (refNodes.length > 0) {
        const childIds = new Set(childNodes.map((n) => n.id));
        // Also treat group box nodes as valid endpoints — edges to/from a group
        // represent connections to/from the group's members (which are children)
        const localIds = new Set([...childIds, ...groupBoxIds]);
        for (const ref of refNodes) {
          const name = ref.data.name ?? ref.id;
          const rels = ref.data._relationships ?? [];
          const expectedDir = rels[0]?.direction ?? "in";
          const hasCorrectEdge = allEdges.some((e) => {
            if (expectedDir === "in") {
              return e.source === ref.id && localIds.has(e.target);
            } else {
              return e.target === ref.id && localIds.has(e.source);
            }
          });
          if (hasCorrectEdge) {
            items.push({ label: `${name} connected`, done: true });
          } else {
            const hasWrongEdge = allEdges.some((e) =>
              (e.source === ref.id || e.target === ref.id) &&
              (localIds.has(e.source) || localIds.has(e.target)),
            );
            const hint = hasWrongEdge
              ? (expectedDir === "in" ? `${name} → container` : `container → ${name}`)
              : `${name} not connected`;
            items.push({ label: hint, done: false });
          }
        }
      }
      break;
    }
    case "component": {
      const components = childNodes.filter((n) => n.data.kind === "component");
      items.push({ label: "At least 1 component", done: components.length >= 1 });
      if (components.length >= 2) {
        items.push({ label: "Components connected", done: levelEdges.length >= 1 });
      }
      if (refNodes.length > 0) {
        const childIds = new Set(childNodes.map((n) => n.id));
        const localIds = new Set([...childIds, ...groupBoxIds]);
        for (const ref of refNodes) {
          const name = ref.data.name ?? ref.id;
          const rels = ref.data._relationships ?? [];
          const expectedDir = rels[0]?.direction ?? "in";
          const hasCorrectEdge = allEdges.some((e) => {
            if (expectedDir === "in") {
              return e.source === ref.id && localIds.has(e.target);
            } else {
              return e.target === ref.id && localIds.has(e.source);
            }
          });
          if (hasCorrectEdge) {
            items.push({ label: `${name} connected`, done: true });
          } else {
            const hasWrongEdge = allEdges.some((e) =>
              (e.source === ref.id || e.target === ref.id) &&
              (localIds.has(e.source) || localIds.has(e.target)),
            );
            const hint = hasWrongEdge
              ? (expectedDir === "in" ? `${name} → component` : `component → ${name}`)
              : `${name} not connected`;
            items.push({ label: hint, done: false });
          }
        }
      }
      break;
    }
    case "operation": {
      const operations = childNodes.filter((n) => n.data.kind === "operation");
      const models = childNodes.filter((n) => n.data.kind === "model");
      const processes = childNodes.filter((n) => n.data.kind === "process");
      // Only check what's actually present
      if (operations.length > 0) {
        const everyDesc = operations.every((n) => (n.data.description ?? "").trim().length > 0);
        items.push({ label: "Description on every operation", done: everyDesc });
      }
      if (models.length > 0) {
        const everyHasProps = models.every((n) => (n.data.properties ?? []).length > 0);
        items.push({ label: "Properties on every model", done: everyHasProps });
      }
      if (processes.length > 0) {
        const everyDesc = processes.every((n) => (n.data.description ?? "").trim().length > 0);
        items.push({ label: "Description on every process", done: everyDesc });
      }
      if (childNodes.length === 0) {
        items.push({ label: "Add operations, models, or processes", done: false });
      }
      break;
    }
  }

  return items;
}

/** Renders simple markdown bold and italic inline. */
function renderGuidance(text: string) {
  const parts: (string | React.JSX.Element)[] = [];
  // Match **bold** and *italic* segments
  const regex = /\*\*(.+?)\*\*|\*(.+?)\*/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;
  let key = 0;
  while ((match = regex.exec(text)) !== null) {
    if (match.index > lastIndex) {
      parts.push(text.slice(lastIndex, match.index));
    }
    if (match[1] !== undefined) {
      parts.push(<strong key={key++}>{match[1]}</strong>);
    } else if (match[2] !== undefined) {
      parts.push(<em key={key++}>{match[2]}</em>);
    }
    lastIndex = regex.lastIndex;
  }
  if (lastIndex < text.length) {
    parts.push(text.slice(lastIndex));
  }
  return parts;
}

export function GuidePanel({ nodes, edges, visibleNodes, currentParentId, parentKind, parentName }: GuidePanelProps) {
  const [collapsed, setCollapsed] = useState(_guideCollapsed);
  const toggleCollapsed = useCallback(() => {
    setCollapsed((c) => { _guideCollapsed = !c; return !c; });
  }, []);

  const level = detectLevel(parentKind);

  const { childNodes, levelEdges, refNodes, groupBoxIds } = useMemo(() => {
    const children = nodes.filter(
      (n) => (n.parentId ?? undefined) === currentParentId,
    );
    const childIds = new Set(children.map((n) => n.id));
    const le = edges.filter((e) => childIds.has(e.source) && childIds.has(e.target));
    // Reference nodes: synthetic nodes with _reference flag on the visible canvas
    const refs = visibleNodes.filter((n) => n.data._reference);
    // Group box IDs: synthetic groupBox nodes visible on the canvas
    const gbIds = new Set(visibleNodes.filter((n) => n.type === "groupBox").map((n) => n.id));
    return { childNodes: children, levelEdges: le, refNodes: refs, groupBoxIds: gbIds };
  }, [nodes, edges, currentParentId, visibleNodes]);

  const checklist = useMemo(() => {
    if (!level) return [];
    return computeChecklist(level, childNodes, levelEdges, refNodes, edges, groupBoxIds);
  }, [level, childNodes, levelEdges, refNodes, edges, groupBoxIds]);

  if (nodes.length === 0 || !level) return null;

  const done = checklist.filter((item) => item.done).length;
  const total = checklist.length;
  const allDone = done === total;

  return (
    <div className="w-56 rounded-lg border border-zinc-200/80 bg-white/80 shadow-sm backdrop-blur-sm dark:border-zinc-700/80 dark:bg-zinc-900/80">
      {/* Header */}
      <button
        type="button"
        className="flex w-full items-center gap-1.5 px-3 py-2 text-left cursor-pointer"
        onClick={toggleCollapsed}
      >
        <svg
          className={`h-3 w-3 shrink-0 text-zinc-400 transition-transform ${collapsed ? "-rotate-90" : ""}`}
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
        <span className="text-xs font-medium text-zinc-600 dark:text-zinc-300">Guide</span>
        <span
          className={`ml-auto rounded-full px-1.5 py-0.5 text-[10px] font-medium leading-none ${
            allDone
              ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-400"
              : "bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-400"
          }`}
        >
          {done}/{total}
        </span>
      </button>

      {/* Body */}
      {!collapsed && (
        <div className="border-t border-zinc-100 px-3 pb-2.5 pt-2 dark:border-zinc-700">
          <p className="mb-2 text-[11px] text-zinc-500 dark:text-zinc-500 leading-snug">
            {renderGuidance(levelGuidance(level, parentName))}
          </p>
          <ul className="flex flex-col gap-1">
            {checklist.map((item) => (
              <li
                key={item.label}
                className={`flex items-start gap-1.5 text-[11px] leading-snug ${
                  item.done
                    ? "text-zinc-500 dark:text-zinc-500"
                    : "text-zinc-700 dark:text-zinc-200"
                }`}
              >
                <span className={`shrink-0 ${item.done ? "text-emerald-500" : "text-red-400"}`}>
                  {item.done ? "\u2713" : "\u2717"}
                </span>
                {item.label}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

/* ── Flow Guide floating panel ─────────────────────────────── */

interface FlowGuidePanelProps {
  stepCount: number;
  transitionCount: number;
  stepsWithDescription: number;
}

function computeFlowChecklist(stepCount: number, transitionCount: number, stepsWithDescription: number): CheckItem[] {
  const items: CheckItem[] = [];
  items.push({ label: "At least 2 steps", done: stepCount >= 2 });
  if (stepCount >= 2) {
    items.push({ label: "Steps connected", done: transitionCount >= 1 });
  }
  if (stepCount >= 1) {
    items.push({ label: "Description on every step", done: stepsWithDescription === stepCount });
  }
  return items;
}

export function FlowGuidePanel({ stepCount, transitionCount, stepsWithDescription }: FlowGuidePanelProps) {
  const [collapsed, setCollapsed] = useState(_flowGuideCollapsed);
  const toggleCollapsed = useCallback(() => {
    setCollapsed((c) => { _flowGuideCollapsed = !c; return !c; });
  }, []);

  const checklist = useMemo(
    () => computeFlowChecklist(stepCount, transitionCount, stepsWithDescription),
    [stepCount, transitionCount, stepsWithDescription],
  );

  const done = checklist.filter((item) => item.done).length;
  const total = checklist.length;
  const allDone = done === total;

  return (
    <div className="w-56 rounded-lg border border-zinc-200/80 bg-white/80 shadow-sm backdrop-blur-sm dark:border-zinc-700/80 dark:bg-zinc-900/80">
      <button
        type="button"
        className="flex w-full items-center gap-1.5 px-3 py-2 text-left cursor-pointer"
        onClick={toggleCollapsed}
      >
        <svg
          className={`h-3 w-3 shrink-0 text-zinc-400 transition-transform ${collapsed ? "-rotate-90" : ""}`}
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
        <span className="text-xs font-medium text-zinc-600 dark:text-zinc-300">Guide</span>
        <span
          className={`ml-auto rounded-full px-1.5 py-0.5 text-[10px] font-medium leading-none ${
            allDone
              ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-400"
              : "bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-400"
          }`}
        >
          {done}/{total}
        </span>
      </button>

      {!collapsed && (
        <div className="border-t border-zinc-100 px-3 pb-2.5 pt-2 dark:border-zinc-700">
          <p className="mb-2 text-[11px] text-zinc-500 dark:text-zinc-500 leading-snug">
            {renderGuidance("**Flow** — What happens during this flow? Each step is one meaningful interaction (a request, a response, a state change) — not a UI gesture like \"clicks button\".")}
          </p>
          <ul className="flex flex-col gap-1">
            {checklist.map((item) => (
              <li
                key={item.label}
                className={`flex items-start gap-1.5 text-[11px] leading-snug ${
                  item.done
                    ? "text-zinc-500 dark:text-zinc-500"
                    : "text-zinc-700 dark:text-zinc-200"
                }`}
              >
                <span className={`shrink-0 ${item.done ? "text-emerald-500" : "text-red-400"}`}>
                  {item.done ? "\u2713" : "\u2717"}
                </span>
                {item.label}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

/* ── Model Contract floating panel ─────────────────────────────── */


function ContractBulletItem({ value, focused, placeholder, onCommit, onFocus, onBlur, onEnter, onDeleteEmpty, onRemove }: {
  value: string; focused: boolean; placeholder?: string;
  onCommit: (value: string) => void; onFocus: () => void; onBlur: () => void;
  onEnter: () => void; onDeleteEmpty: () => void; onRemove: () => void;
}) {
  const spanRef = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (!focused && spanRef.current && spanRef.current.textContent !== value) {
      spanRef.current.textContent = value;
    }
  }, [value, focused]);

  useEffect(() => {
    if (focused && spanRef.current && document.activeElement !== spanRef.current) {
      const el = spanRef.current;
      el.focus();
      const range = document.createRange();
      range.selectNodeContents(el);
      range.collapse(false);
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
    }
  }, [focused]);

  return (
    <div className={`flex items-baseline gap-1 group min-h-[20px] -mx-1 px-1 mb-1.5 last:mb-0 rounded ${focused ? "bg-zinc-200/60 dark:bg-zinc-700/60" : ""}`}>
      <span className="text-zinc-300 dark:text-zinc-600 text-[10px] select-none shrink-0">&bull;</span>
      <span
        ref={spanRef}
        contentEditable
        suppressContentEditableWarning
        className="flex-1 text-xs text-zinc-700 dark:text-zinc-200 caret-current outline-none leading-relaxed empty:before:content-[attr(data-placeholder)] empty:before:text-zinc-400 dark:empty:before:text-zinc-500"
        data-placeholder={placeholder}
        onFocus={onFocus}
        onBlur={(e) => {
          onCommit((e.target as HTMLSpanElement).textContent ?? "");
          onBlur();
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            onCommit((e.target as HTMLSpanElement).textContent ?? "");
            onEnter();
          } else if (e.key === "Backspace" && (e.target as HTMLSpanElement).textContent === "") {
            e.preventDefault();
            onDeleteEmpty();
          }
        }}
      />
      <button
        type="button"
        className="shrink-0 text-[10px] text-zinc-300 hover:text-red-400 dark:text-zinc-600 dark:hover:text-red-400 cursor-pointer opacity-0 group-hover:opacity-100"
        onClick={onRemove}
      >
        &times;
      </button>
    </div>
  );
}

function ContractBulletList({ items, onChange, placeholder }: { items: string[]; onChange: (items: string[]) => void; placeholder?: string }) {
  const [focusedIndex, setFocusedIndex] = useState<number | null>(null);

  return (
    <div className="flex flex-col">
      {items.map((item, i) => (
        <ContractBulletItem
          key={i}
          value={item}
          focused={focusedIndex === i}
          placeholder={placeholder}
          onCommit={(v) => { if (v !== item) onChange(items.map((x, j) => j === i ? v : x)); }}
          onFocus={() => setFocusedIndex(i)}
          onBlur={() => setFocusedIndex(null)}
          onEnter={() => onChange([...items.slice(0, i + 1), "", ...items.slice(i + 1)])}
          onDeleteEmpty={() => onChange(items.filter((_, j) => j !== i))}
          onRemove={() => onChange(items.filter((_, j) => j !== i))}
        />
      ))}
      {/* Ghost bullet — becomes a real item when typed into */}
      <div className="flex items-baseline gap-1 min-h-[20px] -mx-1 px-1 rounded opacity-40 hover:opacity-70 focus-within:opacity-100 transition-opacity">
        <span className="text-zinc-300 dark:text-zinc-600 text-[10px] select-none shrink-0">&bull;</span>
        <span
          contentEditable
          suppressContentEditableWarning
          className="flex-1 text-xs text-zinc-700 dark:text-zinc-200 caret-current outline-none leading-relaxed empty:before:content-[attr(data-placeholder)] empty:before:text-zinc-400 dark:empty:before:text-zinc-500"
          data-placeholder={placeholder ?? "Add..."}
          onInput={(e) => {
            const text = (e.target as HTMLSpanElement).textContent ?? "";
            if (text) {
              onChange([...items, text]);
              (e.target as HTMLSpanElement).textContent = "";
              setFocusedIndex(items.length);
            }
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              const text = (e.target as HTMLSpanElement).textContent ?? "";
              if (text) {
                onChange([...items, text]);
                (e.target as HTMLSpanElement).textContent = "";
                setFocusedIndex(items.length);
              }
            }
          }}
        />
      </div>
    </div>
  );
}

function ContractSection({ label, items, onChange, placeholder }: { label: string; items: string[]; onChange: (items: string[]) => void; placeholder?: string }) {
  return (
    <div>
      <span className="text-[10px] font-medium uppercase tracking-wider text-zinc-500 dark:text-zinc-500">{label}</span>
      <div className="mt-1">
        <ContractBulletList items={items} onChange={onChange} placeholder={placeholder} />
      </div>
    </div>
  );
}

export function ContractPanel({ contract, onChange, hasNodes, isRootLevel }: {
  contract: Contract;
  onChange: (contract: Contract) => void;
  hasNodes: boolean;
  isRootLevel: boolean;
}) {
  const [collapsed, setCollapsed] = useState(true);

  if (!hasNodes || !isRootLevel) return null;

  const hasContent = contract.expect.length > 0 || contract.ask.length > 0 || contract.never.length > 0;

  return (
    <div className="w-56 rounded-lg border border-zinc-200/80 bg-white/80 shadow-sm backdrop-blur-sm dark:border-zinc-700/80 dark:bg-zinc-900/80">
      <button
        type="button"
        className="flex w-full items-center gap-1.5 px-3 py-2 text-left cursor-pointer"
        onClick={() => setCollapsed((c) => !c)}
      >
        <svg
          className={`h-3 w-3 shrink-0 text-zinc-400 transition-transform ${collapsed ? "-rotate-90" : ""}`}
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <polyline points="6 9 12 15 18 9" />
        </svg>
        <span className="text-xs font-medium text-zinc-600 dark:text-zinc-300">Contract</span>
        {hasContent && (
          <span className="ml-auto rounded-full w-2 h-2 bg-zinc-400 dark:bg-zinc-500" />
        )}
      </button>

      {!collapsed && (
        <div className="border-t border-zinc-100 px-3 pb-2.5 pt-2 dark:border-zinc-700 flex flex-col gap-2.5">
          <ContractSection
            label="Expected"
            items={contract.expect}
            onChange={(items) => onChange({ ...contract, expect: items })}
            placeholder="Expected to..."
          />
          <ContractSection
            label="Ask first"
            items={contract.ask}
            onChange={(items) => onChange({ ...contract, ask: items })}
            placeholder="Confirm before..."
          />
          <ContractSection
            label="Never"
            items={contract.never}
            onChange={(items) => onChange({ ...contract, never: items })}
            placeholder="Must never..."
          />
        </div>
      )}
    </div>
  );
}
