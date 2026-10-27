import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { WorktreesSection } from "../src/pages/landing/WorktreesSection";
import { standing, type WorktreeWork, type Worktrees } from "../src/features/worktrees/useWorktrees";

const work = (over: Partial<WorktreeWork> = {}): WorktreeWork => ({
  path: "/repo",
  branch: "main",
  current: false,
  ahead: 0,
  behind: 0,
  files: [],
  components: [],
  claims: [],
  ready: false,
  ...over,
});

const claim = (id: string, state: "proven" | "stale" | "failing" | "planned") => ({
  respId: id,
  nodeId: "store",
  statement: `claim ${id}`,
  file: "src/store.rs",
  line: 1,
  state,
});

describe("WorktreesSection", () => {
  it("Says per worktree whether it is merged into its base, changed nothing, touches no claims, is ready to merge, or counts its open claims by state", () => {
    expect(standing(work())[0].label).toBe("no changes");
    expect(standing(work({ base: "main", behind: 14 }))[0].label).toBe("merged · 14 behind main");
    expect(standing(work({ base: "main", ahead: 1, behind: 14, files: ["a"] }))[0].label).toBe("touches no claims");
    expect(standing(work({ files: ["a"], ready: true }))[0].label).toBe("touches no claims");
    expect(standing(work({ files: ["a"], ready: true, claims: [claim("a", "proven")] }))[0].label).toBe("ready to merge");
    expect(standing(work({ files: ["a"], claims: [claim("a", "failing"), claim("b", "stale")] })).map((p) => p.label)).toEqual(["1 failing", "1 unverified"]);
    expect(standing(work({ files: ["a"], claims: [claim("a", "planned"), claim("b", "stale"), claim("c", "proven")] })).map((p) => [p.label, p.tone])).toEqual([["1 unverified", "stale"], ["1 todo", "planned"]]);
    expect(standing(work({ error: "git failed" }))[0].label).toBe("could not be read");
  });

  it("When the project has several worktrees, shows each on one line with its branch, standing and commits against its base, and a line counting the components more than one changed", () => {
    const data: Worktrees = {
      worktrees: [
        work({ current: true }),
        work({
          path: "/wt/a",
          branch: "feature",
          base: "main",
          ahead: 2,
          behind: 1,
          files: ["src/store.rs"],
          components: [{ id: "store", name: "Store" }],
          claims: [claim("a", "stale"), claim("b", "proven")],
        }),
      ],
      overlaps: [{ component: { id: "store", name: "Store" }, worktrees: ["/repo", "/wt/a"] }],
    };
    const html = renderToStaticMarkup(<WorktreesSection data={data} onOpenWorktree={() => {}} onOpenClaim={() => {}} />);
    expect(html).toContain("feature");
    expect(html).toContain("↑2 ↓1 main");
    expect(html).toContain("1 unverified");
    expect(html).not.toContain("claim a");
    expect(html).toContain("1 component changed in more than one worktree");
  });

  it("If the project has a single checkout, then shows nothing", () => {
    const html = renderToStaticMarkup(
      <WorktreesSection data={{ worktrees: [work()], overlaps: [] }} onOpenWorktree={() => {}} onOpenClaim={() => {}} />,
    );
    expect(html).toBe("");
  });
});
