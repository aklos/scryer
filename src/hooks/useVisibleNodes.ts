import { useEffect, useMemo } from "react";
import type { C4Node, C4NodeData, C4Edge, Group, Hint, Status } from "../types";
import { assignAllHandles } from "../edgeRouting";
import { computeEdgeBundles } from "../edgeBundling";

const NODE_W = 180;
const NODE_H = 160;

interface UseVisibleNodesParams {
  nodes: C4Node[];
  edges: C4Edge[];
  currentParentId: string | undefined;
  refPositions: Record<string, { x: number; y: number }>;
  groups: Group[];
  setRefPositions: React.Dispatch<React.SetStateAction<Record<string, { x: number; y: number }>>>;
  activeHints: Record<string, Hint[]>;
  changedNodeIds: Set<string>;
  driftedNodeIds: Set<string>;
  nonPlanarEdgeIds: Set<string>;
}

export function useVisibleNodes({
  nodes,
  edges,
  currentParentId,
  refPositions,
  groups,
  setRefPositions,
  activeHints,
  changedNodeIds,
  driftedNodeIds,
  nonPlanarEdgeIds,
}: UseVisibleNodesParams) {
  const levelPrefix = currentParentId ?? "root";

  // Show only nodes at the current level, strip parentId so they render as top-level.
  // Also include reference nodes. Groups are drawn as a separate overlay layer
  // (Bubble Sets), not as ReactFlow nodes.
  const visibleNodes = useMemo(() => {
    const childNodes = nodes
      .filter((n) => (n.parentId ?? undefined) === currentParentId)
      .map((n) => ({ ...n, parentId: undefined, extent: undefined }));

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

    if (!currentParentId) return injectMembers(childNodes);

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
    const allLevelNodes = [...effectiveChildNodes, ...extraNodes];
    const childXs = allLevelNodes.map((n: C4Node) => n.position.x);
    const childYs = allLevelNodes.map((n: C4Node) => n.position.y);
    const bounds = allLevelNodes.length > 0
      ? { minX: Math.min(...childXs), maxX: Math.max(...childXs), minY: Math.min(...childYs), maxY: Math.max(...childYs) }
      : { minX: 100, maxX: 100, minY: 100, maxY: 100 };

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

    return [...extraNodes, ...injectMembers(effectiveChildNodes), ...refNodes];
  }, [nodes, edges, currentParentId, refPositions]);

  // Lock in auto-computed reference positions so they don't shift when children move.
  // Wait until auto-layout is done (no _needsLayout nodes) so refs are positioned
  // relative to the final child layout, not the pre-layout default positions.
  useEffect(() => {
    const hasLayoutPending = visibleNodes.some((n) => n.data._needsLayout && !n.data._reference);
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

  // Map each node to the NAME of its immediate group (if any). Rendered as
  // a small label on the node so membership — and which group — is visible
  // at a glance, matching what the AI sees via get_node.
  const groupNameByNodeId = useMemo(() => {
    const map = new Map<string, string>();
    for (const g of groups) {
      for (const id of g.memberIds) map.set(id, g.name);
    }
    return map;
  }, [groups]);

  const visibleNodesWithHints = useMemo(() => {
    return visibleNodes.map((n) => {
      const nodeHints = activeHints[n.id];
      const groupName = groupNameByNodeId.get(n.id);
      const changed = changedNodeIds.has(n.id);
      const drifted = driftedNodeIds.has(n.id);
      if (!nodeHints && !groupName && !changed && !drifted) return n;
      return { ...n, data: { ...n.data, ...(nodeHints ? { _hints: nodeHints } : {}), ...(groupName ? { _groupName: groupName } : {}), ...(changed ? { _changed: true } : {}), ...(drifted ? { _drifted: true } : {}) } };
    });
  }, [visibleNodes, activeHints, groupNameByNodeId, changedNodeIds, driftedNodeIds]);

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
    const bundles = computeEdgeBundles(filtered, visibleNodes);
    const dirSet = new Set(filtered.map(e => `${e.source}::${e.target}`));

    return filtered.map((e) => {
      const handles = handleMap.get(e.id);
      const connected = selIds.size > 0 && (selIds.has(e.source) || selIds.has(e.target));
      const dimmed = selIds.size > 0 && !connected;
      const bundle = bundles.get(e.id);
      const biPair = dirSet.has(`${e.target}::${e.source}`);

      // Force the hub-side handle to the magnet's cardinal direction
      let sourceHandle = handles?.sourceHandle;
      let targetHandle = handles?.targetHandle;
      if (bundle) {
        if (bundle.hubIsSource) sourceHandle = bundle.hubHandle;
        else targetHandle = bundle.hubHandle;
      }

      return {
        ...e,
        ...(sourceHandle || targetHandle ? { sourceHandle, targetHandle } : {}),
        ...(e.data ? { data: { ...e.data, ...(bundle && !e.data._route ? { _route: bundle.route } : {}), ...(connected ? { _highlighted: true } : {}), ...(dimmed ? { _dimmed: true } : {}), ...(biPair ? { _biPair: true } : {}) } } : {}),
      };
    });
  }, [edges, visibleNodes, currentParentId, nodes, nonPlanarEdgeIds]);

  // Track reference node IDs
  const refNodeIds = useMemo(
    () => new Set(visibleNodes.filter((n) => n.data._reference).map((n) => n.id)),
    [visibleNodes],
  );

  return {
    visibleNodes,
    visibleNodesWithHints,
    visibleEdges,
    refNodeIds,
    levelPrefix,
  };
}
