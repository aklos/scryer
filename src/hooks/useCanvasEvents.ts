import { useEffect } from "react";
import type { C4Kind, C4Node, C4NodeData, Status } from "../types";

interface UseCanvasEventsParams {
  expandNode: (nodeId: string) => void;
  setNodes: React.Dispatch<React.SetStateAction<C4Node[]>>;
  setEdges: React.Dispatch<React.SetStateAction<import("../types").C4Edge[]>>;
  screenToFlowPosition: (position: { x: number; y: number }) => { x: number; y: number };
  nodes: C4Node[];
}

export function useCanvasEvents({
  expandNode,
  setNodes,
  setEdges,
  screenToFlowPosition,
  nodes,
}: UseCanvasEventsParams) {
  // node-expand
  useEffect(() => {
    const handler = (e: Event) => {
      const { nodeId } = (e as CustomEvent).detail;
      expandNode(nodeId);
    };
    window.addEventListener("node-expand", handler);
    return () => window.removeEventListener("node-expand", handler);
  }, [expandNode]);

  // operation-reparent
  useEffect(() => {
    const handler = (e: Event) => {
      const { operationId, newParentId } = (e as CustomEvent).detail;
      setNodes((nds) =>
        nds.map((n) => {
          if (n.id !== operationId) return n;
          const d = n.data as C4NodeData & { _originalParentId?: string; _originalStatus?: Status };
          const origParent = d._originalParentId ?? n.parentId;
          const origStatus = d._originalStatus ?? d.status;
          const movedAway = newParentId !== origParent;
          return {
            ...n,
            parentId: newParentId,
            data: {
              ...n.data,
              _originalParentId: origParent,
              _originalStatus: origStatus,
              status: movedAway ? ("changed" as Status) : origStatus,
            } as C4NodeData,
          };
        }),
      );
    };
    window.addEventListener("operation-reparent", handler);
    return () => window.removeEventListener("operation-reparent", handler);
  }, [setNodes]);

  // add-operation
  useEffect(() => {
    const handler = (e: Event) => {
      const { componentId } = (e as CustomEvent).detail;
      const siblingCount = nodes.filter((n) => n.parentId === componentId).length;
      const newNode: C4Node = {
        id: crypto.randomUUID(),
        type: "operation",
        position: { x: (siblingCount % 4) * 280, y: Math.floor(siblingCount / 4) * 200 },
        data: {
          name: "newOperation",
          description: "",
          kind: "operation" as C4Kind,
          status: "proposed",
        },
        parentId: componentId,
      };
      setNodes((nds) => [...nds, newNode]);
    };
    window.addEventListener("add-operation", handler);
    return () => window.removeEventListener("add-operation", handler);
  }, [nodes, setNodes]);

  // remove-node
  useEffect(() => {
    const handler = (e: Event) => {
      const { nodeId } = (e as CustomEvent).detail;
      setNodes((nds) => nds.filter((n) => n.id !== nodeId));
      setEdges((eds) => eds.filter((e) => e.source !== nodeId && e.target !== nodeId));
    };
    window.addEventListener("remove-node", handler);
    return () => window.removeEventListener("remove-node", handler);
  }, [setNodes, setEdges]);

  // add-process (creates a real process node)
  useEffect(() => {
    const handler = (e: Event) => {
      const { componentId } = (e as CustomEvent).detail;
      const position = screenToFlowPosition({
        x: window.innerWidth / 2,
        y: window.innerHeight / 2,
      });
      const newNode: C4Node = {
        id: crypto.randomUUID(),
        type: "process",
        position,
        data: {
          name: "New process",
          description: "",
          kind: "process" as C4Kind,
          status: "proposed",
        },
        parentId: componentId,
      };
      setNodes((nds) => [...nds, newNode]);
    };
    window.addEventListener("add-process", handler);
    return () => window.removeEventListener("add-process", handler);
  }, [screenToFlowPosition, setNodes]);

  // mention-click — select node by name (code level)
  useEffect(() => {
    const handler = (e: Event) => {
      const { name } = (e as CustomEvent).detail;
      setNodes((nds) =>
        nds.map((n) => ({
          ...n,
          selected: (n.data as C4NodeData).name === name,
        })),
      );
    };
    window.addEventListener("mention-click", handler);
    return () => window.removeEventListener("mention-click", handler);
  }, [setNodes]);

  // add-model (creates a real model node)
  useEffect(() => {
    const handler = (e: Event) => {
      const { componentId } = (e as CustomEvent).detail;
      const position = screenToFlowPosition({
        x: window.innerWidth / 2,
        y: window.innerHeight / 2,
      });
      const newNode: C4Node = {
        id: crypto.randomUUID(),
        type: "model",
        position,
        data: {
          name: "newModel",
          description: "",
          kind: "model" as C4Kind,
          status: "proposed",
          properties: [],
        },
        parentId: componentId,
      };
      setNodes((nds) => [...nds, newNode]);
    };
    window.addEventListener("add-model", handler);
    return () => window.removeEventListener("add-model", handler);
  }, [screenToFlowPosition, setNodes]);
}
