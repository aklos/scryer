/**
 * Kinetic treatment wrapper — wraps a lifted scene in the trailer's motion +
 * overlay look (push-in, scrim, sliding headline behind an accent bar). Per
 * scene the push-in origin/intensity and headline placement can be tuned so
 * nothing important crops.
 */

import type { CSSProperties, ReactNode } from "react";
import "./treatment.css";

export interface TreatmentOpts {
  /** Where the headline sits. "center" frees the bottom strip (powerline). */
  placement?: "bottom" | "center";
  /** Push-in transform-origin, e.g. "50% 18%" to keep a header in frame. */
  origin?: string;
  /** Push-in scale [from, to]. Lighter for text-dense scenes. */
  zoom?: [number, number];
  /** Bottom legibility gradient. */
  scrim?: boolean;
}

export function Treated({
  headline,
  children,
  placement = "bottom",
  origin = "center",
  zoom = [1.04, 1.13],
  scrim = true,
}: { headline: string; children: ReactNode } & TreatmentOpts) {
  const stageStyle = {
    transformOrigin: origin,
    "--k-from": zoom[0],
    "--k-to": zoom[1],
  } as CSSProperties;
  return (
    <div className="kscene">
      <div className="kstage" style={stageStyle}>{children}</div>
      {scrim && <div className="kscrim" />}
      <div className={`koverlay${placement === "center" ? " koverlay--center" : ""}`}>
        <div className="khead">
          <div className="kbar" />
          <h2 className="kheadline">{headline}</h2>
        </div>
      </div>
    </div>
  );
}
