/**
 * Fossilization — the canvas visual language for truth-state, applied at the
 * RESPONSIBILITY level. The card itself is a neutral container; each
 * responsibility/property is a seated tile that *reads* as its own state via
 * two orthogonal axes (see `rowSkin`):
 *
 *  1. LIFECYCLE → the tile's material, read on its left edge:
 *     proposed = vapour (dimmed + italic, no edge) · implemented = bare stone
 *     (no edge) · verified = a honed emerald lip · changed = an orange fracture
 *     seam · vagrant = a red unearthed-specimen edge · relocated = a violet
 *     in-flight edge.
 *
 *  2. AGE → a patina, and patina here is SUBTRACTION, not addition. A just-
 *     touched line rides a hair brighter toward `--text`; as code goes untouched
 *     the statement drains toward `--text-muted` and the verified lip dulls off.
 *     Nothing gets dirtier with age — it gets quieter. Age is a property of
 *     CODE, not plans, so `proposed`/`relocated` never weather.
 *
 * Every colour flows through theme tokens (`--accent-*`, `--text-*` in
 * index.css) so it reads on light *and* dark.
 */

import type { Status } from "./viewmodel";

const DAY = 86_400;
// The feel of the timeline lives here — tune these to taste.
export const FRESH_SECS = 2 * DAY; // within this, a touch still rides a lift
export const FOSSIL_START = FRESH_SECS; // settle begins the moment the lift fades — no dead-zone
export const FOSSIL_FULL = 90 * DAY; // fully settled at/after this

const clamp01 = (x: number) => (x < 0 ? 0 : x > 1 ? 1 : x);

/** Statuses that represent code-backed reality — only these weather. Proposed
 *  (not built) and relocated (in-flight refactor) are excluded. */
function isCodeBacked(s: Status | undefined): boolean {
  return s === "implemented" || s === "verified" || s === "changed";
}

/** The two age factors for one code-backed touch date. `gloss` (0..1) eases out
 *  so only a just-touched part clearly lifts; `fossil` (0..1) ramps from the end
 *  of the fresh window to fully settled. Non-code-backed or undated → both 0
 *  (plans don't weather). Shared by the card roll-up and the per-row skin so a
 *  card and its rows age on the same clock. */
function weather(
  status: Status | undefined,
  touchedAt: number | undefined,
  now: number,
): { gloss: number; fossil: number } {
  if (!isCodeBacked(status) || touchedAt == null) return { gloss: 0, fossil: 0 };
  const age = Math.max(0, now - touchedAt);
  const lin = age >= FRESH_SECS ? 0 : clamp01(1 - age / FRESH_SECS);
  const fossil =
    age <= FOSSIL_START
      ? 0
      : clamp01((age - FOSSIL_START) / (FOSSIL_FULL - FOSSIL_START));
  return { gloss: lin * lin, fossil };
}

// --- Per-row skin ------------------------------------------------------------

export interface RowSkin {
  /** Statement/label text colour, already age-adjusted. */
  color: string;
  italic: boolean;
  /** Inset left-edge accent (a box-shadow layer), or null for bare stone. */
  edge: string | null;
}

/** Statement colour as subtractive patina: fresh rides a hair toward `--text`,
 *  age drains toward `--text-muted`. `gloss`/`fossil` never overlap in time, so
 *  at most one mix applies. Capped low so body text stays legible. */
function agedText(gloss: number, fossil: number): string {
  if (gloss > 0)
    return `color-mix(in oklab, var(--text-secondary), var(--text) ${(gloss * 22).toFixed(1)}%)`;
  if (fossil > 0)
    return `color-mix(in oklab, var(--text-secondary), var(--text-muted) ${(fossil * 55).toFixed(1)}%)`;
  return "var(--text-secondary)";
}

/**
 * The visual recipe for one responsibility/property row. `vagrant` overrides the
 * status edge (uncatalogued code leads with its red edge). Persons/externals pass
 * `status: undefined` → neutral baseline (their lines don't carry truth-state).
 */
export function rowSkin(
  status: Status | undefined,
  touchedAt: number | undefined,
  vagrant: boolean,
  now: number,
  zoom: number,
): RowSkin {
  // Floor the lip at 1 device px so the status edge survives sub-pixel rendering
  // when zoomed out (still scales up past 1× when zoomed in).
  const lip = `inset ${Math.max(1, 2 * zoom).toFixed(2)}px 0 0 0`;
  if (vagrant) {
    return { color: "var(--text-secondary)", italic: false, edge: `${lip} var(--accent-red)` };
  }

  const { gloss, fossil } = weather(status, touchedAt, now);
  switch (status) {
    case "proposed":
    case undefined:
      // Vapour — provisional. Dimmed + italic; no edge, no weather.
      return { color: "var(--text-muted)", italic: true, edge: null };
    case "verified": {
      // Honed lip that polishes off (dulls toward transparent) with age.
      const a = (1 - 0.5 * fossil).toFixed(3);
      return {
        color: agedText(gloss, fossil),
        italic: false,
        edge: `${lip} color-mix(in srgb, var(--accent-emerald) calc(${a} * 100%), transparent)`,
      };
    }
    case "changed":
      return {
        color: agedText(gloss, fossil),
        italic: false,
        edge: `${lip} var(--accent-orange)`,
      };
    case "relocated":
      // In-flight refactor — leads with its edge; doesn't weather.
      return { color: "var(--text-muted)", italic: false, edge: `${lip} var(--accent-violet)` };
    case "vagrant":
      return { color: "var(--text-secondary)", italic: false, edge: `${lip} var(--accent-red)` };
    case "implemented":
    default:
      // Bare stone — the baseline. Distinct only by the absence of an edge;
      // settles toward muted with age.
      return { color: agedText(gloss, fossil), italic: false, edge: null };
  }
}
