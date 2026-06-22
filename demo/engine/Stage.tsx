/**
 * The stage: mounts a scene, wires it to a Director, and plays it.
 *
 * Layering, bottom to top: the camera-transformed `content` (the scene's
 * rendered UI), the synthetic `cursor`, then the `caption`. The cursor and
 * caption ride above the camera, so they keep constant size while the content
 * pushes in underneath.
 *
 * The one subtlety worth knowing: `director.flush()` runs in an effect keyed on
 * scene state, so a `d.set(...)` in the script resolves only after React has
 * committed that state — letting the very next camera move measure fresh layout.
 */

import { useEffect, useRef, useState } from "react";
import { Director } from "./director";
import { AnnotationLayer, type Annotation } from "./Annotation";
import { CancelledError, type Scene } from "./types";
import "./engine.css";

export function Stage<S>({ scene }: { scene: Scene<S> }) {
  const [state, setState] = useState<S>(scene.initial);
  const [annotation, setAnnotation] = useState<Annotation | null>(null);
  const [title, setTitle] = useState<string | null>(null);
  const frameRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const cursorRef = useRef<HTMLDivElement>(null);
  const dirRef = useRef<Director<S> | null>(null);

  // Build the director once the refs are live, then run the shot list.
  useEffect(() => {
    const d = new Director<S>(
      { frame: frameRef.current!, content: contentRef.current!, cursor: cursorRef.current! },
      { setState, setAnnotation, setTitle },
    );
    dirRef.current = d;
    // Render harness: signal scene completion so the recorder knows when to stop.
    (window as unknown as { __filmDone?: boolean }).__filmDone = false;
    scene
      .run(d)
      .then(() => {
        (window as unknown as { __filmDone?: boolean }).__filmDone = true;
      })
      .catch((e) => {
        if (!(e instanceof CancelledError)) console.error("film: scene failed", e);
      });
    return () => {
      d.dispose();
      dirRef.current = null;
    };
  }, [scene]);

  // Release any `d.set(...)` awaiting this commit.
  useEffect(() => {
    dirRef.current?.flush();
  }, [state]);

  return (
    <div ref={frameRef} className="film-frame">
      <div ref={contentRef} className="film-content">
        {scene.render(state)}
      </div>

      <div ref={cursorRef} className="film-cursor">
        <span className="film-cursor-ring" />
        <svg width="17" height="23" viewBox="0 0 17 23" fill="none" aria-hidden>
          <path
            d="M0.5 0.7 L0.5 18.6 L5.1 14.2 L8.2 21.2 L10.8 20.0 L7.7 13.2 L13.2 13.2 Z"
            fill="#fff"
            stroke="rgba(0,0,0,0.55)"
            strokeWidth="1"
            strokeLinejoin="round"
          />
        </svg>
      </div>

      <AnnotationLayer data={annotation} />

      <div className="film-title" data-show={title ? "true" : "false"}>
        <span className="film-title-bar" />
        <span className="film-title-text">{title}</span>
      </div>
    </div>
  );
}
