// Stub for @xyflow/react — node handles aren't interactive in docs.
export const Position = { Top: "top", Bottom: "bottom", Left: "left", Right: "right" } as const;

export function Handle() {
  return null;
}

// Re-export types that C4Node imports
export type NodeProps<T = any> = {
  id: string;
  data: T extends { data: infer D } ? D : any;
  selected?: boolean;
};
