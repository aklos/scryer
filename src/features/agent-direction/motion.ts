/**
 * The moves, played on the pane that shows the map or the wiki: a snapshot of what was on screen animates out
 * while the new view animates in, then what the agent pointed at is flashed.
 *
 * Each move ends exactly on the resting layout: only transform, opacity and shadow animate, the live view keeps
 * its own layout throughout, and every overlay (snapshot, ghost card, sweep, ring) is a separate absolutely placed
 * element removed at the end. The live view and the snapshot stay on their own compositing layer for good: text
 * renders differently on a layer than off one, so a layer dropped when the move ends makes the text visibly jump. The new view is shown only once it has settled (the map's level built, fitted and
 * its edges mounted), so nothing pops in after the move. The flash starts once the move is over.
 */

import type { Move, Place } from "./movement";

const EASE = "cubic-bezier(.2,.7,.2,1)";
const SLIDE = "cubic-bezier(.25,.8,.25,1)";
/** The pause between a move landing and the flash, so the eye is at rest when the flash starts. */
const SETTLE_GAP = 160;

export interface Flash {
  /** Claim ids whose rows on the page flash. */
  claims: string[];
  /** Page fields that flash: `"description"`. */
  fields: string[];
  /** Nodes whose cards on the map flash, beside the target's. */
  nodes: string[];
}

export interface DirectPlan {
  /** The pane: its one child not marked `data-motion` is the live view. */
  stage: HTMLElement;
  move: Move;
  to: Place;
  /** The node pointed at. */
  target: string;
  /** Commit the new place synchronously (the caller wraps its state updates in `flushSync`). */
  apply: () => void;
  /** Whether the new view has settled enough to show. */
  ready: () => boolean;
  /** A node's name, for the card that morphs into its page. */
  name: (id: string) => string;
  flash: Flash;
  /** The system asks for reduced motion: cut, and flash without movement. */
  reduced: boolean;
}

type Rect = { x: number; y: number; w: number; h: number };

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
const frame = () => new Promise<void>((r) => requestAnimationFrame(() => r()));

async function waitFor(pred: () => boolean, timeout = 1200): Promise<void> {
  const until = performance.now() + timeout;
  while (!pred() && performance.now() < until) await frame();
}

function liveView(stage: HTMLElement): HTMLElement | null {
  return Array.from(stage.children).find((c): c is HTMLElement => !(c as HTMLElement).dataset.motion) ?? null;
}

function rel(el: Element, stage: HTMLElement): Rect {
  const r = el.getBoundingClientRect();
  const s = stage.getBoundingClientRect();
  return { x: r.left - s.left, y: r.top - s.top, w: r.width, h: r.height };
}

const box = (r: Rect) => ({ left: `${r.x}px`, top: `${r.y}px`, width: `${r.w}px`, height: `${r.h}px` });
const center = (r: Rect) => ({ x: r.x + r.w / 2, y: r.y + r.h / 2 });

function cardOf(root: ParentNode, id: string): Element | null {
  return root.querySelector(`.react-flow__node[data-id="${CSS.escape(id)}"]`);
}

function overlay(stage: HTMLElement, kind: string, z: number, className = ""): HTMLDivElement {
  const el = document.createElement("div");
  el.dataset.motion = kind;
  el.setAttribute("aria-hidden", "true");
  if (className) el.className = className;
  Object.assign(el.style, { position: "absolute", pointerEvents: "none", zIndex: String(z) });
  stage.appendChild(el);
  return el;
}

/** A still copy of the live view laid over it, its scroll positions kept, at stacking order `z` (−1 under the
 *  live view, 1 over it). */
function snapshot(stage: HTMLElement, live: HTMLElement, z: number): HTMLElement {
  const snap = live.cloneNode(true) as HTMLElement;
  snap.dataset.motion = "snapshot";
  snap.setAttribute("aria-hidden", "true");
  snap.inert = true;
  // On its own layer from its first frame, like the live view (see `[data-direct-stage]` in index.css).
  Object.assign(snap.style, { position: "absolute", inset: "0", pointerEvents: "none", zIndex: String(z), willChange: "transform, opacity" });
  stage.appendChild(snap);
  const a = document.createTreeWalker(live, NodeFilter.SHOW_ELEMENT);
  const b = document.createTreeWalker(snap, NodeFilter.SHOW_ELEMENT);
  for (let x = a.nextNode(), y = b.nextNode(); x && y; x = a.nextNode(), y = b.nextNode()) {
    const from = x as HTMLElement;
    if (from.scrollTop || from.scrollLeft) {
      (y as HTMLElement).scrollTop = from.scrollTop;
      (y as HTMLElement).scrollLeft = from.scrollLeft;
    }
  }
  return snap;
}

function play(el: Element, keyframes: Keyframe[], ms: number, o: { delay?: number; ease?: string } = {}) {
  return el
    .animate(keyframes, { duration: ms, delay: o.delay ?? 0, easing: o.ease ?? EASE, fill: "both" })
    .finished.then(() => undefined)
    .catch(() => undefined);
}
const fade = (el: Element, from: number, to: number, ms: number, delay = 0) =>
  play(el, [{ opacity: from }, { opacity: to }], ms, { delay, ease: "linear" });

