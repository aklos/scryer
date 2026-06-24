/**
 * The director: the authoring surface a scene's `run` choreographs against.
 *
 * It owns the live camera transform and the synthetic cursor (both imperative,
 * off the rAF tweens — no React churn), and it bridges to React for the two
 * things that *are* state: the scene's own model (`set`) and the caption.
 *
 * The script reads as a shot list:
 *
 *     await d.camera("title", { hold: 700 });   // push in, dwell
 *     await d.cursorTo("anchor-1");             // hand moves to the anchor
 *     await d.click();                          // tap
 *     await d.set((s) => ({ ...s, open: true })); // UI reacts; awaits the paint
 *
 * `set` resolves only once React has committed the new state to the DOM, so a
 * camera move on the next line measures the *new* layout, not the old one.
 */

import type { Dispatch, SetStateAction } from "react";
import { camEase, cursorEase, lerp } from "./easing";
import { contentRect, frameTransform, type FrameOpts, type Rect, type Transform } from "./camera";
import { tween, wait, type TweenHandle } from "./tween";
import { CancelledError } from "./types";
import type { Annotation } from "./Annotation";

interface Dom {
  frame: HTMLElement;
  content: HTMLElement;
  cursor: HTMLElement;
}

/** Drop the caret at the end of a contentEditable, so typed text grows rightward
 *  rather than re-inserting at the start each keystroke. */
function caretToEnd(el: HTMLElement): void {
  const range = document.createRange();
  range.selectNodeContents(el);
  range.collapse(false);
  const sel = window.getSelection();
  sel?.removeAllRanges();
  sel?.addRange(range);
}

/** Set an input/textarea value through React's tracked setter, so its `onChange`
 *  fires on the dispatched `input` event (React dedupes against its own cache). */
function setNativeValue(el: HTMLInputElement | HTMLTextAreaElement, value: string): void {
  const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement : HTMLInputElement;
  const setter = Object.getOwnPropertyDescriptor(proto.prototype, "value")?.set;
  setter?.call(el, value);
}

interface Bridge<S> {
  setState: Dispatch<SetStateAction<S>>;
  setAnnotation: Dispatch<SetStateAction<Annotation | null>>;
  setTitle: Dispatch<SetStateAction<string | null>>;
}

export interface CameraOpts extends FrameOpts {
  /** Travel time, ms. */
  duration?: number;
  /** Dwell after arriving, ms. */
  hold?: number;
}

export class Director<S = unknown> {
  private tf: Transform = { x: 0, y: 0, s: 1 };
  private cur = { x: 0, y: 0 };
  private live: TweenHandle[] = [];
  private stateResolvers: Array<() => void> = [];
  private disposed = false;
  private lastAnnotation: Annotation | null = null;

  constructor(private dom: Dom, private bridge: Bridge<S>) {
    const v = this.view();
    this.cur = { x: v.w * 0.5, y: v.h * 0.62 };
    this.applyCamera();
    this.applyCursor();
  }

  // --- choreography API ------------------------------------------------------

  /** Push/pan the camera to frame a target (a `data-cam` name or a selector). */
  async camera(target: string | Rect, opts: CameraOpts = {}): Promise<void> {
    this.guard();
    const rect = typeof target === "string" ? this.rectOf(target) : target;
    const to = frameTransform(rect, this.view(), opts);
    const from = { ...this.tf };
    await this.run(
      tween(opts.duration ?? 720, camEase, (e) => {
        this.tf = {
          x: lerp(from.x, to.x, e),
          y: lerp(from.y, to.y, e),
          s: lerp(from.s, to.s, e),
        };
        this.applyCamera();
      }),
    );
    if (opts.hold) await this.wait(opts.hold);
  }

