/**
 * Run many agents on one codebase, as a picture rather than the app: the
 * codebase as one solid form, turning slowly, and agents around it, each
 * tethered to the point it is working on, its work flowing down the tether.
 * Where an agent works the surface rises; the form carries that growth
 * round as it turns, so every agent's work shapes the same body and each
 * sees what the others left. Nothing waits for anything else.
 */

import { ACCENT, mix, p, s } from "../../kit";
import { CardTitle, Dock, Stage2, cardStyle } from "../dressing";

export const AGENTS2 = s(18);

type V = [number, number, number];

const K = { clear: s(2.8), form: s(2.9), work: s(4.0), end: s(17) };

const HUES = [ACCENT, "#38BDF8", "#A78BFA", "#F0B44C", "#34D399", "#F472B6"];
const CX = 960;
const CY = 410;
const FOCAL = 1600;

const dot = (a: V, b: V) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const norm = (a: V): V => {
  const l = Math.hypot(a[0], a[1], a[2]) || 1;
  return [a[0] / l, a[1] / l, a[2] / l];
};
const cross = (a: V, b: V): V => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];

/** A sphere of near-even triangles: an icosahedron, each face split three times. */
const SPHERE = (() => {
  const t = (1 + Math.sqrt(5)) / 2;
  let verts: V[] = [
    [-1, t, 0], [1, t, 0], [-1, -t, 0], [1, -t, 0],
    [0, -1, t], [0, 1, t], [0, -1, -t], [0, 1, -t],
    [t, 0, -1], [t, 0, 1], [-t, 0, -1], [-t, 0, 1],
  ].map((v) => norm(v as V));
  let faces: [number, number, number][] = [
    [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
    [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
    [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
    [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
  ];
  for (let level = 0; level < 3; level++) {
    const mid = new Map<string, number>();
    const half = (a: number, b: number) => {
      const key = a < b ? `${a}-${b}` : `${b}-${a}`;
      const hit = mid.get(key);
      if (hit !== undefined) return hit;
      verts.push(norm([(verts[a][0] + verts[b][0]) / 2, (verts[a][1] + verts[b][1]) / 2, (verts[a][2] + verts[b][2]) / 2]));
      mid.set(key, verts.length - 1);
      return verts.length - 1;
    };
    faces = faces.flatMap(([a, b, c]) => {
      const ab = half(a, b);
      const bc = half(b, c);
      const ca = half(c, a);
      return [[a, ab, ca], [b, bc, ab], [c, ca, bc], [ab, bc, ca]] as [number, number, number][];
    });
  }
  return { verts, faces };
})();

/** The form's turn at frame `f`: a slow yaw under a gently rocking tilt. */
function turn(f: number) {
  const t = f / 30;
  return { yaw: t * 0.22, pitch: 0.32 + 0.08 * Math.sin(t * 0.35) };
}

function rotate(v: V, yaw: number, pitch: number): V {
  const [x, y, z] = v;
  const x1 = x * Math.cos(yaw) + z * Math.sin(yaw);
  const z1 = -x * Math.sin(yaw) + z * Math.cos(yaw);
  const y2 = y * Math.cos(pitch) - z1 * Math.sin(pitch);
  const z2 = y * Math.sin(pitch) + z1 * Math.cos(pitch);
  return [x1, y2, z2];
}

function unrotate(v: V, yaw: number, pitch: number): V {
  const [x, y2, z2] = v;
  const y = y2 * Math.cos(pitch) + z2 * Math.sin(pitch);
  const z1 = -y2 * Math.sin(pitch) + z2 * Math.cos(pitch);
  const xx = x * Math.cos(yaw) - z1 * Math.sin(yaw);
  const zz = x * Math.sin(yaw) + z1 * Math.cos(yaw);
  return [xx, y, zz];
}

/** Where agent `k` hovers at frame `f`, as a direction from the centre (camera frame). */
function agentDir(k: number, f: number): V {
  const t = f / 30;
  const tilt = (k / HUES.length) * Math.PI;
  const a = t * (0.32 + 0.05 * k) + (k * Math.PI * 2) / HUES.length;
  const v: V = [Math.cos(a), Math.sin(a) * Math.sin(tilt) * 0.75, Math.sin(a) * Math.cos(tilt)];
  // Agents favour the side facing the viewer, so the work is seen.
  return norm([v[0], v[1] * 0.9, v[2] * 0.6 - 0.45]);
}

/** The base size: small at first, larger as the work adds up. */
const radius = (f: number) => mix(110, 250, Math.pow(p(f, K.work, K.end - K.work, "move"), 0.9));

const STEP = s(0.4);

/**
 * Every place an agent has worked so far, in the form's own frame, so it
 * stays where it was made as the form turns.
 */
function marks(f: number): { at: V; age: number; k: number }[] {
  const out: { at: V; age: number; k: number }[] = [];
  for (let ts = K.work; ts <= f; ts += STEP) {
    const { yaw, pitch } = turn(ts);
    for (let k = 0; k < HUES.length; k++) out.push({ at: unrotate(agentDir(k, ts), yaw, pitch), age: f - ts, k });
  }
  return out;
}

/** How far the surface stands out in direction `v` (form frame). */
function relief(v: V, f: number, ms: { at: V; age: number }[]): number {
  const t = f / 30;
  const morph = 0.07 * Math.sin(3 * v[0] + t * 0.8) * Math.cos(2 * v[1] - t * 0.6) + 0.05 * Math.sin(4 * v[2] + 2 * v[0] + t * 0.5);
  let grow = 0;
  for (const m of ms) {
    const d = 1 - dot(v, m.at);
    if (d > 0.2) continue;
    grow += 0.016 * Math.min(1, m.age / s(0.8)) * Math.exp(-d / 0.035);
  }
  return 1 + morph + Math.min(0.55, grow);
}

function hexRgb(h: string): V {
  const n = parseInt(h.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

const LIGHT = norm([-0.45, -0.65, -0.62]);

export function Agents2({ f }: { f: number }) {
  const show = p(f, K.form, s(1.0));
  const R = radius(f) * mix(0.6, 1, show);
  const { yaw, pitch } = turn(f);
  const ms = marks(f);

  // The surface: each vertex pushed out by its relief, turned, projected.
  const world = SPHERE.verts.map((v) => {
    const r = R * relief(v, f, ms);
    return rotate([v[0] * r, v[1] * r, v[2] * r], yaw, pitch);
  });
  const screen = world.map(([x, y, z]) => {
    const k = FOCAL / (FOCAL + z);
    return [CX + x * k, CY + y * k] as [number, number];
  });

  // Where each agent is working now, for the warm patch under it.
  const now = HUES.map((_, k) => agentDir(k, f));

  const faces = SPHERE.faces
    .map(([a, b, c]) => {
      const n = norm(cross([world[b][0] - world[a][0], world[b][1] - world[a][1], world[b][2] - world[a][2]], [world[c][0] - world[a][0], world[c][1] - world[a][1], world[c][2] - world[a][2]]));
      const mid: V = [(world[a][0] + world[b][0] + world[c][0]) / 3, (world[a][1] + world[b][1] + world[c][1]) / 3, (world[a][2] + world[b][2] + world[c][2]) / 3];
      return { a, b, c, n, mid };
    })
    // Facing the camera only (the camera looks along +z).
    .filter((fc) => fc.n[2] < 0)
    .sort((m, n) => n.mid[2] - m.mid[2]);

  const base: V = [30, 36, 47];
  const lit: V = [128, 142, 168];

  const agents = HUES.map((hue, k) => {
    const d = now[k];
    const along = mix(1.7, 1.45, show);
    const pos: V = [d[0] * R * along, d[1] * R * along, d[2] * R * along];
    const local = unrotate(d, yaw, pitch);
    const surf = R * relief(local, f, ms);
    const touch: V = [d[0] * surf, d[1] * surf, d[2] * surf];
    const proj = (v: V): [number, number] => {
      const kk = FOCAL / (FOCAL + v[2]);
      return [CX + v[0] * kk, CY + v[1] * kk];
    };
    return { hue, k, at: proj(pos), touch: proj(touch), z: pos[2], on: p(f, K.work - s(0.6) + k * 4, s(0.8)) };
  });

  const Agent = ({ ag }: { ag: (typeof agents)[number] }) => (
    <g opacity={ag.on * show}>
      {/* The tether, and the work flowing down it to the surface. */}
      <line x1={ag.at[0]} y1={ag.at[1]} x2={ag.touch[0]} y2={ag.touch[1]} stroke={ag.hue} strokeOpacity={0.55} strokeWidth={1.5} />
      {[0, 1, 2, 3].map((i) => {
        const u = ((f / 30) * 1.1 + i / 4 + ag.k * 0.13) % 1;
        return <circle key={i} cx={mix(ag.at[0], ag.touch[0], u)} cy={mix(ag.at[1], ag.touch[1], u)} r={2.6} fill={ag.hue} fillOpacity={0.9 * Math.sin(u * Math.PI)} />;
      })}
      <circle cx={ag.touch[0]} cy={ag.touch[1]} r={9} fill="none" stroke={ag.hue} strokeOpacity={0.8} strokeWidth={1.5} />
      <circle cx={ag.at[0]} cy={ag.at[1]} r={16} fill="#0B0D11" stroke={ag.hue} strokeOpacity={0.45} strokeWidth={1.5} />
      <circle cx={ag.at[0]} cy={ag.at[1]} r={7} fill={ag.hue} />
    </g>
  );

  return (
    <Stage2>
      <div style={{ position: "absolute", left: 0, right: 0, top: 470, display: "flex", justifyContent: "center", zIndex: 6, ...cardStyle(f, K.clear) }}>
        <CardTitle f={f} lines={["Run many agents on one codebase."]} size={104} />
      </div>
      {show > 0 ? (
        <svg width={1920} height={1080} style={{ position: "absolute", inset: 0, opacity: show }}>
          {agents.filter((ag) => ag.z > 0).map((ag) => (
            <Agent key={ag.k} ag={ag} />
          ))}
          {faces.map(({ a, b, c, n, mid }, i) => {
            const lamb = Math.max(0, dot(n, LIGHT));
            const rim = Math.pow(1 - Math.abs(n[2]), 3);
            let col: V = [mix(base[0], lit[0], lamb) + 60 * rim, mix(base[1], lit[1], lamb) + 70 * rim, mix(base[2], lit[2], lamb) + 90 * rim];
            // A warm patch where an agent is working right now.
            const dir = norm(mid);
            for (let k = 0; k < HUES.length; k++) {
              const w = Math.exp(-(1 - dot(dir, now[k])) / 0.018) * agents[k].on;
              if (w < 0.02) continue;
              const h = hexRgb(HUES[k]);
              col = [mix(col[0], h[0], 0.7 * w), mix(col[1], h[1], 0.7 * w), mix(col[2], h[2], 0.7 * w)];
            }
            const fill = `rgb(${Math.round(col[0])},${Math.round(col[1])},${Math.round(col[2])})`;
            const d = `M${screen[a][0].toFixed(1)},${screen[a][1].toFixed(1)}L${screen[b][0].toFixed(1)},${screen[b][1].toFixed(1)}L${screen[c][0].toFixed(1)},${screen[c][1].toFixed(1)}Z`;
            return <path key={i} d={d} fill={fill} stroke={fill} strokeWidth={0.8} strokeLinejoin="round" />;
          })}
          {agents.filter((ag) => ag.z <= 0).map((ag) => (
            <Agent key={ag.k} ag={ag} />
          ))}
        </svg>
      ) : null}
      <Dock f={f} at={K.work} text="Run many agents on one codebase." subs={[{ at: K.work, text: "Each sees where the others have been. Nothing locks." }]} />
    </Stage2>
  );
}
