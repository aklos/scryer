import { useEffect, useMemo } from "react";
import type { C4Kind, C4Node, C4NodeData, C4Edge, Group, Hint, Status } from "../types";
import { assignAllHandles } from "../edgeRouting";
import { routeCrossingEdges } from "../layout/routing";

const NODE_W = 180;
const NODE_H = 160;

interface UseVisibleNodesParams {
  nodes: C4Node[];
  edges: C4Edge[];
  currentParentId: string | undefined;
  refPositions: Record<string, { x: number; y: number }>;
  groups: Group[];
  selectedGroupId: string | null;
  setRefPositions: React.Dispatch<React.SetStateAction<Record<string, { x: number; y: number }>>>;
  activeHints: Record<string, Hint[]>;
  changedNodeIds: Set<string>;
  driftedNodeIds: Set<string>;
  nonPlanarEdgeIds: Set<string>;
  faceRoutes: Map<string, { x: number; y: number }[]>;
}

export function useVisibleNodes({
  nodes,
  edges,
  currentParentId,
  refPositions,
  groups,
  selectedGroupId,
  setRefPositions,
  activeHints,
  changedNodeIds,
  driftedNodeIds,
  nonPlanarEdgeIds,
  faceRoutes,
}: UseVisibleNodesParams) {
  const levelPrefix = currentParentId ?? "root";

  // Show only nodes at the current level, strip parentId so they render as top-level.
  // Also include reference nodes and group bounding boxes.
  const visibleNodes = useMemo(() => {
    const childNodes = nodes
      .filter((n) => (n.parentId ?? undefined) === currentParentId)
      .map((n) => ({ ...n, parentId: undefined, extent: undefined }));

    // Inject group bounding boxes for groups that have visible members
    const groupBoxes: C4Node[] = [];
    const PAD_X = 30;
    const PAD_TOP = 42;
    const PAD_BOTTOM = 20;
    for (const group of groups) {
      const members = childNodes.filter((n) => group.memberIds.includes(n.id));
      if (members.length === 0) continue;
      const xs = members.map((n) => n.position.x);
      const ys = members.map((n) => n.position.y);
      const minX = Math.min(...xs) - PAD_X;
      const minY = Math.min(...ys) - PAD_TOP;
      const maxX = Math.max(...xs) + NODE_W + PAD_X;
      const maxY = Math.max(...ys) + NODE_H + PAD_BOTTOM;
      const w = maxX - minX;
      const h = maxY - minY;
      groupBoxes.push({
        id: group.id,
        type: "groupBox",
        position: { x: minX, y: minY },
        style: { width: w, height: h, pointerEvents: "none" as const },
        zIndex: -1,
        draggable: false,
        selectable: true,
        selected: selectedGroupId === group.id,
        measured: { width: w, height: h },
        data: {
          name: group.name,
          description: "",
          kind: "system" as C4Kind,
          groupKind: group.kind,
          contract: group.contract,
          _memberIds: group.memberIds,
        },
      } as C4Node);
    }

    // Build a set of node IDs that have at least one child
    const parentIdsWithChildren = new Set<string>();
    for (const n of nodes) {
      if (n.parentId) parentIdsWithChildren.add(n.parentId);
    }

    // Inject _operations, _processes, _models into component nodes, and _hasChildren for expandable nodes
    const injectMembers = (nodeList: C4Node[]): C4Node[] =>
      nodeList.map((n) => {
        const kind = (n.data as C4NodeData).kind;
        const hasChildren = parentIdsWithChildren.has(n.id);
        if (kind === "component") {
          const allMembers = nodes
            .filter((c) => c.parentId === n.id && (c.data as C4NodeData).kind === "operation")
            .map((c) => ({ id: c.id, name: (c.data as C4NodeData).name, status: (c.data as C4NodeData).status }));
          const procChips = nodes
            .filter((c) => c.parentId === n.id && (c.data as C4NodeData).kind === "process")
            .map((c) => ({ id: c.id, name: (c.data as C4NodeData).name, status: (c.data as C4NodeData).status }));
          const modelChips = nodes
            .filter((c) => c.parentId === n.id && (c.data as C4NodeData).kind === "model")
            .map((c) => ({ id: c.id, name: (c.data as C4NodeData).name, status: (c.data as C4NodeData).status }));
          if (allMembers.length === 0 && procChips.length === 0 && modelChips.length === 0) {
            return hasChildren ? { ...n, data: { ...n.data, _hasChildren: true } } : n;
          }
          return { ...n, data: { ...n.data, _operations: allMembers, _processes: procChips, _models: modelChips, ...(hasChildren ? { _hasChildren: true } : {}) } };
        }
        if (hasChildren && (kind === "system" || kind === "container")) {
          return { ...n, data: { ...n.data, _hasChildren: true } };
        }
        return n;
      });

    const withGroups = injectMembers([...groupBoxes, ...childNodes]);

    if (!currentParentId) return withGroups;

    // At code level: inject mention names into process nodes
    const parentNode = nodes.find((n) => n.id === currentParentId);
    const isCodeLevel = parentNode && (parentNode.data as C4NodeData).kind === "component";
    let effectiveChildNodes = childNodes;
    if (isCodeLevel) {
      // Compute mention names for process description autocomplete
      const opNames = nodes
        .filter((n) => n.parentId === currentParentId && (n.data as C4NodeData).kind === "operation")
        .map((n) => ({ name: (n.data as C4NodeData).name, kind: "operation" as const, status: (n.data as C4NodeData).status }));
      const procNames = nodes
        .filter((n) => n.parentId === currentParentId && (n.data as C4NodeData).kind === "process")
        .map((n) => ({ name: (n.data as C4NodeData).name, kind: "process" as const, status: (n.data as C4NodeData).status }));
      const componentModelNames = nodes
        .filter((n) => n.parentId === currentParentId && (n.data as C4NodeData).kind === "model")
        .map((n) => ({ name: (n.data as C4NodeData).name, kind: "model" as const, status: (n.data as C4NodeData).status }));
      const compEdges = edges.filter((e) => e.source === currentParentId || e.target === currentParentId);
      const refIds = new Set(compEdges.map((e) => (e.source === currentParentId ? e.target : e.source)));
      const refMemberNames: { name: string; kind: "operation" | "process" | "model"; status?: Status; ref?: boolean }[] = [];
      for (const refId of refIds) {
        const refNode = nodes.find((n) => n.id === refId);
        if (!refNode || (refNode.data as C4NodeData).kind !== "component") continue;
        for (const kind of ["operation", "process", "model"] as const) {
          nodes
            .filter((n) => n.parentId === refId && (n.data as C4NodeData).kind === kind)
            .forEach((n) => refMemberNames.push({ name: (n.data as C4NodeData).name, kind, status: (n.data as C4NodeData).status, ref: true }));
        }
      }
      const codeLevelMentionNames = [...opNames, ...procNames, ...componentModelNames, ...refMemberNames];

      // Inject _mentionNames into process and operation child nodes
      effectiveChildNodes = childNodes.map((n) => {
        const kind = (n.data as C4NodeData).kind;
        if (kind === "process" || kind === "operation") {
          return { ...n, data: { ...n.data, _mentionNames: codeLevelMentionNames.filter((m) => m.name !== (n.data as C4NodeData).name) } };
        }
        return n;
      });

      // Code level uses rack view — no reference nodes or mention edges needed
      return injectMembers(effectiveChildNodes);
    }

    const extraNodes: C4Node[] = [];

    // Find edges involving the current parent at the parent's own level
    const parentEdges = edges.filter(
      (e) => e.source === currentParentId || e.target === currentParentId,
    );
    if (parentEdges.length === 0) return [...extraNodes, ...injectMembers(effectiveChildNodes)];

    // Collect external node IDs and their relationships
    const refMap = new Map<string, { direction: "in" | "out"; label: string; method?: string }[]>();
    for (const edge of parentEdges) {
      const isSource = edge.source === currentParentId;
      const otherId = isSource ? edge.target : edge.source;
      const direction = isSource ? "out" : "in";
      const label = (edge.data as { label?: string })?.label ?? "";
      const method = (edge.data as { method?: string })?.method;

      if (!refMap.has(otherId)) refMap.set(otherId, []);
      refMap.get(otherId)!.push({ direction, label, method });
    }

    // Build reference nodes from the external nodes
    const childIds = new Set(effectiveChildNodes.map((n: C4Node) => n.id));

    // Compute child bounding box for auto-positioning reference nodes.
    // Include group box extents so refs don't overlap group borders.
    const allLevelNodes = [...effectiveChildNodes, ...extraNodes];
    const childXs = allLevelNodes.map((n: C4Node) => n.position.x);
    const childYs = allLevelNodes.map((n: C4Node) => n.position.y);
    let bounds = allLevelNodes.length > 0
      ? { minX: Math.min(...childXs), maxX: Math.max(...childXs), minY: Math.min(...childYs), maxY: Math.max(...childYs) }
      : { minX: 100, maxX: 100, minY: 100, maxY: 100 };
    // Expand bounds to include group boxes (which extend beyond member positions)
    for (const group of groups) {
      const members = allLevelNodes.filter((n: C4Node) => group.memberIds.includes(n.id));
      if (members.length === 0) continue;
      const gxs = members.map((n: C4Node) => n.position.x);
      const gys = members.map((n: C4Node) => n.position.y);
      const gMinX = Math.min(...gxs) - 30; // PAD_X from group box rendering
      const gMinY = Math.min(...gys) - 42; // PAD_TOP
      const gMaxX = Math.max(...gxs) + NODE_W + 30;
      const gMaxY = Math.max(...gys) + NODE_H + 20; // PAD_BOTTOM
      bounds = {
        minX: Math.min(bounds.minX, gMinX),
        minY: Math.min(bounds.minY, gMinY),
        maxX: Math.max(bounds.maxX, gMaxX),
        maxY: Math.max(bounds.maxY, gMaxY),
      };
    }

    // Separate refs by primary direction for placement
    const inRefs: string[] = [];
    const outRefs: string[] = [];
    for (const [refId, rels] of refMap) {
      if (childIds.has(refId)) continue;
      const refNode = nodes.find((n) => n.id === refId);
      if (!refNode) continue;
      // Skip components from other containers — they shouldn't leak as refs
      const refKind = (refNode.data as C4NodeData).kind;
      if ((refKind === "component" || refKind === "operation" || refKind === "process" || refKind === "model") && refNode.parentId !== currentParentId) continue;
      if (isCodeLevel) {
        if ((refNode.data as C4NodeData).kind !== "component") continue;
        const hasOutgoing = rels.some((r) => r.direction === "out");
        if (!hasOutgoing) continue;
      }
      const primaryDir = rels[0]?.direction ?? "out";
      if (primaryDir === "in") inRefs.push(refId);
      else outRefs.push(refId);
    }

    const refSpacing = NODE_W + 70;
    const refNodes: C4Node[] = [];

    // "in" refs go above child nodes
    const inTotalW = (inRefs.length - 1) * refSpacing;
    const inStartX = (bounds.minX + bounds.maxX) / 2 - inTotalW / 2;
    const inY = bounds.minY - NODE_H - 120;
    for (let i = 0; i < inRefs.length; i++) {
      const refId = inRefs[i];
      const original = nodes.find((n) => n.id === refId)!;
      const relationships = refMap.get(refId)!;
      const autoPos = { x: inStartX + i * refSpacing, y: inY };
      refNodes.push({
        ...original,
        position: refPositions[`${currentParentId ?? "root"}/${refId}`] ?? autoPos,
        parentId: undefined, extent: undefined,
        selectable: true, deletable: false,
        data: {
          ...original.data,
          _reference: true,
          _relationships: relationships,
          ...(isCodeLevel ? { _codeLevel: true } : {}),
          ...((original.data as C4NodeData).kind === "component" ? {
            _operations: nodes
              .filter((c) => c.parentId === refId && (c.data as C4NodeData).kind === "operation")
              .map((c) => ({ id: c.id, name: (c.data as C4NodeData).name, status: (c.data as C4NodeData).status })),
            _processes: nodes
              .filter((c) => c.parentId === refId && (c.data as C4NodeData).kind === "process")
              .map((c) => ({ id: c.id, name: (c.data as C4NodeData).name, status: (c.data as C4NodeData).status })),
            _models: nodes
              .filter((c) => c.parentId === refId && (c.data as C4NodeData).kind === "model")
              .map((c) => ({ id: c.id, name: (c.data as C4NodeData).name, status: (c.data as C4NodeData).status })),
          } : {}),
        },
      });
    }

    // "out" refs go below child nodes
    const outTotalW = (outRefs.length - 1) * refSpacing;
    const outStartX = (bounds.minX + bounds.maxX) / 2 - outTotalW / 2;
    const outY = bounds.maxY + NODE_H + 120;
    for (let i = 0; i < outRefs.length; i++) {
      const refId = outRefs[i];
      const original = nodes.find((n) => n.id === refId)!;
      const relationships = refMap.get(refId)!;
      const autoPos = { x: outStartX + i * refSpacing, y: outY };
      refNodes.push({
        ...original,
        position: refPositions[`${currentParentId ?? "root"}/${refId}`] ?? autoPos,
        parentId: undefined, extent: undefined,
        selectable: true, deletable: false,
        data: {
          ...original.data,
          _reference: true,
          _relationships: relationships,
          ...(isCodeLevel ? { _codeLevel: true } : {}),
          ...((original.data as C4NodeData).kind === "component" ? {
            _operations: nodes
              .filter((c) => c.parentId === refId && (c.data as C4NodeData).kind === "operation")
              .map((c) => ({ id: c.id, name: (c.data as C4NodeData).name, status: (c.data as C4NodeData).status })),
            _processes: nodes
              .filter((c) => c.parentId === refId && (c.data as C4NodeData).kind === "process")
              .map((c) => ({ id: c.id, name: (c.data as C4NodeData).name, status: (c.data as C4NodeData).status })),
            _models: nodes
              .filter((c) => c.parentId === refId && (c.data as C4NodeData).kind === "model")
              .map((c) => ({ id: c.id, name: (c.data as C4NodeData).name, status: (c.data as C4NodeData).status })),
          } : {}),
        },
      });
    }

    return [...groupBoxes, ...extraNodes, ...injectMembers(effectiveChildNodes), ...refNodes];
  }, [nodes, edges, currentParentId, refPositions, groups, selectedGroupId]);

  // Lock in auto-computed reference positions so they don't shift when children move.
  // Wait until auto-layout is done (no _needsLayout nodes) so refs are positioned
  // relative to the final child layout, not the pre-layout default positions.
  useEffect(() => {
    const hasLayoutPending = visibleNodes.some((n) => n.data._needsLayout && n.type !== "groupBox" && !n.data._reference);
    if (hasLayoutPending) return;
    const refs = visibleNodes.filter((n) => n.data._reference);
    if (refs.length === 0) return;
    const updates: Record<string, { x: number; y: number }> = {};
    let hasNew = false;
    for (const ref of refs) {
      const key = `${levelPrefix}/${ref.id}`;
      if (!(key in refPositions)) {
        updates[key] = ref.position;
        hasNew = true;
      }
    }
    if (hasNew) {
      setRefPositions((prev) => ({ ...prev, ...updates }));
    }
  }, [visibleNodes, refPositions, levelPrefix, setRefPositions]);

  // Inject group membership highlight when a group box is selected
  const groupHighlightInfo = useMemo(() => {
    if (!selectedGroupId) return null;
    const group = groups.find((g) => g.id === selectedGroupId);
    return group ? { memberIds: new Set(group.memberIds) } : null;
  }, [selectedGroupId, groups]);

  const visibleNodesWithHints = useMemo(() => {
    return visibleNodes.map((n) => {
      const nodeHints = activeHints[n.id];
      const memberHighlight = groupHighlightInfo?.memberIds.has(n.id);
      const changed = changedNodeIds.has(n.id);
      const drifted = driftedNodeIds.has(n.id);
      if (!nodeHints && !memberHighlight && !changed && !drifted) return n;
      return { ...n, data: { ...n.data, ...(nodeHints ? { _hints: nodeHints } : {}), ...(memberHighlight ? { _groupHighlight: true } : {}), ...(changed ? { _changed: true } : {}), ...(drifted ? { _drifted: true } : {}) } };
    });
  }, [visibleNodes, activeHints, groupHighlightInfo, changedNodeIds, driftedNodeIds]);

  // Only show edges between visible nodes, assigning handles via graph-aware routing
  // At code level (inside a component), no edges are needed — the rack view handles display
  const visibleEdges = useMemo(() => {
    const parentNode = currentParentId ? nodes.find((n) => n.id === currentParentId) : null;
    const isCodeLevel = parentNode && (parentNode.data as C4NodeData).kind === "component";
    if (isCodeLevel) return [];

    const ids = new Set(visibleNodes.map((n) => n.id));
    const selIds = new Set(visibleNodes.filter((n) => n.selected).map((n) => n.id));
    const filtered = edges.filter((e) => {
      if (!ids.has(e.source) || !ids.has(e.target)) return false;
      return true;
    });
    const handleMap = assignAllHandles(visibleNodes, filtered);

    // Route only non-planar edges as orthogonal polylines
    const nonPlanarFiltered = filtered.filter((e) => nonPlanarEdgeIds.has(e.id));
    const routeMap = nonPlanarFiltered.length > 0
      ? routeCrossingEdges(
          visibleNodes.filter((n) => n.type !== "groupBox"),
          nonPlanarFiltered,
        )
      : new Map();

    // For routed edges, override handle assignment based on route approach direction
    const nodeMap = new Map(visibleNodes.map((n) => [n.id, n]));
    function handleForApproach(nodeId: string, fromX: number, fromY: number): string {
      const node = nodeMap.get(nodeId);
      if (!node) return "top";
      const cx = node.position.x + (node.measured?.width ?? 180) / 2;
      const cy = node.position.y + (node.measured?.height ?? 160) / 2;
      const dx = fromX - cx, dy = fromY - cy;
      // Pick handle based on which side the approach comes from
      if (Math.abs(dx) > Math.abs(dy)) {
        return dx > 0 ? "right" : "left";
      } else {
        return dy > 0 ? "bottom" : "top";
      }
    }

    return filtered.map((e) => {
      let handles = handleMap.get(e.id);
      const route = routeMap.get(e.id) ?? faceRoutes.get(e.id);

      // Override handles for routed edges based on route approach direction
      if (route && route.length >= 1) {
        const firstBend = route[0];
        const lastBend = route[route.length - 1];
        const srcHandle = handleForApproach(e.source, firstBend.x, firstBend.y);
        const tgtHandle = handleForApproach(e.target, lastBend.x, lastBend.y);
        handles = { sourceHandle: srcHandle, targetHandle: tgtHandle };
      }

      const connected = selIds.size > 0 && (selIds.has(e.source) || selIds.has(e.target));
      const dimmed = selIds.size > 0 && !connected;
      return {
        ...e,
        ...(handles ?? {}),
        ...(e.data ? { data: { ...e.data, ...(route ? { _route: route } : {}), ...(connected ? { _highlighted: true } : {}), ...(dimmed ? { _dimmed: true } : {}) } } : {}),
      };
    });
  }, [edges, visibleNodes, currentParentId, nodes, nonPlanarEdgeIds, faceRoutes]);

  // Track reference/group/process/model node IDs
  const refNodeIds = useMemo(
    () => new Set(visibleNodes.filter((n) => n.data._reference).map((n) => n.id)),
    [visibleNodes],
  );

  const groupNodeIds = useMemo(
    () => new Set(visibleNodes.filter((n) => n.type === "groupBox").map((n) => n.id)),
    [visibleNodes],
  );

  return {
    visibleNodes,
    visibleNodesWithHints,
    visibleEdges,
    refNodeIds,
    groupNodeIds,
    levelPrefix,
  };
}
