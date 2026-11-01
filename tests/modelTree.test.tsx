/** Todo claims for `ModelTree`. */
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ClaimMix, levelTitle, ModelTree } from "../src/widgets/model-tree/ModelTree";
import type { ScryModel } from "../src/entities/model/viewmodel";
import type { DerivedCounts } from "../src/entities/model/derived";

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

  it("Tints and letters a row changed since the base, A added and M modified, and dots a row holding changes below it", () => {
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
    const html = renderToStaticMarkup(
      <ModelTree
        model={model}
        previewable={new Set()}
        selected={null}
        expanded={new Set(["sys", "app"])}
        onSelectNode={() => {}}
        onToggle={() => {}}
        editor={undefined}
        activeNodeIds={new Set()}
        activeLevel={undefined}
        changes={{ nodes: new Map([["save", "added"], ["tags", "modified"]]), holding: new Set(["sys", "app"]), removed: 0 }}
      />,
    );
    const row = (name: string) => html.slice(html.lastIndexOf("data-rk", html.indexOf(`>${name}<`)), html.indexOf("data-rk", html.indexOf(`>${name}<`)));
    expect(row("save")).toContain('data-change="added"');
    expect(row("save")).toContain("text-emerald-700");
    expect(row("tags")).toContain('data-change="modified"');
    expect(row("App")).toContain('data-change="holding"');
  });
});
