/** Todo claims for `ModelTree`. */
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ClaimMix, levelTitle, ModelTree } from "../src/widgets/model-tree/ModelTree";
import type { ScryModel } from "../src/entities/model/viewmodel";
import type { DerivedCounts } from "../src/entities/model/derived";
import type { StyleViolation } from "../src/entities/model/health";

const counts = (c: Partial<DerivedCounts>): DerivedCounts => ({
  claims: 0, proven: 0, stale: 0, failing: 0, planned: 0,
  untestedComponents: 0, survivingMutants: 0, styleViolations: 0, darkFiles: 0, ...c,
});
const segments = (html: string) => [...html.matchAll(/var\(--claim-mix-(\w+)\)/g)].map((m) => m[1]);

describe("ModelTree", () => {
  it.todo("When the user types in the tree's filter, shows only rows whose name matches and their ancestors, with every surviving branch open");
  it.todo("Shows beside a collapsed row the number of todo claims in its subtree, and beside an open row only its own");

  it("Titles each level of the tree with what its rows are, in the words the map titles its levels with", () => {
    expect(levelTitle(["person", "system", "system"])).toBe("System context");
    expect(levelTitle(["container", "container"])).toBe("Containers");
    expect(levelTitle(["component", "symbol"])).toBe("Components");
    expect(levelTitle(["symbol", "symbol"])).toBe("Code");
  });

  it("Draws a subtree's claims as one bar: failing first, then outdated, todo and proven, each state present at least a sliver", () => {
    const html = renderToStaticMarkup(<ClaimMix counts={counts({ proven: 206, stale: 2, planned: 3 })} empty />);
    expect(segments(html)).toEqual(["stale", "planned", "proven"]);
    // Two changed claims among two hundred still get a visible share.
    const grow = [...html.matchAll(/flex-grow:\s*([\d.]+)/g)].map((m) => Number(m[1]));
    expect(grow[0] / grow.reduce((t, g) => t + g, 0)).toBeGreaterThan(0.05);
    expect(html).toContain("206 proven");
  });

  it("Draws the bar's bare track for code with no claims, and nothing for what holds none by nature", () => {
    const bare = renderToStaticMarkup(<ClaimMix counts={counts({})} empty />);
    expect(bare).toContain("var(--claim-mix-empty)");
    expect(segments(bare)).toEqual(["empty"]);
    const none = renderToStaticMarkup(<ClaimMix counts={undefined} empty={false} />);
    expect(none).not.toContain("claim-mix");
  });

  const store = new Map<string, string>();
  globalThis.localStorage ??= {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
  } as unknown as Storage;
  const model = {
    version: 1,
    nodes: [
      { id: "sys", kind: "system", name: "Notes" },
      { id: "app", kind: "container", name: "App", parentId: "sys" },
      { id: "save", kind: "component", name: "save", parentId: "app" },
      { id: "tags", kind: "component", name: "tags", parentId: "app" },
    ],
    links: [],
  } as unknown as ScryModel;
  const violation = { node: "save" } as StyleViolation;
  const tree = (expanded: string[]) =>
    renderToStaticMarkup(
      <ModelTree
        model={model}
        previewable={new Set()}
        selected={null}
        expanded={new Set(expanded)}
        onSelectNode={() => {}}
        onToggle={() => {}}
        editor={undefined}
        activeNodeIds={new Set()}
        activeLevel={undefined}
        structural={new Map([["sys", [violation, violation]], ["app", [violation, violation]], ["save", [violation, violation]]])}
        changes={{ nodes: new Map([["save", "added"], ["tags", "modified"]]), holding: new Set(["sys", "app"]), removed: 0 }}
      />,
    );
  const rowIn = (html: string, name: string) => {
    const at = html.indexOf(`>${name}<`);
    const end = html.indexOf("data-rk", at);
    return html.slice(html.lastIndexOf("data-rk", at), end === -1 ? undefined : end);
  };

  it("Tints and letters a row changed since the base, A added and M modified, and dots a collapsed row holding changes below it", () => {
    const open = tree(["sys", "app"]);
    expect(rowIn(open, "save")).toContain('data-change="added"');
    expect(rowIn(open, "save")).toContain("text-emerald-700");
    expect(rowIn(open, "tags")).toContain('data-change="modified"');
    expect(rowIn(open, "App")).not.toContain('data-change="holding"');
    expect(rowIn(tree(["sys"]), "App")).toContain('data-change="holding"');
  });

  it("Marks violations charged to a row with a chip, open or closed, and violations only below a closed row with a bare number", () => {
    const open = tree(["sys", "app"]);
    expect(rowIn(open, "save")).toContain('data-structural-mark="here"');
    expect(rowIn(open, "save")).toContain('data-structural-count="2"');
    expect(rowIn(open, "save")).not.toContain("lucide-flag");
    expect(rowIn(open, "App")).not.toContain("data-structural-count");
    const closed = tree(["sys"]);
    expect(rowIn(closed, "App")).toContain('data-structural-mark="below"');
    expect(rowIn(closed, "App")).toContain('data-structural-count="2"');
    // Each says what it means where it is read: on hover.
    expect(rowIn(open, "save")).toContain("own code");
    expect(rowIn(closed, "App")).toContain("Expand to find them");
  });

  it("Keeps every row's change and violation marks in their own fixed columns, empty or not", () => {
    const html = tree(["sys", "app"]);
    for (const name of ["Notes", "App", "save", "tags"]) {
      expect(rowIn(html, name)).toContain('data-slot="change"');
      expect(rowIn(html, name)).toContain('data-slot="violations"');
    }
  });
});
