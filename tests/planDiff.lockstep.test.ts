/**
 * Lockstep with the Rust diff engine.
 *
 * planDiff.ts is a hand-maintained port of `scryer-core/src/diff.rs`. The
 * fixtures under `crates/scryer-core/tests/lockstep/` hold model pairs plus
 * the diff Rust computed for them (pinned by the `lockstep` cargo test);
 * this suite asserts the TypeScript port computes the identical diff, so a
 * change to either side that forgets the other fails loudly instead of
 * letting the canvas and `get_pending` quietly disagree.
 *
 * Regenerate after an intentional diff.rs change:
 *   UPDATE_LOCKSTEP=1 cargo test -p scryer-core --test lockstep && pnpm test
 */

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { planDiff, type ElementChange, type ModelDiff } from "../src/planDiff";
import type { ScryModel } from "../src/viewmodel";

const FIXTURE_DIR = new URL("../crates/scryer-core/tests/lockstep/", import.meta.url).pathname;

/** Rust iterates id-sorted BTreeMaps, the port iterates model order — element
 *  order is not part of the contract, so compare order-insensitively. The
 *  changes WITHIN one element are pushed in a fixed shared order and stay
 *  significant. */
function canonical(diff: ModelDiff): ElementChange[] {
  const key = (c: ElementChange) => `${c.kind}\0${c.id}\0${c.ownerId ?? ""}`;
  // Round-trip to drop `undefined` fields the way serialization would.
  return (JSON.parse(JSON.stringify(diff.changes)) as ElementChange[]).sort((a, b) =>
    key(a) < key(b) ? -1 : key(a) > key(b) ? 1 : 0,
  );
}

describe("planDiff matches diff.rs on the lockstep fixtures", () => {
  const files = readdirSync(FIXTURE_DIR).filter((f) => f.endsWith(".json"));
  expect(files.length).toBeGreaterThan(0);

  for (const file of files) {
    it(file.replace(/\.json$/, ""), () => {
      const { from, to, diff } = JSON.parse(readFileSync(join(FIXTURE_DIR, file), "utf8")) as {
        from: ScryModel;
        to: ScryModel;
        diff: ModelDiff;
      };
      expect(canonical(planDiff(from, to))).toEqual(canonical(diff));
    });
  }
});
