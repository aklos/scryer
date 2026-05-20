/**
 * Mock model for the layer-1 viewer. Throwaway data — not persisted.
 * Spans all three C4 altitudes: system -> container -> component.
 *
 * Flat structure: all entries on the surface with optional `groupId`.
 * Groups are lightweight overlays — their region is derived from members.
 * Positions are seeded by `hydrateCells`.
 */

import type {
  Model,
  Surface,
  Entry,
  Kind,
  Link,
  Responsibility,
  Status,
} from "./viewmodel";
import { hydrateCells } from "./pack";

function r(status: Status, text: string, detail?: string): Responsibility {
  return detail ? { id: "", status, text, detail } : { id: "", status, text };
}

function lk(to: string, label: string): Link {
  return { to, label };
}

function E(
  kind: Kind,
  id: string,
  title: string,
  responsibilities: Responsibility[],
  opts: {
    childSurfaceId?: string;
    fulfills?: string;
    links?: Link[];
    technology?: string;
    description?: string;
    groupId?: string;
  } = {},
): Entry {
  const e: Entry = { id, title, kind, responsibilities };
  if (opts.childSurfaceId) e.childSurfaceId = opts.childSurfaceId;
  if (opts.fulfills) e.fulfills = opts.fulfills;
  if (opts.links) e.links = opts.links;
  if (opts.technology) e.technology = opts.technology;
  if (opts.description) e.description = opts.description;
  if (opts.groupId) e.groupId = opts.groupId;
  return e;
}

function P(
  id: string,
  title: string,
  description: string,
  links: Link[] = [],
): Entry {
  return { id, title, kind: "person", responsibilities: [], description, links };
}

/** External system — C4 `system` with the `external` flag set. No drill-in,
 * no status. Carries a technology tag (the product, e.g. "S3 Bucket") and
 * a short description of the role it plays for us. */
function X(
  id: string,
  title: string,
  technology: string,
  description: string,
): Entry {
  return {
    id,
    title,
    kind: "system",
    external: true,
    responsibilities: [],
    technology,
    description,
  };
}

/** Assign `<entryId>#<index>` ids to every responsibility. */
function assignIds(surface: Surface): void {
  for (const e of surface.entries) {
    e.responsibilities.forEach((resp, i) => {
      resp.id = `${e.id}#${i}`;
    });
  }
}

// --- component surfaces (leaves) -------------------------------------------

const cmpWeb: Surface = {
  id: "cmp-web",
  altitude: "component",
  entries: [
    E("component", "e-page-renderer", "renderPage", [
      r("verified", "Resolves the route to a Payload page"),
      r("verified", "Renders the page's layout blocks", "block components map 1:1 to Payload block types"),
    ], { fulfills: "e-web#0", links: [lk("e-cms", "fetches the page from")], groupId: "g-web-pages" }),
    E("component", "e-post-renderer", "renderPost", [
      r("verified", "Resolves a blog post by slug"),
      r("implemented", "Renders the post body and metadata"),
    ], { fulfills: "e-web#0", links: [lk("e-cms", "fetches the post from")], groupId: "g-web-pages" }),
    E("component", "e-site-nav", "SiteNav", [
      r("verified", "Renders the primary navigation"),
      r("removing", "Renders the legacy mega-menu"),
    ], { fulfills: "e-web#1", groupId: "g-web-components" }),
    E("component", "e-contact-form", "ContactForm", [
      r("implemented", "Collects the visitor's message"),
      r("planned", "Submits the message to the CMS"),
      r("placeholder", "Shows a confirmation state"),
      r("vagrant", "Logs failed submissions"),
    ], { fulfills: "e-web#2", links: [lk("e-cms", "posts the submission to")], groupId: "g-web-components" }),
  ],
  groups: [
    { id: "g-web-pages", name: "Pages", cell: { row: 0, col: 0 }, size: { cols: 1, rows: 1 } },
    { id: "g-web-components", name: "Components", cell: { row: 0, col: 0 }, size: { cols: 1, rows: 1 } },
  ],
};