  /** Glide the synthetic cursor to a target's centre. */
  async cursorTo(target: string, opts: { duration?: number } = {}): Promise<void> {
    this.guard();
    const { x, y } = this.screenCentre(this.resolve(target));
    const from = { ...this.cur };
    await this.run(
      tween(opts.duration ?? 460, cursorEase, (e) => {
        this.cur = { x: lerp(from.x, x, e), y: lerp(from.y, y, e) };
        this.applyCursor();
      }),
    );
  }

  /**
   * A tap at the cursor: press dip + expanding ripple. If `target` is given,
   * dispatches a real DOM click on it after the press — for interactions the
   * scene can't drive through props (e.g. opening an internal source peek).
   */
  async click(target?: string): Promise<void> {
    this.guard();
    const ring = this.dom.cursor.querySelector<HTMLElement>(".film-cursor-ring");
    ring?.animate(
      [
        { transform: "scale(0.2)", opacity: 0.55 },
        { transform: "scale(1)", opacity: 0 },
      ],
      { duration: 520, easing: "cubic-bezier(0.22,1,0.36,1)" },
    );
    // composite:"add" so the press dip COMPOSES with the cursor's base
    // translate3d (its live position) instead of replacing it — otherwise the
    // keyframes' bare scale() drops the translate and the cursor snaps to (0,0)
    // for the length of the tap.
    this.dom.cursor.animate(
      [{ transform: "scale(1)" }, { transform: "scale(0.86)" }, { transform: "scale(1)" }],
      { duration: 220, easing: "ease-out", composite: "add" },
    );
    if (target) {
      const el = this.resolve(target);
      // A press pulse on the hit element itself — box-shadow only, so it reads
      // on buttons and diagram cards alike without disturbing React Flow's
      // positioning transform.
      el.animate(
        [
          { boxShadow: "0 0 0 0 rgba(96,165,250,0)" },
          { boxShadow: "0 0 0 4px rgba(96,165,250,0.5)" },
          { boxShadow: "0 0 0 0 rgba(96,165,250,0)" },
        ],
        { duration: 440, easing: "cubic-bezier(0.22,1,0.36,1)" },
      );
      el.click();
    }
    await this.wait(180);
  }

  /**
   * Type text into a real editable element (a `contentEditable` span or an
   * `<input>`/`<textarea>`), one character at a time — driving the component's
   * own input handlers so the model updates exactly as if a human typed. Used
   * for the live edit-mode beats the scene can't express through `set` (the
   * field is uncontrolled, seeded from `initial` on mount).
   */
  async typeInto(target: string, text: string, opts: { charMs?: number } = {}): Promise<void> {
    this.guard();
    const el = this.resolve(target);
    el.focus();
    const charMs = opts.charMs ?? 42;
    const isField = el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement;
    for (let i = 1; i <= text.length; i++) {
      const slice = text.slice(0, i);
      if (isField) {
        setNativeValue(el as HTMLInputElement | HTMLTextAreaElement, slice);
      } else {
        el.textContent = slice;
        caretToEnd(el);
      }
      el.dispatchEvent(new InputEvent("input", { bubbles: true }));
      await this.wait(charMs);
    }
  }

  /** Update the scene's model; resolves after React paints the change. */
  async set(updater: SetStateAction<S>): Promise<void> {
    this.guard();
    await new Promise<void>((resolve) => {
      this.stateResolvers.push(resolve);
      this.bridge.setState(updater);
    });
  }

