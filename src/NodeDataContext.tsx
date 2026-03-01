import { createContext, useContext } from "react";

type UpdateNodeDataFn = (id: string, data: Record<string, unknown>) => void;

const NodeDataContext = createContext<UpdateNodeDataFn | null>(null);

export const NodeDataProvider = NodeDataContext.Provider;

/**
 * Returns an updateNodeData function. When inside a NodeDataProvider
 * (code-level rack), uses the provided setNodes-based updater.
 * Otherwise returns null so callers fall back to useReactFlow().
 */
export function useNodeDataOverride(): UpdateNodeDataFn | null {
  return useContext(NodeDataContext);
}