const cmpCms: Surface = {
  id: "cmp-cms",
  altitude: "component",
  entries: [
    E("component", "e-page-coll", "Page", [
      r("verified", "Defines the page schema"),
      r("verified", "Holds the ordered layout blocks"),
      r("implemented", "Tracks draft and published status"),
    ], { fulfills: "e-cms#0", groupId: "g-cms-collections" }),
    E("component", "e-post-coll", "Post", [
      r("verified", "Defines the blog-post schema"),
      r("verified", "Holds the author, tags, and cover image"),
    ], { fulfills: "e-cms#0", groupId: "g-cms-collections" }),
    E("component", "e-media-coll", "Media", [
      r("implemented", "Defines the media schema"),
      r("implemented", "Generates resized image variants"),
      r("placeholder", "Purges orphaned files"),
    ], { fulfills: "e-cms#2", links: [lk("e-storage", "stores files in")], groupId: "g-cms-collections" }),
    E("component", "e-access-control", "accessControl", [
      r("verified", "Restricts admin access to editors"),
      r("implemented", "Hides draft content from the public API"),
    ], { fulfills: "e-cms#1", groupId: "g-cms-services" }),
    E("component", "e-form-hook", "submitContactForm", [
      r("planned", "Validates the submission payload"),
      r("planned", "Persists the message"),
      r("placeholder", "Notifies the team by email"),
    ], { fulfills: "e-cms#3", links: [lk("e-email", "sends notifications through")], groupId: "g-cms-services" }),
  ],
  groups: [
    { id: "g-cms-collections", name: "Collections", cell: { row: 0, col: 0 }, size: { cols: 1, rows: 1 } },
    { id: "g-cms-services", name: "Services", cell: { row: 0, col: 0 }, size: { cols: 1, rows: 1 } },
  ],
};

// --- container surface -------------------------------------------------------

const ctrSystem: Surface = {
  id: "ctr-system",
  altitude: "container",
  entries: [
    E("container", "e-web", "Web Frontend", [
      r("verified", "Server-renders pages and blog posts"),
      r("verified", "Routes between site sections"),
      r("implemented", "Renders a contact form"),
    ], {
      childSurfaceId: "cmp-web",
      fulfills: "e-website#2",
      links: [lk("e-cms", "fetches content from")],
      technology: "Next.js",
      description: "Public-facing website rendered server-side from CMS content",
      groupId: "g-app",
    }),
    E("container", "e-cms", "Payload CMS", [
      r("verified", "Serves a REST and GraphQL content API"),
      r("implemented", "Provides the editor admin panel"),
      r("implemented", "Manages uploaded media"),
      r("planned", "Receives contact-form submissions"),
    ], {
      childSurfaceId: "cmp-cms",
      fulfills: "e-website#1",
      links: [
        lk("e-database", "reads and writes content"),
        lk("e-storage", "uploads media to"),
        lk("e-email", "sends email via"),
      ],
      technology: "Node.js · Payload 3",
      description: "Headless CMS hosting content collections and the editor admin UI",
      groupId: "g-app",
    }),
    E("container", "e-database", "Database", [
      r("verified", "Persists content documents"),
      r("verified", "Serves content queries"),
    ], {
      fulfills: "e-website#0",
      technology: "MongoDB 7",
      description: "Stores all CMS content and uploaded-media metadata",
      groupId: "g-data",
    }),
  ],
  groups: [
    { id: "g-app", name: "Application", cell: { row: 0, col: 0 }, size: { cols: 1, rows: 1 } },
    { id: "g-data", name: "Data", cell: { row: 0, col: 0 }, size: { cols: 1, rows: 1 } },
  ],
};

// --- system surface (root) ---------------------------------------------------

const sysRoot: Surface = {
  id: "sys-root",
  altitude: "system",
  entries: [
    P("e-visitor", "Visitor", "Browses the public website — reads pages and blog posts", [
      lk("e-website", "reads pages and posts from"),
    ]),
    P("e-editor", "Content Editor", "Creates and edits pages, posts, and media in the admin panel", [
      lk("e-website", "manages content in"),
    ]),
    X("e-storage", "Cloud Storage", "S3 Bucket", "Stores uploaded media"),
    X("e-email", "Email Service", "Resend", "Delivers transactional email"),
    E("system", "e-website", "Company Website", [
      r("verified", "Publishes marketing pages and blog posts"),
      r("implemented", "Lets editors manage content without a developer"),
      r("verified", "Renders content for visitors"),
      r("planned", "Handles contact-form submissions"),
    ], {
      childSurfaceId: "ctr-system",
      links: [
        lk("e-storage", "stores media in"),
        lk("e-email", "sends email via"),
      ],
      technology: "Payload CMS · Next.js",
      description: "The marketing site plus its CMS — everything we own that the business runs on",
    }),
  ],
  groups: [],
};

const allSurfaces = [sysRoot, ctrSystem, cmpWeb, cmpCms];
allSurfaces.forEach(assignIds);

const surfaces: Record<string, Surface> = {};
for (const s of allSurfaces) surfaces[s.id] = hydrateCells(s);

export const mockModel: Model = {
  rootSurfaceId: "sys-root",
  surfaces,
};

export function surfaceTitle(model: Model, surfaceId: string): string {
  const s = model.surfaces[surfaceId];
  if (!s) return surfaceId;
  if (surfaceId === model.rootSurfaceId) return "System";
  for (const surf of Object.values(model.surfaces)) {
    const found = surf.entries.find((e) => e.childSurfaceId === surfaceId);
    if (found) return found.title;
  }
  return s.altitude;
}
