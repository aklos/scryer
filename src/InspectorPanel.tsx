/**
 * Read-side inspector. Driven by canvas selection (see ./selection). The card
 * carries the authored intent (title/status/responsibilities); this panel shows
 * the *code-side* the card omits — boundary, connections, and the per-
 * responsibility source mapping rendered as real code ("read through to code").
 *
 * Strictly read-only: editing intent stays on the card (pencil/double-click).
 */

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  X,
  ArrowUpRight,
  ArrowDownLeft,
  ChevronLeft,
  ExternalLink,
  FileCode2,
  CircleSlash,
} from "lucide-react";
import type { ScryModel, Node, Link, Responsibility } from "./viewmodel";
import { isDataShape } from "./viewmodel";
import type { Selection } from "./selection";
import { STATUS_COLORS } from "./statusColors";
import { effectiveRespStatus } from "./rollup";
import { DATA_SHAPE_ICON, KIND_ICON } from "./kindIcons";

interface Segment {
  text: string;
  kind: string;
}

interface SourceSpan {
  file: string;
  startLine: number;
  focusStart: number;
  focusEnd: number;
  lines: Segment[][];
}

/** Coarse token class → themed colour. Empty kind inherits the line colour. */
const TOKEN_COLOR: Record<string, string> = {
  keyword: "var(--color-violet-400)",
  string: "var(--color-emerald-400)",
  comment: "var(--text-ghost)",
  number: "var(--color-orange-400)",
  constant: "var(--color-orange-300)",
  function: "var(--color-blue-400)",
  type: "var(--color-cyan-400)",
  property: "var(--color-blue-300)",
  tag: "var(--color-red-400)",
  operator: "var(--text-muted)",
  punct: "var(--text-muted)",
};

// ---------------------------------------------------------------------------
// Shell + resize
// ---------------------------------------------------------------------------

export function InspectorPanel({
  model,
  selection,
  projectPath,
  onSelectNode,
  onSelectResponsibility,
  onClose,
}: {
  model: ScryModel;
  selection: NonNullable<Selection>;
  projectPath: string | null;
  onSelectNode: (nodeId: string) => void;
  onSelectResponsibility: (nodeId: string, respId: string) => void;
  onClose: () => void;
}) {
  const node = model.nodes.find((n) => n.id === selection.nodeId);

  const [width, setWidth] = useState(() => {
    const saved = Number(localStorage.getItem("scryer:inspectorWidth"));
    return saved >= 360 ? saved : 540;
  });
  useEffect(() => {
    localStorage.setItem("scryer:inspectorWidth", String(width));
  }, [width]);
  const startResize = (e: React.PointerEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = width;
    const onMove = (ev: PointerEvent) =>
      setWidth(Math.min(1100, Math.max(360, startW + (startX - ev.clientX))));
    const onUp = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  };

  return (
    <aside
      data-no-pickup
      style={{ width }}
      className="relative flex h-full shrink-0 flex-col bg-[var(--surface)] text-[var(--text)]"
    >
      <div
        onPointerDown={startResize}
        className="group/resize absolute left-0 top-0 z-20 flex h-full w-2 -translate-x-1/2 cursor-col-resize items-stretch"
      >
        <span className="m-auto h-full w-px bg-[var(--border)] transition-colors group-hover/resize:bg-[var(--color-blue-500)]" />
      </div>

      {!node ? (
        <div className="flex flex-1 items-center justify-center p-6 text-[12px] text-[var(--text-muted)]">
          Selection no longer exists.
        </div>
      ) : selection.kind === "node" ? (
        <NodeInspector
          model={model}
          node={node}
          projectPath={projectPath}
          onSelectResponsibility={onSelectResponsibility}
          onClose={onClose}
        />
      ) : (
        <ResponsibilityInspector
          model={model}
          node={node}
          respId={selection.respId}
          projectPath={projectPath}
          onSelectNode={onSelectNode}
          onClose={onClose}
        />
      )}
    </aside>
  );
}

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

function CloseButton({ onClose }: { onClose: () => void }) {
  return (
    <button
      type="button"
      onClick={onClose}
      aria-label="Close inspector"
      className="-mr-1 shrink-0 cursor-pointer rounded p-1 text-[var(--text-ghost)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)]"
    >
      <X className="h-4 w-4" />
    </button>
  );
}

