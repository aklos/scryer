/**
 * Wraps the actual C4Node component from the app for use in docs.
 * Stubs provide @xyflow/react and @tauri-apps/* so it renders standalone.
 */
import { C4Node } from "@app/nodes/C4Node";
import { ThemeContext } from "@app/theme";
import type { C4Kind, C4Shape } from "@app/types";

const SCALE = 0.75;
const W = 180;
const H = 160;

interface NodeDemoProps {
  kind?: C4Kind;
  shape?: C4Shape;
  label?: string;
  technology?: string;
  description?: string;
  status?: "proposed" | "wip" | "ready";
  external?: boolean;
}

export function NodeDemo({
  kind = "system",
  shape,
  label = "Node",
  technology,
  description,
  status,
  external,
}: NodeDemoProps) {
  // Estimate height: base 160 + member chips area if needed
  const extraH = kind === "system" ? 14 : kind === "container" ? 10 : kind === "component" ? 14 : 0;
  const fullH = H + extraH;
  const sw = W * SCALE;
  const sh = fullH * SCALE;

  const data: Record<string, unknown> = {
    name: label,
    kind,
    shape,
    technology,
    description,
    status,
    external: !!external,
  };

  return (
    <ThemeContext.Provider value={0}>
      <div style={{ width: sw, height: sh }}>
        <div
          className="origin-top-left"
          style={{ width: W, height: fullH, transform: `scale(${SCALE})` }}
        >
          <C4Node
            id={`doc-${label}`}
            data={data as any}
            selected={false}
          />
        </div>
      </div>
    </ThemeContext.Provider>
  );
}

export function NodeGallery({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-wrap gap-6 justify-center py-6 px-4 rounded-xl bg-zinc-50 dark:bg-zinc-900 border border-zinc-200 dark:border-zinc-700">
      {children}
    </div>
  );
}

export function ShapeGallery() {
  const shapes: { shape: C4Shape; label: string; kind?: C4Kind }[] = [
    { shape: "cylinder", label: "Cylinder" },
    { shape: "pipe", label: "Pipe" },
    { shape: "trapezoid", label: "Trapezoid" },
    { shape: "bucket", label: "Bucket" },
    { shape: "hexagon", label: "Hexagon" },
  ];

  return (
    <div className="flex flex-wrap gap-6 justify-center py-6 px-4 rounded-xl bg-zinc-50 dark:bg-zinc-900 border border-zinc-200 dark:border-zinc-700">
      <NodeDemo kind="person" label="Person" />
      <NodeDemo kind="system" label="Rectangle" />
      {shapes.map((s) => (
        <NodeDemo key={s.shape} shape={s.shape} kind={s.kind ?? "container"} label={s.label} />
      ))}
    </div>
  );
}
