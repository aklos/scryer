/**
 * Connection-highlight overlay — connections are drawn on demand, not at rest.
 *
 * When a node is selected, lines fan out from its card to each partner present
 * on the surface, each carrying the relationship verb and a direction arrow;
 * the non-ego cards dim (handled by Surface). This is Shneiderman's
 * details-on-demand: the resting canvas stays calm (each card shows only a
 * connection count), and you summon the wiring for the one node you're checking.
 *
 * Geometry is read from the DOM (`[data-conn-node]` on cards + perimeter refs)
 * relative to this SVG's own origin, so it works uniformly for grid cards and
 * ring reference nodes. The overlay lives INSIDE the PanZoom transform, so it
 * pans with the content for free; it only recomputes on zoom / selection /
 * layout (when the measured rects actually change).
 */

import { useLayoutEffect, useRef, useState } from "react";
import { useZoom } from "./PanZoom";

export interface ConnLink {
  from: string;
  to: string;
  verb: string;
  /** The partner is off-surface (an ancestor boundary, e.g. a person → the
   *  system you've drilled into) — anchor the line to the surface boundary. */
  boundary?: boolean;
}

interface DrawnEdge {
  d: string;
  lx: number;
  ly: number;
  verb: string;
  partnerId: string;
  /** Label sits over no card → safe to show at rest; otherwise hover-only. */
  clear: boolean;
  /** Anchored to the surface boundary, not a partner card (no pan-to nav). */
  boundary: boolean;
}

interface LocalRect {
  cx: number;
  cy: number;
  w: number;
  h: number;
}

function toLocal(r: DOMRect, origin: DOMRect): LocalRect {
  return {
    cx: r.left - origin.left + r.width / 2,
    cy: r.top - origin.top + r.height / 2,
    w: r.width,
    h: r.height,
  };
}

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** AABB overlap between a label box and a card's local rect. */
function overlaps(b: Box, c: LocalRect): boolean {
  const cx0 = c.cx - c.w / 2;
  const cy0 = c.cy - c.h / 2;
  return b.x < cx0 + c.w && cx0 < b.x + b.w && b.y < cy0 + c.h && cy0 < b.y + b.h;
}

/** The point on a rect's border along the ray toward (tx, ty). */
function edgePoint(c: LocalRect, tx: number, ty: number): { x: number; y: number } {
  const dx = tx - c.cx;
  const dy = ty - c.cy;
  // Degenerate ray (coincident centres) → avoid 0 * Infinity = NaN.
  if (dx === 0 && dy === 0) return { x: c.cx, y: c.cy };
  const hw = c.w / 2 - 4;
  const hh = c.h / 2 - 4;
  const s = Math.min(dx ? hw / Math.abs(dx) : Infinity, dy ? hh / Math.abs(dy) : Infinity);
  return { x: c.cx + dx * s, y: c.cy + dy * s };
}

function el(id: string): Element | null {
  const sel =
    typeof CSS !== "undefined" && CSS.escape ? CSS.escape(id) : `${id}`;
  return document.querySelector(`[data-conn-node="${sel}"]`);
}

