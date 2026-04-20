import { useMemo } from "react";
import type { C4Node, C4Edge, C4NodeData, SourceLocation } from "../types";

export type ExternalEdge = C4Edge & {
  externalNodeName: string;
  externalNodeKind: string;
  direction: "out" | "in";
};

export interface NodeContext {
  descendants: C4Node[];
  internalEdges: C4Edge[];
  externalEdges: ExternalEdge[];
  nodeSourceMap: Record<string, SourceLocation[]>;
}

const EMPTY: NodeContext = {
  descendants: [],
  internalEdges: [],
  externalEdges: [],
  nodeSourceMap: {},
};

/** Frontend mirror of the MCP `get_node` tool — computes the subtree, edge partition, and source map slice for a selected node. */
export function useNodeContext(
  nodeId: string | null,
  allNodes: C4Node[],
  allEdges: C4Edge[],
  sourceMap: Record<string, SourceLocation[]>,
): NodeContext {
  return useMemo(() => {
    if (!nodeId) return EMPTY;

    // Subtree IDs: walk parentId upward fixed-point
    const subtreeIds = new Set<string>();
    subtreeIds.add(nodeId);
    let changed = true;
    while (changed) {
      changed = false;
      for (const n of allNodes) {
        if (n.parentId && subtreeIds.has(n.parentId) && !subtreeIds.has(n.id)) {
          subtreeIds.add(n.id);
          changed = true;
        }
      }
    }

    const descendants = allNodes.filter((n) => subtreeIds.has(n.id) && n.id !== nodeId);

    const internalEdges: C4Edge[] = [];
    const externalEdges: ExternalEdge[] = [];
    for (const edge of allEdges) {
      const srcIn = subtreeIds.has(edge.source);
      const tgtIn = subtreeIds.has(edge.target);
      if (srcIn && tgtIn) {
        internalEdges.push(edge);
      } else if (srcIn || tgtIn) {
        const extId = srcIn ? edge.target : edge.source;
        const extNode = allNodes.find((n) => n.id === extId);
        if (extNode) {
          externalEdges.push({
            ...edge,
            externalNodeName: (extNode.data as C4NodeData).name,
            externalNodeKind: (extNode.data as C4NodeData).kind,
            direction: srcIn ? "out" : "in",
          });
        }
      }
    }

    const nodeSourceMap: Record<string, SourceLocation[]> = {};
    for (const [k, v] of Object.entries(sourceMap)) {
      if (subtreeIds.has(k)) nodeSourceMap[k] = v;
    }

    return { descendants, internalEdges, externalEdges, nodeSourceMap };
  }, [nodeId, allNodes, allEdges, sourceMap]);
}
