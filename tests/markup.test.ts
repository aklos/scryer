/**
 * Statement display markup (`src/markup.tsx`) — the markdown-lite subset
 * agents author into claim statements (`**bold**` scan anchors; `_…_` is a
 * retired clause marker that still strips clean but renders plain). The
 * invariants that matter: strip(parse) round-trips to plain text, malformed
 * markers degrade to literal text (never an error), and identifier
 * underscores never read as markers.
 */
import { describe, expect, it } from "vitest";
import { earsTokenize, hasMarkup, parseMarkup, serializeEars, stripMarkup } from "../src/markup";

describe("parseMarkup", () => {
  it("splits a marked statement into anchor / plain segments (legacy clause markers go plain)", () => {
    expect(parseMarkup("**When** _a callback arrives_, **append** an event")).toEqual([
      { text: "When", bold: true },
      { text: " " },
      { text: "a callback arrives" },
      { text: ", " },
      { text: "append", bold: true },
      { text: " an event" },
    ]);
  });

  it("returns one plain segment for an unmarked statement", () => {
    expect(parseMarkup("Receive inbound WhatsApp messages")).toEqual([
      { text: "Receive inbound WhatsApp messages" },
    ]);
  });

  it("renders unmatched markers literally", () => {
    expect(parseMarkup("a ** stray and _dangling")).toEqual([{ text: "a ** stray and _dangling" }]);
  });

  it("never reads identifier underscores as markers", () => {
    expect(parseMarkup("claim the claimed_message_ledger id")).toEqual([
      { text: "claim the claimed_message_ledger id" },
    ]);
  });

  it("consumes a legacy underscore marker only at a word boundary", () => {
    expect(parseMarkup("**If** _the id is claimed_, **drop** it")).toEqual([
      { text: "If", bold: true },
      { text: " " },
      { text: "the id is claimed" },
      { text: ", " },
      { text: "drop", bold: true },
      { text: " it" },
    ]);
  });

  it("bolds multiple response verbs", () => {
    expect(parseMarkup("**ack** the webhook and **hand** the turn to Emily")).toEqual([
      { text: "ack", bold: true },
      { text: " the webhook and " },
      { text: "hand", bold: true },
      { text: " the turn to Emily" },
    ]);
  });
});

describe("stripMarkup", () => {
  it("round-trips a marked statement to its plain text", () => {
    expect(stripMarkup("**When** _a callback arrives_, **append** an event")).toBe(
      "When a callback arrives, append an event",
    );
  });

  it("is the identity on unmarked statements", () => {
    const plain = "Deduplicate Meta's at-least-once webhook retries";
    expect(stripMarkup(plain)).toBe(plain);
  });

  it("leaves unmatched markers and identifier underscores alone", () => {
    expect(stripMarkup("a ** stray, a _dangling, and a claimed_message id")).toBe(
      "a ** stray, a _dangling, and a claimed_message id",
    );
  });
});

describe("earsTokenize (the positional pass)", () => {
  it("anchors an event-driven claim: keyword bold, response verb bold", () => {
    expect(earsTokenize("When a callback arrives, append an event")).toEqual([
      { text: "When", bold: true },
      { text: " a callback arrives, " },
      { text: "append", bold: true },
      { text: " an event" },
    ]);
  });

  it("bolds `then` after an If clause", () => {
    expect(earsTokenize("If the signature is invalid, then reject the request")).toEqual([
      { text: "If", bold: true },
      { text: " the signature is invalid, " },
      { text: "then", bold: true },
      { text: " " },
      { text: "reject", bold: true },
      { text: " the request" },
    ]);
  });

  it("stacks While + when, in grammar order only", () => {
    const segs = earsTokenize("While a reconcile runs, when a claim is edited, queue the edit");
    expect(segs.filter((s) => s.bold).map((s) => s.text)).toEqual(["While", "when", "queue"]);
    expect(segs.map((s) => s.text).join("")).toBe(
      "While a reconcile runs, when a claim is edited, queue the edit",
    );
  });

  it("treats a ubiquitous claim as verb-led: first word bold, rest plain", () => {
    expect(earsTokenize("Authenticate every inbound POST")).toEqual([
      { text: "Authenticate", bold: true },
      { text: " every inbound POST" },
    ]);
  });

  it("does not mistake 'Whenever' for a keyword", () => {
    expect(earsTokenize("Whenever possible, batch the writes")[0]).toEqual({
      text: "Whenever",
      bold: true,
    });
  });

  it("anchors only the keyword of an unfinished clause (mid-keystroke feedback)", () => {
    expect(earsTokenize("When a message arr")).toEqual([
      { text: "When", bold: true },
      { text: " a message arr" },
    ]);
  });
});

describe("serializeEars", () => {
  it("mints anchor markers that stripMarkup round-trips to the input", () => {
    const plain = "When a callback arrives, append an event";
    const minted = serializeEars(plain);
    expect(minted).toBe("**When** a callback arrives, **append** an event");
    expect(stripMarkup(minted)).toBe(plain);
  });

  it("keeps odd spacing intact when minting", () => {
    const minted = serializeEars("When a callback arrives , append an event");
    expect(stripMarkup(minted)).toBe("When a callback arrives , append an event");
    expect(hasMarkup(minted)).toBe(true);
  });

  it("bolds only the leading verb of a ubiquitous claim", () => {
    expect(serializeEars("Authenticate every inbound POST")).toBe(
      "**Authenticate** every inbound POST",
    );
  });
});