/** Play the move from what is on screen to `plan.to`, then flash what was pointed at. */
export async function direct(plan: DirectPlan): Promise<void> {
  const { stage, move, reduced } = plan;
  const old = liveView(stage);
  if (!old || reduced || move.kind === "none") {
    plan.apply();
    await waitFor(plan.ready);
    if (move.kind !== "none" && !reduced) await sleep(SETTLE_GAP);
    flash(plan);
    return;
  }

  // What must be measured on the old view, before it goes.
  const pane = rel(stage, stage);
  const wiki = plan.to.view === "wiki";
  // A move through a card on the old map needs that card on screen; without it, hop.
  const throughOldCard = (move.kind === "push" && !wiki) || (move.kind === "morph" && wiki);
  const viaOld = throughOldCard && move.via ? cardOf(old, move.via) : null;
  const kind = throughOldCard && !viaOld ? "hop" : move.kind;
  const viaRect = viaOld ? rel(viaOld, stage) : null;

  // The snapshot sits under the live view only where the new page slides in over it.
  const snap = snapshot(stage, old, kind === "push" && wiki ? -1 : 1);
  plan.apply();
  const live = liveView(stage);
  if (!live) {
    snap.remove();
    return;
  }
  live.style.opacity = "0";
  await waitFor(plan.ready);
  if (wiki) revealFirst(live, plan.flash);
  // Let the new view lay out and paint under the snapshot first: a move started in the same frame as a heavy page's
  // first layout loses its opening frames and appears to start partway.
  await frame();
  await frame();
  live.style.opacity = "";

  const lifted = "-1px 0 0 var(--border), -10px 0 24px -8px rgb(0 0 0 / 0.28)";
  const steps: Promise<void>[] = [];
  let ghost: HTMLDivElement | null = null;
  if (kind === "morph" && wiki && viaRect) {
    // The card opens into its page.
    ghost = overlay(stage, "ghost", 2, "direct-ghost");
    ghost.textContent = plan.name(move.via!);
    const T = 260;
    steps.push(
      fade(snap, 1, 0, T * 0.4),
      play(ghost, [box(viaRect), box(pane)], T * 0.75),
      fade(ghost, 1, 0, T * 0.35, T * 0.65),
      fade(live, 0, 1, T * 0.35, T * 0.65),
    );
  } else if (kind === "morph") {
    // The page folds back into its card on the map.
    const card = cardOf(live, move.via!);
    const to = card ? rel(card, stage) : null;
    const T = 260;
    if (to) {
      ghost = overlay(stage, "ghost", 2, "direct-ghost");
      ghost.textContent = plan.name(move.via!);
      steps.push(
        fade(snap, 1, 0, T * 0.3),
        fade(ghost, 0, 1, T * 0.2),
        play(ghost, [box(pane), box(to)], T * 0.8, { delay: T * 0.15 }),
        fade(ghost, 1, 0, T * 0.15, T * 0.9),
        fade(live, 0, 1, T * 0.6, T * 0.4),
      );
    } else {
      steps.push(fade(snap, 1, 0, T * 0.5), fade(live, 0, 1, T * 0.5, T * 0.3));
    }
  } else if (wiki && kind === "push") {
    // Into a child page: it slides in from the right, over its parent, the way its crumb is added.
    const T = 240;
    live.style.boxShadow = lifted;
    steps.push(
      play(snap, [{ transform: "none", opacity: 1 }, { transform: "translateX(-30px)", opacity: 0.35 }], T, { ease: SLIDE }),
      play(live, [{ transform: "translateX(100%)" }, { transform: "none" }], T, { ease: SLIDE }),
    );
  } else if (wiki && kind === "pull") {
    // Out to a parent: the page slides off to the right and uncovers its parent.
    const T = 240;
    snap.style.boxShadow = lifted;
    steps.push(
      play(snap, [{ transform: "none" }, { transform: "translateX(100%)" }], T, { ease: SLIDE }),
      play(live, [{ transform: "translateX(-30px)", opacity: 0.35 }, { transform: "none", opacity: 1 }], T, { ease: SLIDE }),
    );
  } else if (wiki) {
    // Elsewhere: off the breadcrumb's axis, so vertical.
    const T = 200;
    steps.push(
      play(snap, [{ transform: "none", opacity: 1 }, { transform: "translateY(-12px)", opacity: 0 }], T * 0.5, { ease: "ease-in" }),
      play(live, [{ transform: "translateY(16px)", opacity: 0 }, { transform: "none", opacity: 1 }], T * 0.65, { delay: T * 0.35 }),
    );
  } else if (kind === "push" && viaRect) {
    // Into a card: the camera dives into it and the inner level settles in.
    const T = 220;
    const c = center(viaRect);
    const b = center(pane);
    snap.style.transformOrigin = `${c.x}px ${c.y}px`;
    live.style.transformOrigin = `${b.x}px ${b.y}px`;
    steps.push(
      play(snap, [{ transform: "none" }, { transform: `translate(${b.x - c.x}px, ${b.y - c.y}px) scale(${pane.w / viaRect.w})` }], T, {
        ease: "cubic-bezier(.4,0,.7,1)",
      }),
      fade(snap, 1, 0, T * 0.6, T * 0.2),
      play(live, [{ transform: "scale(.9)" }, { transform: "none" }], T * 0.6, { delay: T * 0.4 }),
      fade(live, 0, 1, T * 0.5, T * 0.4),
    );
  } else if (kind === "pull") {
    // Out of the level: it collapses into its card among its siblings.
    const T = 220;
    const card = move.via ? cardOf(live, move.via) : null;
    const to = card ? rel(card, stage) : { x: pane.w * 0.45, y: pane.h * 0.45, w: pane.w * 0.1, h: pane.h * 0.1 };
    const c = center(to);
    const b = center(pane);
    snap.style.transformOrigin = `${b.x}px ${b.y}px`;
    live.style.transformOrigin = `${c.x}px ${c.y}px`;
    steps.push(
      play(snap, [{ transform: "none" }, { transform: `translate(${c.x - b.x}px, ${c.y - b.y}px) scale(${to.w / pane.w})` }], T, {
        ease: "cubic-bezier(.3,0,.2,1)",
      }),
      fade(snap, 1, 0, T * 0.6, T * 0.15),
      play(live, [{ transform: "scale(1.12)" }, { transform: "none" }], T * 0.75, { delay: T * 0.25 }),
      fade(live, 0, 1, T * 0.55, T * 0.25),
    );
  } else {
    // A hop between levels neither holds: shrink out, grow in.
    const T = 200;
    steps.push(
      play(snap, [{ transform: "none" }, { transform: "scale(.9)" }], T * 0.5, { ease: "ease-in" }),
      fade(snap, 1, 0, T * 0.45),
      play(live, [{ transform: "scale(.9)" }, { transform: "none" }], T * 0.6, { delay: T * 0.4 }),
      fade(live, 0, 1, T * 0.5, T * 0.4),
    );
  }
  await Promise.all(steps);

  snap.remove();
  ghost?.remove();
  live.getAnimations().forEach((a) => a.cancel());
  live.style.boxShadow = "";
  live.style.transformOrigin = "";
  await sleep(SETTLE_GAP);
  flash(plan);
}

