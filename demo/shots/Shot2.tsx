/**
 * Shot 2 — a claim is a test. The Rust test that proves one Ledger claim, in
 * the code font; its doc comment lifts off the code and settles as the claim
 * row the node page shows. Props: `lift`, 0 (comment in the code) to 1 (row
 * settled, code dimmed).
 */

import type { ReactNode } from "react";
import { interpolate } from "remotion";
import { ClaimRows, claim, nodeId } from "./app";
import { Fragment, Headline, INK, MONO, Stage } from "./stage";

const TEXT = "When a refund exceeds the captured amount, rejects it";

/** One token of code, coloured as the app's dark source peek would. */
const C = {
  doc: "#8b98a8",
  attr: "#e0b56f",
  kw: "#c4a7ff",
  fn: "#7dd3fc",
  plain: "#dde3ea",
  punct: "#8b98a8",
};
const t = (color: string, s: string) => <span style={{ color }}>{s}</span>;

const CODE: ReactNode[] = [
  <>{t(C.doc, `/// ${TEXT}`)}</>,
  <>{t(C.attr, "#[test]")}</>,
  <>
    {t(C.kw, "fn ")}
    {t(C.fn, "rejects_over_refund")}
    {t(C.punct, "() {")}
  </>,
  <>
    {t(C.kw, "    let mut ")}
    {t(C.plain, "ledger = ")}
    {t(C.fn, "open_in_memory")}
    {t(C.punct, "();")}
  </>,
];

export function Shot2({ lift }: { lift: number }) {
  const settled = interpolate(lift, [0.4, 1], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (
    <Stage light={{ x: 960, y: 560 }}>
      <Headline at={{ x: 120, y: 84 }}>A claim is a test.</Headline>
      {/* The thread from the comment's empty slot up to the row it became. */}
      <div
        style={{
          position: "absolute",
          left: 216,
          top: 432,
          width: 2,
          height: 140,
          opacity: settled,
          background: "linear-gradient(to bottom, rgba(242,244,248,0.35), rgba(242,244,248,0.04))",
        }}
      />
      <Fragment at={{ x: 120, y: 300 }} width={560} height={44} scale={3} style={{ opacity: settled }}>
        <div className="dark shot-grade shot-claims h-full bg-[var(--surface)] px-3 pt-[9px] text-[var(--text)]">
          <ClaimRows host={nodeId("Ledger")} rows={[{ claim: claim(TEXT), state: "proven" }]} />
        </div>
      </Fragment>
      <Fragment at={{ x: 120, y: 572 }} width={840} height={156} scale={2}>
        <div style={{ padding: "22px 32px", fontFamily: MONO, fontSize: 16, lineHeight: 1.75, color: INK.fg, whiteSpace: "pre" }}>
          {CODE.map((line, i) => (
            <div key={i} style={{ opacity: i === 0 ? 1 - settled * 0.85 : 1 - settled * 0.45 }}>
              {line}
            </div>
          ))}
        </div>
      </Fragment>
    </Stage>
  );
}