export function ConnectionsOverlay({
  focusId,
  links,
  layoutKey = "",
  onNavigate,
}: {
  focusId: string | null;
  links: ConnLink[];
  /** A signature of the ego nodes' positions/sizes; bumps when cards move or
   *  resize so the lines re-track layout (pan is free — we're in the transform). */
  layoutKey?: string;
  onNavigate?: (partnerId: string) => void;
}) {
  const zoom = useZoom();
  const svgRef = useRef<SVGSVGElement>(null);
  const [edges, setEdges] = useState<DrawnEdge[]>([]);
  const [hovered, setHovered] = useState<number | null>(null);
  const lastKey = useRef<string>("");

  const linksKey = links.map((l) => `${l.from}>${l.to}|${l.verb}`).join(";");

  // Gate the (layout-flushing) DOM measurement on the inputs that actually move
  // the lines — selection, the link set, zoom, and the ego layout signature —
  // so dragging/resizing unrelated nodes doesn't force a reflow every frame.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useLayoutEffect(() => {
    const svg = svgRef.current;
    const clear = () => {
      if (lastKey.current !== "") {
        lastKey.current = "";
        setEdges([]);
      }
    };
    if (!svg || !focusId || links.length === 0) return clear();
    const focusEl = focusId ? el(focusId) : null;
    if (!focusEl) return clear();

    const origin = svg.getBoundingClientRect();
    const fr = toLocal(focusEl.getBoundingClientRect(), origin);
    // Pass 1: resolve each partner's rect (skip partners not on this surface).
    const items: {
      l: ConnLink;
      partnerId: string;
      pr: LocalRect;
      boundary: boolean;
    }[] = [];
    links.forEach((l) => {
      const partnerId = l.from === focusId ? l.to : l.from;
      // A boundary link (to an off-surface ancestor — e.g. a person → the
      // system you're inside) anchors to the surface boundary, not a card.
      const pEl = l.boundary
        ? document.querySelector("[data-grid]")
        : el(partnerId);
      if (!pEl) return;
      items.push({
        l,
        partnerId,
        pr: toLocal(pEl.getBoundingClientRect(), origin),
        boundary: !!l.boundary,
      });
    });
    // Only real cards count for label-occlusion (the boundary rect is huge).
    const cardRects = [fr, ...items.filter((it) => !it.boundary).map((it) => it.pr)];
    const fontSize = Math.max(9, 11 * zoom);

    const drawn: DrawnEdge[] = items.map((it, i) => {
      const { l, partnerId, pr, boundary } = it;
      // Draw in the model's TRUE direction src→dst (arrow lands on dst),
      // regardless of which end is focused, so incoming links aren't reversed.
      const outgoing = l.from === focusId;
      const src = outgoing ? fr : pr;
      const dst = outgoing ? pr : fr;
      const a = edgePoint(src, dst.cx, dst.cy);
      const b = edgePoint(dst, src.cx, src.cy);
      const mx = (a.x + b.x) / 2;
      const my = (a.y + b.y) / 2;
      const dx = b.x - a.x;
      const dy = b.y - a.y;
      const len = Math.hypot(dx, dy) || 1;
      // Fan multiple lines from one node apart so they don't overlap.
      const off = (i - (items.length - 1) / 2) * 26 * zoom;
      const cx = mx - (dy / len) * off;
      const cy = my + (dx / len) * off;
      const d = `M ${a.x} ${a.y} Q ${cx} ${cy} ${b.x} ${b.y}`;
      const lx = 0.25 * a.x + 0.5 * cx + 0.25 * b.x;
      const ly = 0.25 * a.y + 0.5 * cy + 0.25 * b.y;
      // Show the verb at rest only if its box clears every ego card; the rest
      // reveal on hover so a label never sits on top of card content.
      const labelW = l.verb.length * fontSize * 0.56 + 10 * zoom;
      const labelH = fontSize * 1.7;
      const lbox: Box = { x: lx - labelW / 2, y: ly - labelH / 2, w: labelW, h: labelH };
      const labelClear = !cardRects.some((c) => overlaps(lbox, c));
      return { d, lx, ly, verb: l.verb, partnerId, clear: labelClear, boundary };
    });

    const key = `${zoom}~${drawn.map((e) => e.d + (e.clear ? "c" : "h")).join("|")}`;
    if (key !== lastKey.current) {
      lastKey.current = key;
      setEdges(drawn);
      setHovered(null);
    }
  }, [focusId, linksKey, zoom, layoutKey]);

  if (!focusId) return null;

  const sw = Math.max(1.25, 1.6 * zoom);
  const fontSize = Math.max(9, 11 * zoom);
  const arrow = Math.max(5, 6 * zoom);

  return (
    <svg
      ref={svgRef}
      aria-hidden
      style={{
        position: "absolute",
        left: 0,
        top: 0,
        width: 0,
        height: 0,
        overflow: "visible",
        pointerEvents: "none",
        zIndex: 50,
      }}
    >
      <defs>
        <marker
          id="conn-arrow"
          viewBox="0 0 10 10"
          refX="8"
          refY="5"
          markerWidth={arrow}
          markerHeight={arrow}
          markerUnits="userSpaceOnUse"
          orient="auto-start-reverse"
        >
          <path d="M0 0 L10 5 L0 10 z" fill="var(--accent-blue)" />
        </marker>
      </defs>
      {edges.map((e, i) => {
        const isHover = hovered === i;
        const showLabel = e.verb && (e.clear || isHover);
        const labelW = e.verb.length * fontSize * 0.56 + 10 * zoom;
        const canNav = !!onNavigate && !e.boundary;
        return (
          <g key={`${e.partnerId}-${i}`}>
            {/* visible line */}
            <path
              d={e.d}
              fill="none"
              stroke="var(--accent-blue)"
              strokeWidth={isHover ? sw * 1.7 : sw}
              strokeLinecap="round"
              markerEnd="url(#conn-arrow)"
            />
            {/* fat invisible hit area — hover reveals the verb, click pans to partner */}
            <path
              d={e.d}
              fill="none"
              stroke="transparent"
              strokeWidth={Math.max(14, 16 * zoom)}
              strokeLinecap="round"
              style={{
                pointerEvents: "stroke",
                cursor: canNav ? "pointer" : "default",
              }}
              onMouseEnter={() => setHovered(i)}
              onMouseLeave={() => setHovered((h) => (h === i ? null : h))}
              onClick={
                canNav
                  ? (ev) => {
                      ev.stopPropagation();
                      onNavigate!(e.partnerId);
                    }
                  : undefined
              }
            />
            {showLabel && (
              <g style={{ pointerEvents: "none" }}>
                <rect
                  x={e.lx - labelW / 2}
                  y={e.ly - fontSize * 0.85}
                  width={labelW}
                  height={fontSize * 1.7}
                  rx={4 * zoom}
                  fill="var(--surface-raised)"
                  stroke={isHover ? "var(--accent-blue)" : "var(--border)"}
                  strokeWidth={Math.max(1, zoom)}
                />
                <text
                  x={e.lx}
                  y={e.ly + fontSize * 0.35}
                  textAnchor="middle"
                  fontSize={fontSize}
                  fontWeight={550}
                  fill="var(--text)"
                >
                  {e.verb}
                </text>
              </g>
            )}
          </g>
        );
      })}
    </svg>
  );
}