  /**
   * Anchor a coachmark to a target: a highlight ring + a tooltip pointing at
   * it. Capture the target's screen rect now (the camera should be settled), so
   * the annotation pins to where the element actually is on screen.
   */
  async annotate(
    target: string,
    text: string,
    opts: { place?: "top" | "bottom" | "right" | "above" } = {},
  ): Promise<void> {
    this.guard();
    const f = this.dom.frame.getBoundingClientRect();
    const r = this.resolve(target).getBoundingClientRect();
    const rect = { x: r.left - f.left, y: r.top - f.top, w: r.width, h: r.height };
    const place = opts.place ?? (rect.y + rect.h / 2 < f.height * 0.55 ? "bottom" : "top");
    // Cap the label so it wraps and stays inside the video-safe area: from where
    // the label starts (the leader's end, ~LEAD past the pin) to the frame's
    // right edge, less a ~6% title-safe margin — and never wider than a
    // comfortable reading measure. Keeps text off the edges on any player.
    const LEAD = 66; // leader length + label inset, matching AnnotationLayer
    const safe = Math.max(96, f.width * 0.06);
    let maxWidth: number;
    if (place === "above") {
      // Centred over the target — symmetric room to the nearer frame edge.
      const cx = rect.x + rect.w / 2;
      maxWidth = Math.max(160, Math.min(560, 2 * (Math.min(cx, f.width - cx) - safe)));
    } else {
      const labelLeft = (place === "right" ? rect.x + rect.w : rect.x) + LEAD;
      maxWidth = Math.max(160, Math.min(560, f.width - labelLeft - safe));
    }
    const annotation: Annotation = { ...rect, text, place, maxWidth };
    this.lastAnnotation = annotation;
    this.bridge.setAnnotation(annotation);
  }

  /** A clean lower-third title for the rare global beat (not anchored to an
   *  element). Pass null to clear. */
  async title(text: string | null): Promise<void> {
    this.guard();
    this.bridge.setTitle(text);
  }

  /** Drop any anchored annotation and/or title. An anchored label fades out
   *  (set exiting → hold for the CSS transition → unmount) rather than popping. */
  async clear(): Promise<void> {
    this.guard();
    this.bridge.setTitle(null);
    if (this.lastAnnotation) {
      this.bridge.setAnnotation({ ...this.lastAnnotation, exiting: true });
      this.lastAnnotation = null;
      await this.wait(340);
      this.bridge.setAnnotation(null);
    } else {
      this.bridge.setAnnotation(null);
    }
  }

  async wait(ms: number): Promise<void> {
    this.guard();
    await this.run(wait(ms));
  }

  // --- React bridge ----------------------------------------------------------

  /** Called by the Stage after every committed render. */
  flush(): void {
    const pending = this.stateResolvers;
    this.stateResolvers = [];
    pending.forEach((r) => r());
  }

  dispose(): void {
    this.disposed = true;
    this.live.forEach((h) => h.cancel());
    this.live = [];
    this.stateResolvers.forEach((r) => r());
    this.stateResolvers = [];
  }

  // --- internals -------------------------------------------------------------

  private async run(h: TweenHandle): Promise<void> {
    this.live.push(h);
    await h.done;
    this.live = this.live.filter((x) => x !== h);
    this.guard();
  }

  private guard(): void {
    if (this.disposed) throw new CancelledError();
  }

  private view(): { w: number; h: number } {
    return { w: this.dom.frame.offsetWidth, h: this.dom.frame.offsetHeight };
  }

  private resolve(target: string): HTMLElement {
    const sel = /^[.#[]/.test(target) ? target : `[data-cam="${target}"]`;
    const el = this.dom.content.querySelector<HTMLElement>(sel);
    if (!el) throw new Error(`film: no target for "${target}" (${sel})`);
    return el;
  }

  private rectOf(target: string): Rect {
    return contentRect(this.resolve(target), this.dom.content);
  }

  private screenCentre(el: HTMLElement): { x: number; y: number } {
    const f = this.dom.frame.getBoundingClientRect();
    const r = el.getBoundingClientRect();
    return { x: r.left - f.left + r.width / 2, y: r.top - f.top + r.height / 2 };
  }

  private applyCamera(): void {
    this.dom.content.style.transform =
      `translate3d(${this.tf.x}px, ${this.tf.y}px, 0) scale(${this.tf.s})`;
  }

  private applyCursor(): void {
    this.dom.cursor.style.transform = `translate3d(${this.cur.x}px, ${this.cur.y}px, 0)`;
  }
}
