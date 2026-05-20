/**
 * View-model types for the layer-1 viewer.
 *
 * The canvas is a single flat inventory grid. Every entry occupies a whole-cell
 * footprint at a stored `cell`. Groups are explicit rectangular regions on the
 * same grid — they have a stored `cell` (top-left) and `size` (cols x rows).
 * Entries with a matching `groupId` must be placed within that region.
 *
 * Nesting is via `parentGroupId`. Depth tinting distinguishes levels.
 */

import type { Status } from "./statusColors";

export type { Status };

export type Altitude = "system" | "container" | "component";

/**
 * Element kind — C4 element taxonomy. Required on every entry.
 *
 * - `person`: actor; lives on the perimeter; no drill-in; no responsibilities.
 * - `system` / `container` / `component`: in-scope structure. The kind must
 *   match the surface's altitude.
 *
 * The `external` flag (only meaningful on `system` / `container`) marks the
 * element as out-of-scope — same kind, styled differently. Externals can't
 * drill in and don't carry status; any responsibilities on them are read as
 * expectations, not commitments.
 */
export type Kind = "person" | "system" | "container" | "component";

export interface Cell {
  row: number;
  col: number;
}

export interface Responsibility {
  id: string;
  text: string;
  detail?: string;
  /** Optional — externals carry unstatused expectations. */
  status?: Status;
}

export interface Link {
  to: string;
  label: string;
}

export interface Entry {
  id: string;
  title: string;
  kind: Kind;
  /** Out-of-scope marker. Only meaningful on `system` / `container`. */
  external?: boolean;
  responsibilities: Responsibility[];
  /** Technology tag — what this card *is*, as a product/stack (e.g.
   * "Next.js", "MongoDB 7", "S3 Bucket", "Resend"). Applies to externals
   * too; only persons don't carry it. */
  technology?: string;
  /** A 1-2 sentence description of what this card is / does. Applies to any
   * kind (persons, externals, in-scope). */
  description?: string;
  childSurfaceId?: string;
  fulfills?: string;
  links?: Link[];
  cell?: Cell;
  groupId?: string;
}

/**
 * A group — an explicit rectangular region on the flat grid. Has a stored
 * position (`cell`) and dimensions (`size`). Rendered as a tinted, bordered
 * background. User resizes via a corner handle.
 */
export interface Group {
  id: string;
  name: string;
  cell: Cell;
  size: { cols: number; rows: number };
  parentGroupId?: string;
}

export interface Surface {
  id: string;
  altitude: Altitude;
  entries: Entry[];
  groups: Group[];
}

export interface Model {
  surfaces: Record<string, Surface>;
  rootSurfaceId: string;
}
