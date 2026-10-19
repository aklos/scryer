/**
 * The film's captions: a sliding bottom headline behind an accent bar, or the
 * centred brand card for the cold open and the close. Pure in the frame —
 * every motion is an interpolation from the frame the caption appeared.
 */

import { Easing, Img, interpolate, staticFile, useCurrentFrame } from "remotion";
import qr from "../repo-qr.png";
import type { Caption as CaptionState } from "./state";
import "./caption.css";

const snap = Easing.bezier(0.16, 1, 0.3, 1);

/** 0 → 1 over `frames`, starting `delay` frames after `since`. */
function rise(frame: number, since: number, delay: number, frames: number): number {
  return interpolate(frame - since, [delay, delay + frames], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: snap,
  });
}

export function Caption({ caption }: { caption: CaptionState | null }) {
  const frame = useCurrentFrame();
  if (!caption) return null;

  if (caption.style === "title") {
    const t = rise(frame, caption.since, 4, 22);
    const isClose = caption.sub?.includes("/");
    return (
      <>
        <div className="cap-veil" style={{ opacity: rise(frame, caption.since, 0, 12) }} />
        <div className="cap-center" style={{ opacity: t, transform: `translateY(${(1 - t) * 24}px)` }}>
          <div className="cap-brand">
            <Img className="cap-logo" src={staticFile("logo.png")} />
            scryer
          </div>
          <p className="cap-hook">{caption.text}</p>
          {isClose && <p className="cap-url">{caption.sub}</p>}
          {isClose && <Img className="cap-qr" src={qr} />}
        </div>
      </>
    );
  }

  const bar = rise(frame, caption.since, 10, 18);
  const slide = rise(frame, caption.since, 10, 21);
  return (
    <>
      <div className="cap-scrim" style={{ opacity: rise(frame, caption.since, 0, 12) }} />
      <div className="cap-overlay">
        <div className="cap-head" style={{ opacity: slide, transform: `translateY(${(1 - slide) * 40}px)` }}>
          <div className="cap-bar" style={{ transform: `scaleY(${bar})` }} />
          <h2 className="cap-headline">{caption.text}</h2>
        </div>
      </div>
    </>
  );
}
