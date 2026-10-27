/**
 * The app's vocabulary: every word a surface uses for a kind of node, a
 * claim's state or a structural idea, with the one line that defines it and
 * the docs section that explains it. Surfaces show these through `Term`, so a
 * word reads the same everywhere and explains itself on hover.
 */

import type { ClaimState } from "./derived";

/** Where the docs site is published; a term's `docs` is a path under it. */
export const DOCS_BASE = "https://aklos.github.io/scryer/";

export interface GlossaryEntry {
  label: string;
  definition: string;
  /** Path and anchor under {@link DOCS_BASE}. */
  docs?: string;
}

export const GLOSSARY = {
  claim: {
    label: "claim",
    definition: "A behaviour test's description, read as a statement about the code it tests.",
    docs: "concepts/claims/",
  },
  proven: {
    label: "proven",
    definition: "Its test was green, and neither the test nor the code it exercises has changed since.",
    docs: "concepts/status/",
  },
  unverified: {
    label: "unverified",
    definition:
      "The test or the code it exercises changed since its verdict, or no report has covered it yet. Re-running the test settles it.",
    docs: "concepts/status/",
  },
  failing: {
    label: "failing",
    definition: "Its test was red in the last report.",
    docs: "concepts/status/",
  },
  todo: {
    label: "todo",
    definition: "A todo test: work not done yet, or behaviour with no test body written.",
    docs: "concepts/status/",
  },
  dark: {
    label: "dark file",
    definition: "A product file no test reaches by import.",
    docs: "concepts/status/#health",
  },
  style: {
    label: "style",
    definition:
      "A container's declared architecture: which layers it has and which may import which. The import graph is checked against it.",
    docs: "concepts/structure/#styles",
  },
  layer: {
    label: "layer",
    definition: "A component's role in its container's style, which decides what it may import.",
    docs: "concepts/structure/#styles",
  },
  system: {
    label: "Software System",
    definition: "The whole product: its containers, and the people and external systems around it.",
    docs: "concepts/structure/",
  },
  container: {
    label: "Container",
    definition: "A deployable or package, read from the project's manifests.",
    docs: "concepts/structure/",
  },
  component: {
    label: "Component",
    definition: "A module inside a container, read from its source tree.",
    docs: "concepts/structure/",
  },
  code: {
    label: "Code",
    definition: "A function, type or other symbol inside a component.",
  },
  dataShape: {
    label: "Data shape",
    definition: "A type that describes data: a struct, interface or record.",
  },
  person: {
    label: "Person",
    definition: "Someone who uses the system, declared in structure.toml.",
    docs: "concepts/structure/",
  },
  external: {
    label: "External System",
    definition: "A system outside this one that it talks to, declared in structure.toml.",
    docs: "concepts/structure/",
  },
} satisfies Record<string, GlossaryEntry>;

export type TermId = keyof typeof GLOSSARY;

/** The docs page for a term, when it has one. */
export function docsUrl(id: TermId): string | undefined {
  const e: GlossaryEntry = GLOSSARY[id];
  return e.docs ? DOCS_BASE + e.docs : undefined;
}

/** The term for each claim state, by its wire name. */
export const CLAIM_TERM = {
  proven: "proven",
  stale: "unverified",
  failing: "failing",
  planned: "todo",
} as const satisfies Record<ClaimState, TermId>;