function Eyebrow({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex items-center gap-1.5 text-[10px] font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
      {children}
    </div>
  );
}

/** A labelled content group with breathing room and a count. */
function Group({
  label,
  count,
  children,
}: {
  label: string;
  count?: number;
  children: React.ReactNode;
}) {
  return (
    <section className="px-4 py-3.5">
      <div className="mb-2 flex items-center gap-2">
        <span className="text-[10px] font-semibold uppercase tracking-[0.14em] text-[var(--text-tertiary)]">
          {label}
        </span>
        {count != null && count > 0 && (
          <span className="rounded bg-[var(--surface-inset)] px-1.5 text-[10px] tabular-nums text-[var(--text-muted)]">
            {count}
          </span>
        )}
      </div>
      {children}
    </section>
  );
}

function StatusPill({ node, resp }: { node: Node; resp: Responsibility }) {
  const status = effectiveRespStatus(node, resp);
  const colors = STATUS_COLORS[status] ?? null;
  if (!colors) return null;
  return (
    <span className="inline-flex items-center gap-1.5 text-[11px] text-[var(--text-secondary)]">
      <span className={`h-1.5 w-1.5 rounded-full ${colors.dot}`} />
      {colors.label}
    </span>
  );
}

function Empty({ children }: { children: React.ReactNode }) {
  return (
    <div className="text-[11px] leading-relaxed text-[var(--text-ghost)]">
      {children}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Node inspector
// ---------------------------------------------------------------------------

function NodeInspector({
  model,
  node,
  projectPath,
  onSelectResponsibility,
  onClose,
}: {
  model: ScryModel;
  node: Node;
  projectPath: string | null;
  onSelectResponsibility: (nodeId: string, respId: string) => void;
  onClose: () => void;
}) {
  const boundary = model.boundaries?.[node.id] ?? [];
  const responsibilities = node.responsibilities ?? [];
  const sourceMap = model.sourceMap ?? {};
  const properties = node.properties ?? [];
  const dataShape = isDataShape(node);
  const eyebrow = dataShape ? DATA_SHAPE_ICON : KIND_ICON[node.kind];
  const Icon = eyebrow?.Icon;
  // A symbol that declares a data shape maps its declaration location by node
  // id (rather than per-responsibility); show it as the type's definition.
  const definition = properties.length > 0 ? sourceMap[node.id] ?? [] : [];
  const outgoing = model.links.filter((l) => l.src === node.id);
  const incoming = model.links.filter((l) => l.dst === node.id);
  const nameOf = (id: string) => model.nodes.find((n) => n.id === id)?.name ?? id;

  return (
    <>
      <header className="flex items-start gap-3 border-b border-[var(--border-subtle)] px-4 pb-3 pt-3.5">
        <div className="min-w-0 flex-1">
          <Eyebrow>
            {Icon && <Icon className="h-3 w-3" />}
            {dataShape ? "data type" : node.kind}
          </Eyebrow>
          <h2 className="mt-1 truncate text-[15px] font-semibold leading-tight text-[var(--text)]">
            {node.name || "Untitled"}
          </h2>
          {node.technology && (
            <div className="mt-0.5 text-[11px] italic text-[var(--text-muted)]">
              {node.technology}
            </div>
          )}
        </div>
        <CloseButton onClose={onClose} />
      </header>

      <div className="flex-1 divide-y divide-[var(--border-subtle)] overflow-y-auto">
        {node.description && (
          <p className="px-4 py-3.5 text-[12.5px] leading-relaxed text-[var(--text-secondary)]">
            {node.description}
          </p>
        )}

        {properties.length > 0 && (
          <>
            <Group label="Properties" count={properties.length}>
              {properties.length === 0 ? (
                <Empty>No properties.</Empty>
              ) : (
                <ul className="flex flex-col gap-1.5">
                  {properties.map((p, i) => {
                    const colors = p.status ? STATUS_COLORS[p.status] : null;
                    return (
                      <li key={i} className="flex items-start gap-2.5">
                        <span
                          className={`mt-[5px] h-1.5 w-1.5 shrink-0 rounded-full ${
                            colors ? colors.dot : "bg-[var(--text-ghost)]"
                          }`}
                          title={colors?.label}
                        />
                        <span className="flex-1 text-[12.5px] leading-snug">
                          <span className="font-mono font-medium text-[var(--text-secondary)]">
                            {p.label}
                          </span>
                          {p.description && (
                            <span className="text-[var(--text-muted)]"> — {p.description}</span>
                          )}
                        </span>
                      </li>
                    );
                  })}
                </ul>
              )}
            </Group>

            <Group label="Definition" count={definition.length}>
              {definition.length === 0 ? (
                <Empty>
                  Not yet mapped. Re-run a fill to anchor this type to its
                  declaration.
                </Empty>
              ) : (
                <div className="flex flex-col gap-2.5">
                  {definition.map((loc, i) => (
                    <CodeBlock
                      key={i}
                      projectPath={projectPath}
                      pattern={loc.pattern}
                      symbol={loc.symbol}
                      line={loc.line}
                      endLine={loc.endLine}
                    />
                  ))}
                </div>
              )}
            </Group>
          </>
        )}

        {!dataShape && (
        <>
        <Group label="Responsibilities" count={responsibilities.length}>
          {responsibilities.length === 0 ? (
            <Empty>No responsibilities.</Empty>
          ) : (
            <ul className="-mx-1 flex flex-col">
              {responsibilities.map((r) => {
                const mapped = sourceMap[r.id]?.length ?? 0;
                const status = effectiveRespStatus(node, r);
                const colors = STATUS_COLORS[status] ?? null;
                return (
                  <li key={r.id}>
                    <button
                      type="button"
                      onClick={() => onSelectResponsibility(node.id, r.id)}
                      className="group/r flex w-full items-start gap-2.5 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-[var(--surface-hover)] cursor-pointer"
                    >
                      <span
                        className={`mt-[5px] h-1.5 w-1.5 shrink-0 rounded-full ${
                          colors ? colors.dot : "bg-[var(--text-ghost)]"
                        }`}
                        title={colors?.label}
                      />
                      <span className="flex-1 text-[12.5px] leading-snug text-[var(--text-secondary)] group-hover/r:text-[var(--text)]">
                        {r.statement}
                      </span>
                      <span
                        className={`mt-px inline-flex shrink-0 items-center gap-1 text-[10px] tabular-nums ${
                          mapped > 0
                            ? "text-[var(--text-muted)]"
                            : "text-[var(--text-ghost)]"
                        }`}
                        title={mapped > 0 ? `${mapped} mapped span(s)` : "unmapped"}
                      >
                        {mapped > 0 ? (
                          <>
                            <FileCode2 className="h-3 w-3" />
                            {mapped}
                          </>
                        ) : (
                          <CircleSlash className="h-3 w-3" />
                        )}
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </Group>

        <Group label="Boundary" count={boundary.length}>
          {boundary.length === 0 ? (
            <Empty>No code boundary mapped yet.</Empty>
          ) : (
            <ul className="flex flex-col gap-1">
              {boundary.map((s, i) => (
                <li key={i} className="text-[11px] leading-relaxed">
                  <span className="font-mono text-[var(--text-secondary)]">
                    {s.pattern}
                  </span>
                  {s.comment && (
                    <span className="text-[var(--text-muted)]"> — {s.comment}</span>
                  )}
                </li>
              ))}
            </ul>
          )}
        </Group>
        </>
        )}

        {(outgoing.length > 0 || incoming.length > 0) && (
          <Group label="Connections" count={outgoing.length + incoming.length}>
            <ul className="flex flex-col gap-1.5">
              {outgoing.map((l) => (
                <ConnRow key={l.id} dir="out" partner={nameOf(l.dst)} link={l} />
              ))}
              {incoming.map((l) => (
                <ConnRow key={l.id} dir="in" partner={nameOf(l.src)} link={l} />
              ))}
            </ul>
          </Group>
        )}
      </div>
    </>
  );
}

// ---------------------------------------------------------------------------
// Responsibility inspector — code is the hero
// ---------------------------------------------------------------------------

function ResponsibilityInspector({
  model,
  node,
  respId,
  projectPath,
  onSelectNode,
  onClose,
}: {
  model: ScryModel;
  node: Node;
  respId: string;
  projectPath: string | null;
  onSelectNode: (nodeId: string) => void;
  onClose: () => void;
}) {
  const resp = (node.responsibilities ?? []).find((r) => r.id === respId);
  const locations = model.sourceMap?.[respId] ?? [];

  return (
    <>
      <header className="border-b border-[var(--border-subtle)] px-4 pb-3 pt-3.5">
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => onSelectNode(node.id)}
            className="-ml-1 flex min-w-0 items-center gap-1 rounded px-1 py-0.5 text-[11px] font-medium text-[var(--text-muted)] transition-colors hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)] cursor-pointer"
          >
            <ChevronLeft className="h-3.5 w-3.5 shrink-0" />
            <span className="truncate">{node.name || "Untitled"}</span>
          </button>
          <span className="flex-1" />
          <CloseButton onClose={onClose} />
        </div>
        {resp ? (
          <>
            <p className="mt-2.5 text-[14px] font-medium leading-snug text-[var(--text)]">
              {resp.statement}
            </p>
            <div className="mt-2">
              <StatusPill node={node} resp={resp} />
            </div>
          </>
        ) : (
          <p className="mt-2.5 text-[13px] text-[var(--text-muted)]">
            This responsibility no longer exists.
          </p>
        )}
      </header>

      {resp && (
        <div className="flex flex-1 flex-col overflow-y-auto">
          <Group label="Mapped code" count={locations.length}>
            {locations.length === 0 ? (
              <Empty>
                Not yet mapped. Re-run a fill to anchor this responsibility to
                the code that discharges it.
              </Empty>
            ) : (
              <div className="flex flex-col gap-2.5">
                {locations.map((loc, i) => (
                  <CodeBlock
                    key={i}
                    projectPath={projectPath}
                    pattern={loc.pattern}
                    symbol={loc.symbol}
                    line={loc.line}
                    endLine={loc.endLine}
                  />
                ))}
              </div>
            )}
          </Group>

          {(resp.relocatedFrom || resp.relocatedTo) && (
            <Group label="Relocation">
              {resp.relocatedFrom && <Empty>Moved here from another node.</Empty>}
              {resp.relocatedTo && <Empty>Moved out to another node.</Empty>}
            </Group>
          )}
        </div>
      )}
    </>
  );
}

// ---------------------------------------------------------------------------
// Code rendering
// ---------------------------------------------------------------------------

function CodeBlock({
  projectPath,
  pattern,
  symbol,
  line,
  endLine,
}: {
  projectPath: string | null;
  pattern: string;
  symbol?: string;
  line?: number;
  endLine?: number;
}) {
  // A mapping with neither a symbol anchor nor an explicit line is whole-file:
  // there's no precise span to show, so don't dump the file head as "code".
  const anchored = Boolean(symbol) || line != null;

  const containerRef = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);
  const [span, setSpan] = useState<SourceSpan | null>(null);
  const [err, setErr] = useState<string | null>(null);

  // Lazy: only fetch + parse once the block scrolls near view.
  useEffect(() => {
    if (!anchored) return;
    const el = containerRef.current;
    if (!el) return;
    const obs = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setVisible(true);
          obs.disconnect();
        }
      },
      { rootMargin: "300px" },
    );
    obs.observe(el);
    return () => obs.disconnect();
  }, [anchored]);

  useEffect(() => {
    if (!anchored || !visible) return;
    if (!projectPath) {
      setErr("No project path.");
      return;
    }
    let cancelled = false;
    setSpan(null);
    setErr(null);
    invoke<SourceSpan>("read_source_span", {
      projectPath,
      file: pattern,
      symbol: symbol ?? null,
      line: line ?? null,
      endLine: endLine ?? null,
    })
      .then((s) => !cancelled && setSpan(s))
      .catch((e) => !cancelled && setErr(String(e)));
    return () => {
      cancelled = true;
    };
  }, [anchored, visible, projectPath, pattern, symbol, line, endLine]);

  const openInEditor = () =>
    void invoke("open_in_editor", {
      file: pattern,
      line: span?.focusStart ?? line ?? null,
      projectPath,
    });

  // Whole-file mapping: honest one-liner, never an import dump.
  if (!anchored) {
    return (
      <button
        type="button"
        onClick={openInEditor}
        title="Open in editor"
        className="flex w-full items-center gap-2 rounded-md border border-dashed border-[var(--border)] px-3 py-2 text-left transition-colors hover:bg-[var(--surface-hover)] cursor-pointer"
      >
        <CircleSlash className="h-3.5 w-3.5 shrink-0 text-[var(--text-ghost)]" />
        <span className="min-w-0 flex-1">
          <span className="block font-mono text-[11px] text-[var(--text-secondary)]">
            {pattern}
          </span>
          <span className="text-[10px] text-[var(--text-ghost)]">
            whole-file mapping — no symbol anchor
          </span>
        </span>
        <ExternalLink className="h-3.5 w-3.5 shrink-0 text-[var(--text-ghost)]" />
      </button>
    );
  }

  return (
    <div
      ref={containerRef}
      className="overflow-hidden rounded-md border border-[var(--border)]"
    >
      <button
        type="button"
        onClick={openInEditor}
        title="Open in editor"
        className="flex w-full items-center gap-2 border-b border-[var(--border-subtle)] bg-[var(--surface-raised)] px-3 py-2 text-left transition-colors hover:bg-[var(--surface-hover)] cursor-pointer"
      >
        {symbol && (
          <span className="shrink-0 font-mono text-[11px] font-semibold text-[var(--text)]">
            {symbol}
          </span>
        )}
        <span className="min-w-0 flex-1 truncate font-mono text-[10px] text-[var(--text-muted)]">
          {pattern}
          {span && (
            <span className="text-[var(--text-ghost)]">
              :{span.focusStart}
              {span.focusEnd !== span.focusStart ? `–${span.focusEnd}` : ""}
            </span>
          )}
        </span>
        <ExternalLink className="h-3.5 w-3.5 shrink-0 text-[var(--text-ghost)]" />
      </button>

      {err ? (
        <div className="px-3 py-2 font-mono text-[10.5px] text-red-400/80">
          {err}
        </div>
      ) : !span ? (
        <div className="px-3 py-2 font-mono text-[10.5px] text-[var(--text-ghost)]">
          loading…
        </div>
      ) : (
        <div className="overflow-x-auto bg-[var(--surface-inset)] py-1.5 font-mono text-[11px] leading-[1.6]">
          {span.lines.map((segs, i) => {
            const lineNo = span.startLine + i;
            const focus = lineNo >= span.focusStart && lineNo <= span.focusEnd;
            return (
              <div
                key={i}
                className="flex"
                style={
                  focus
                    ? {
                        backgroundColor:
                          "color-mix(in srgb, var(--color-blue-500) 11%, transparent)",
                        boxShadow:
                          "inset 2px 0 0 0 color-mix(in srgb, var(--color-blue-500) 70%, transparent)",
                      }
                    : { opacity: 0.5 }
                }
              >
                <span className="w-10 shrink-0 select-none pr-2 text-right tabular-nums text-[var(--text-ghost)]">
                  {lineNo}
                </span>
                <span className="whitespace-pre pr-4 text-[var(--text)]">
                  {segs.length === 0
                    ? " "
                    : segs.map((s, j) => (
                        <span
                          key={j}
                          style={s.kind ? { color: TOKEN_COLOR[s.kind] } : undefined}
                        >
                          {s.text}
                        </span>
                      ))}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function ConnRow({
  dir,
  partner,
  link,
}: {
  dir: "out" | "in";
  partner: string;
  link: Link;
}) {
  const Arrow = dir === "out" ? ArrowUpRight : ArrowDownLeft;
  return (
    <li className="flex items-baseline gap-2 text-[11.5px]">
      <Arrow
        className={`relative top-0.5 h-3.5 w-3.5 shrink-0 ${
          dir === "out" ? "text-[var(--text-muted)]" : "text-[var(--text-ghost)]"
        }`}
      />
      <span className="shrink-0 font-medium text-[var(--text-secondary)]">
        {partner}
      </span>
      {link.label && (
        <span className="truncate text-[var(--text-muted)]">{link.label}</span>
      )}
    </li>
  );
}
