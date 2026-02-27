import { useCallback } from "react";
import { applyNodeChanges } from "@xyflow/react";
import type { OnNodesChange } from "@xyflow/react";
import type { C4Node, C4NodeData, C4Edge, Group } from "../types";

interface UseNodesChangeParams {
  refNodeIds: Set<string>;
  groupNodeIds: Set<string>;
  levelPrefix: string;
  setNodes: React.Dispatch<React.SetStateAction<C4Node[]>>;
  setEdges: React.Dispatch<React.SetStateAction<C4Edge[]>>;
  setRefPositions: React.Dispatch<React.SetStateAction<Record<string, { x: number; y: number }>>>;
  setGroups: React.Dispatch<React.SetStateAction<Group[]>>;
  setSelectedGroupId: React.Dispatch<React.SetStateAction<string | null>>;
  setSourceMap: React.Dispatch<React.SetStateAction<Record<string, import("../types").SourceLocation[]>>>;
}

export function useNodesChange({
  refNodeIds,
  groupNodeIds,
  levelPrefix,
  setNodes,
  setEdges,
  setRefPositions,
  setGroups,
  setSelectedGroupId,
  setSourceMap,
}: UseNodesChangeParams): OnNodesChange {
  return useCallback(
    (changes) => {
      // Redirect position changes for reference nodes to local state
      const refPosUpdates: Record<string, { x: number; y: number }> = {};
      const dimUpdates = new Map<string, { width?: number; height?: number }>();
      const filtered = changes.filter((c) => {
        if (c.type === "dimensions") {
          if (c.dimensions) dimUpdates.set(c.id, c.dimensions);
          return false;
        }
        if (c.type === "position" && refNodeIds.has(c.id)) {
          if (c.position) refPosUpdates[`${levelPrefix}/${c.id}`] = c.position;
          return false;
        }
        // Group boxes: ignore position changes (auto-computed from members)
        if (c.type === "position" && groupNodeIds.has(c.id)) return false;
        // Group box removal: remove the group from state
        if (c.type === "remove" && groupNodeIds.has(c.id)) {
          setGroups((prev) => prev.filter((g) => g.id !== c.id));
          return false;
        }
        // Group box selection: track separately since they aren't in `nodes`
        if (c.type === "select" && groupNodeIds.has(c.id)) {
          if (c.selected) {
            setSelectedGroupId(c.id);
            setNodes((nds) => nds.map((n) => n.selected ? { ...n, selected: false } : n));
            setEdges((eds) => eds.map((e) => e.selected ? { ...e, selected: false } : e));
          } else {
            setSelectedGroupId((prev) => prev === c.id ? null : prev);
          }
          return false;
        }
        return true;
      });

      // Clear group selection when a regular node is selected
      if (filtered.some((c) => c.type === "select" && c.selected)) {
        setSelectedGroupId(null);
      }

      if (Object.keys(refPosUpdates).length > 0) {
        setRefPositions((prev) => ({ ...prev, ...refPosUpdates }));
      }

      // Apply measured dimensions to stored nodes so ReactFlow treats them as initialized
      if (dimUpdates.size > 0) {
        setNodes((nds) => {
          let changed = false;
          const result = nds.map((n) => {
            const dim = dimUpdates.get(n.id);
            if (!dim) return n;
            if (n.measured?.width === dim.width && n.measured?.height === dim.height) return n;
            changed = true;
            return { ...n, measured: { width: dim.width, height: dim.height } };
          });
          return changed ? result : nds;
        });
      }

      // All non-dimension changes were filtered out — nothing more to do.
      if (filtered.length === 0) return;

      setNodes((nds) => {
        // Cascade: when a node is removed, also remove all its descendants + their edges.
        const removeIds = new Set(
          filtered.filter((c) => c.type === "remove").map((c) => c.id),
        );
        if (removeIds.size > 0) {
          let grew = true;
          while (grew) {
            grew = false;
            for (const n of nds) {
              if (n.parentId && removeIds.has(n.parentId) && !removeIds.has(n.id)) {
                removeIds.add(n.id);
                filtered.push({ type: "remove", id: n.id });
                grew = true;
              }
            }
          }
          setEdges((eds) =>
            eds.filter((e) => !removeIds.has(e.source) && !removeIds.has(e.target)),
          );
          setSourceMap((prev) => {
            const next = { ...prev };
            for (const nid of removeIds) delete next[nid];
            return next;
          });
          // Remove deleted node IDs from group memberships, auto-delete empty groups
          setGroups((prev) => {
            const updated = prev
              .map((g) => ({ ...g, memberIds: g.memberIds.filter((mid) => !removeIds.has(mid)) }))
              .filter((g) => g.memberIds.length > 0);
            return updated.length === prev.length && updated.every((g, i) => g.memberIds.length === prev[i].memberIds.length) ? prev : updated;
          });
        }

        // Build parentId lookup — visibleNodes strips parentId for rendering,
        // so replace changes from ReactFlow would lose it without this restore.
        const parentMap = new Map<string, string>();
        for (const n of nds) {
          if (n.parentId) parentMap.set(n.id, n.parentId);
        }

        const updated = applyNodeChanges(filtered, nds) as C4Node[];

        let changed = updated.length !== nds.length;
        const result = updated.map((n) => {
          const pid = parentMap.get(n.id);
          const hasTransient = '_hints' in n.data || '_groupHighlight' in n.data || n.data._reference !== undefined || n.data._relationships !== undefined || (n.data as Record<string, unknown>)._operations !== undefined || (n.data as Record<string, unknown>)._processes !== undefined || (n.data as Record<string, unknown>)._models !== undefined || '_originalParentId' in n.data || '_originalStatus' in n.data || '_mentionNames' in n.data;
          if (hasTransient) {
            const { _hints, _groupHighlight, _reference, _relationships, _operations, _processes, _models, _originalParentId, _originalStatus, _mentionNames, ...rest } = n.data as C4NodeData & Record<string, unknown>;
            n = { ...n, data: rest as C4NodeData };
            changed = true;
          }
          if (pid && !n.parentId) {
            changed = true;
            return { ...n, parentId: pid };
          }
          return n;
        });
        if (!changed) {
          for (let i = 0; i < result.length; i++) {
            if (result[i] !== nds[i]) { changed = true; break; }
          }
        }
        return changed ? result : nds;
      });
    },
    [refNodeIds, groupNodeIds, levelPrefix, setNodes, setEdges, setRefPositions, setGroups, setSelectedGroupId, setSourceMap],
  );
}
