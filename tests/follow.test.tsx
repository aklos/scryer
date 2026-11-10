/** Claims for following an agent session: mechanical tracking of its work, paused by your own moves and by
 *  accepting an agent's show. */
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import {
  dueMove,
  followReducer,
  followTarget,
  offerFor,
  SETTLE_MS,
  stepLine,
  type SessionBrief,
} from "../src/features/sessions/follow";
import type { SessionStep } from "../src/features/sessions/useSessions";
import { FollowSegments, type FollowBar } from "../src/widgets/top-bar/Powerline";
import { FollowOffer } from "../src/features/sessions/FollowOffer";

const claim = { respId: "r", nodeId: "n", statement: "Rejects a refund larger than the capture", file: "f", line: 1, state: "proven" as const };
const edit = (at: number, file: string, nodeId?: string, edits = 1): SessionStep => ({ kind: "edit", at, file, nodeId, edits });

describe("follow", () => {
  it("Follows a session to the node of its latest claim step or edit in a component, and nowhere before it has one", () => {
    expect(followTarget({})).toBeNull();
    expect(followTarget({ steps: [edit(1, "README.md")] })).toBeNull();
    expect(followTarget({ steps: [edit(1, "src/a.ts", "a"), edit(2, "src/b.ts", "b"), edit(3, "README.md"), { kind: "stop", at: 4 }] })).toBe("b");
    expect(followTarget({ steps: [edit(1, "src/a.ts", "a"), { kind: "verdict", at: 2, claim }] })).toBe("n");
    expect(stepLine(edit(1, "src/a.ts", "a", 3), (id) => (id === "a" ? "drafts" : id))).toBe("edited src/a.ts in drafts ×3");
  });

  it("While following, moves the view to the session's latest node only once it has held there for the settle time", () => {
    const at = 10_000;
    expect(dueMove({ target: "b", since: at, wentTo: "a", now: at + SETTLE_MS - 1 })).toBeNull();
    expect(dueMove({ target: "b", since: at, wentTo: "a", now: at + SETTLE_MS })).toBe("b");
    expect(dueMove({ target: "b", since: at, wentTo: "b", now: at + 60_000 })).toBeNull();
    expect(dueMove({ target: null, since: at, wentTo: "a", now: at + 60_000 })).toBeNull();
  });

  it("When the developer goes elsewhere themself, pauses the follow, and resuming goes back to the session's latest node", () => {
    const on = followReducer(null, { type: "follow", session: "s1" });
    expect(on).toEqual({ session: "s1", paused: false });
    const away = followReducer(on, { type: "away" });
    expect(away).toEqual({ session: "s1", paused: true });
    expect(followReducer(away, { type: "resume" })).toEqual({ session: "s1", paused: false });
    expect(followReducer(null, { type: "away" })).toBeNull();
  });

  it("When a show is accepted, pauses the follow", () => {
    expect(followReducer({ session: "s1", paused: false }, { type: "show" })).toEqual({ session: "s1", paused: true });
    expect(followReducer(null, { type: "show" })).toBeNull();
  });

  it("If the followed session stops being live, then stops following", () => {
    expect(followReducer({ session: "s1", paused: true }, { type: "ended" })).toBeNull();
    expect(followReducer({ session: "s1", paused: false }, { type: "stop" })).toBeNull();
  });

  it("Offers to follow a live session the first time it orients or edits, never one already offered or followed", () => {
    const brief = (over: Partial<SessionBrief> = {}): SessionBrief => ({ session: "s2", live: true, touched: 0, task: "fix export", ...over });
    const none = { offered: new Set<string>(), follow: null };
    expect(offerFor(brief(), none)).toBe(true);
    expect(offerFor(brief({ task: undefined, touched: 1 }), none)).toBe(true);
    expect(offerFor(brief({ task: undefined }), none), "bound to a prompt, no work yet").toBe(false);
    expect(offerFor(brief({ live: false }), none)).toBe(false);
    expect(offerFor(brief(), { offered: new Set(["s2"]), follow: null })).toBe(false);
    expect(offerFor(brief(), { offered: new Set<string>(), follow: { session: "s2", paused: true } })).toBe(false);
    expect(offerFor(brief(), { offered: new Set<string>(), follow: { session: "s1", paused: false } })).toBe(true);
  });
});

describe("follow bar", () => {
  const bar = (paused: boolean): FollowBar => ({ title: "“fix export”", paused, onResume: () => {}, onStop: () => {} });

  it("While following, names the session and offers to stop; while paused, offers Resume", () => {
    const live = renderToStaticMarkup(<FollowSegments follow={bar(false)} />);
    expect(live).toContain("fix export");
    expect(live).toContain("stop");
    expect(live).not.toContain("resume");
    const paused = renderToStaticMarkup(<FollowSegments follow={bar(true)} />);
    expect(paused).toContain("paused");
    expect(paused).toContain("resume");
    expect(paused).toContain("stop");
  });
});

describe("follow offer", () => {
  it("Names the session that started working and offers to follow it", () => {
    const html = renderToStaticMarkup(
      <FollowOffer offer={{ session: "s2", live: true, touched: 1, task: "fix export" }} onFollow={() => {}} onDismiss={() => {}} />,
    );
    expect(html).toContain("fix export");
    expect(html).toContain("Follow");
    expect(renderToStaticMarkup(<FollowOffer offer={null} onFollow={() => {}} onDismiss={() => {}} />)).toBe("");
  });
});