/** Scroll the page so the first thing to flash is in view, before the page is shown. */
function revealFirst(live: HTMLElement, f: Flash) {
  const first = flashTargets(live, f)[0];
  first?.scrollIntoView({ block: "center", behavior: "instant" as ScrollBehavior });
}

function flashTargets(live: HTMLElement, f: Flash): Element[] {
  return [
    ...f.fields.map((k) => live.querySelector(`[data-field="${CSS.escape(k)}"]`)),
    // A claim's row carries `resp-<id>` as its element id (the claims section's `respElementId`), or, while the
    // section edits, `data-erow`.
    ...f.claims.map((id) => live.querySelector(`[id="resp-${CSS.escape(id)}"], [data-erow="${CSS.escape(id)}"]`)),
  ].filter((e): e is Element => !!e);
}

/** A wash sweeps each flashed field or claim on the page; on the map, a ring pulses on each flashed card. */
function flash(plan: DirectPlan) {
  const { stage, reduced } = plan;
  const live = liveView(stage);
  if (!live) return;
  if (plan.to.view === "wiki") {
    flashTargets(live, plan.flash).forEach((el, i) => {
      const sweep = overlay(stage, "sweep", 3, "direct-sweep");
      Object.assign(sweep.style, box(rel(el, stage)));
      const done = reduced
        ? play(sweep, [{ opacity: 1 }, { opacity: 1, offset: 0.35 }, { opacity: 0 }], 650, { delay: i * 70, ease: "ease-out" })
        : play(
            sweep,
            [
              { transform: "scaleX(0)", opacity: 1 },
              { transform: "scaleX(1)", opacity: 1, offset: 0.35 },
              { transform: "scaleX(1)", opacity: 0 },
            ],
            650,
            { delay: i * 70 },
          );
      void done.then(() => sweep.remove());
    });
    return;
  }
  [plan.target, ...plan.flash.nodes].forEach((id, i) => {
    const card = cardOf(live, id);
    if (!card) return;
    const ring = overlay(stage, "ring", 3, "direct-ring");
    Object.assign(ring.style, box(rel(card, stage)));
    const keyframes: Keyframe[] = reduced
      ? [{ opacity: 1 }, { opacity: 1, offset: 0.6 }, { opacity: 0 }]
      : [
          { opacity: 0, transform: "scale(1.06)" },
          { opacity: 1, transform: "none", offset: 0.2 },
          { opacity: 1, offset: 0.6 },
          { opacity: 0 },
        ];
    void play(ring, keyframes, 900, { delay: i * 70, ease: "ease-out" }).then(() => ring.remove());
  });
}
