/**
 * Autoplay runner — plays the timeline beat by beat, cross-fading each incoming
 * beat over the outgoing one. Keying each layer by index remounts it, so the
 * beat's own entrance animations (push-in, headline slide-up) restart on cut.
 * Plays once and holds the close card; `loop` repeats for preview.
 */

import { useEffect, useState } from "react";
import { timeline } from "./scenes";

export function Runner({ loop = false }: { loop?: boolean }) {
  const [i, setI] = useState(0);

  useEffect(() => {
    const isLast = i === timeline.length - 1;
    if (isLast && !loop) return; // hold on the close card
    const t = setTimeout(
      () => setI((v) => (v + 1) % timeline.length),
      timeline[i].duration,
    );
    return () => clearTimeout(t);
  }, [i, loop]);

  const prevIdx = i > 0 ? i - 1 : loop ? timeline.length - 1 : null;

  return (
    <div className="kroot">
      {prevIdx !== null && (
        <div className="klayer" key={`prev-${i}`}>
          {timeline[prevIdx].render()}
        </div>
      )}
      <div className="klayer klayer--in" key={`cur-${i}`}>
        {timeline[i].render()}
      </div>
    </div>
  );
}
