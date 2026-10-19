/**
 * Declares the docs site's content collection for Astro, using Starlight's
 * docs loader and schema.
 */

import { defineCollection } from "astro:content";
import { docsSchema } from "@astrojs/starlight/schema";
import { docsLoader } from "@astrojs/starlight/loaders";

export const collections = {
  docs: defineCollection({ loader: docsLoader(), schema: docsSchema() }),
};
