import { useEffect, useMemo } from "react";
import type { C4Kind, C4Node, C4NodeData, C4Edge, Group, Hint } from "../types";
import { assignAllHandles } from "../edgeRouting";

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
        style: { width: w, height: h },
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
          _memberIds: group.memberIds,
        },
      } as C4Node);
    }

    // Inject _operations, _processes, and _models into component nodes so they show inline
    const injectMembers = (nodeList: C4Node[]): C4Node[] =>
      nodeList.map((n) => {
        if ((n.data as C4NodeData).kind !== "component") return n;
        const allMembers = nodes
          .filter((c) => c.parentId === n.id && (c.data as C4NodeData).kind === "operation")
          .map((c) => ({ id: c.id, name: (c.data as C4NodeData).name, status: (c.data as C4NodeData).status }));
        const procChips = nodes
          .filter((c) => c.parentId === n.id && (c.data as C4NodeData).kind === "process")
          .map((c) => ({ id: c.id, name: (c.data as C4NodeData).name, status: (c.data as C4NodeData).status }));
        const modelChips = nodes
          .filter((c) => c.parentId === n.id && (c.data as C4NodeData).kind === "model")
          .map((c) => ({ id: c.id, name: (c.data as C4NodeData).name, status: (c.data as C4NodeData).status }));
        if (allMembers.length === 0 && procChips.length === 0 && modelChips.length === 0) return n;
        return { ...n, data: { ...n.data, _operations: allMembers, _processes: procChips, _models: modelChips } };
      });

    const withGroups = injectMembers([...groupBoxes, ...childNodes]);

    if (!currentParentId) return withGroups;

    // At code level: inject mention names into process nodes
    const parentNode = nodes.find((n) => n.id === currentParentId);
    const isCodeLevel = parentNode && (parentNode.data as C4NodeData).kind === "component";
    let effectiveChildNodes = childNodes;
    const extraNodes: C4Node[] = [];
    if (isCodeLevel) {
      // Compute mention names for process description autocomplete
      const opNames = nodes
        .filter((n) => n.parentId === currentParentId && (n.data as C4NodeData).kind === "operation")
        .map((n) => ({ name: (n.data as C4NodeData).name, kind: "operation" as const }));
      const procNames = nodes
        .filter((n) => n.parentId === currentParentId && (n.data as C4NodeData).kind === "process")
        .map((n) => ({ name: (n.data as C4NodeData).name, kind: "process" as const }));
      const componentModelNames = nodes
        .filter((n) => n.parentId === currentParentId && (n.data as C4NodeData).kind === "model")
        .map((n) => ({ name: (n.data as C4NodeData).name, kind: "model" as const }));
      const compEdges = edges.filter((e) => e.source === currentParentId || e.target === currentParentId);
      const refIds = new Set(compEdges.map((e) => (e.source === currentParentId ? e.target : e.source)));
      const refMemberNames: { name: string; kind: "operation" | "process" | "model"; ref?: boolean }[] = [];
      for (const refId of refIds) {
        const refNode = nodes.find((n) => n.id === refId);
        if (!refNode || (refNode.data as C4NodeData).kind !== "component") continue;
        for (const kind of ["operation", "process", "model"] as const) {
          nodes
            .filter((n) => n.parentId === refId && (n.data as C4NodeData).kind === kind)
            .forEach((n) => refMemberNames.push({ name: (n.data as C4NodeData).name, kind, ref: true }));
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
    }

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

    // Compute child bounding box for auto-positioning reference nodes
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

  // Lock in auto-computed reference positions so they don't shift when children move
  useEffect(() => {
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
      if (!nodeHints && !memberHighlight) return n;
      return { ...n, data: { ...n.data, ...(nodeHints ? { _hints: nodeHints } : {}), ...(memberHighlight ? { _groupHighlight: true } : {}) } };
    });
  }, [visibleNodes, activeHints, groupHighlightInfo]);

  // Only show edges between visible nodes, assigning handles via graph-aware routing
  // At code level (inside a component), show synthetic mention edges
  const visibleEdges = useMemo(() => {
    const parentNode = currentParentId ? nodes.find((n) => n.id === currentParentId) : null;
    const isCodeLevel = parentNode && (parentNode.data as C4NodeData).kind === "component";
    if (isCodeLevel) {
      // Build name→id map for visible nodes (including members of ref components)
      const nameToId = new Map<string, string>();
      for (const n of visibleNodes) {
        const nd = n.data as C4NodeData;
        nameToId.set(nd.name, n.id);
        // Map member names of reference components to the ref node itself
        if (n.data._reference && nd.kind === "component") {
          for (const list of [nd._operations, nd._processes, nd._models] as ({ id: string; name: string }[] | undefined)[]) {
            if (list) for (const m of list) nameToId.set(m.name, n.id);
          }
        }
      }
      // Parse @[name] mentions and create synthetic edges
      const mentionEdges: C4Edge[] = [];
      const seen = new Set<string>();
      for (const n of visibleNodes) {
        const desc = (n.data as C4NodeData).description ?? "";
        const re = /@\[([^\]]+)\]/g;
        let match;
        while ((match = re.exec(desc)) !== null) {
          const targetId = nameToId.get(match[1]);
          if (targetId && targetId !== n.id) {
            const key = `${n.id}-${targetId}`;
            if (!seen.has(key)) {
              seen.add(key);
              mentionEdges.push({
                id: `mention-${key}`,
                source: n.id,
                target: targetId,
                zIndex: -1,
                data: { label: "", _mention: true },
              } as C4Edge);
            }
          }
        }
      }
      const codeSel = new Set(visibleNodes.filter((n) => n.selected && !n.data._reference).map((n) => n.id));
      if (codeSel.size > 0) {
        return mentionEdges.map((e) => {
          const connected = codeSel.has(e.source) || codeSel.has(e.target);
          return { ...e, data: { ...e.data, ...(connected ? { _highlighted: true } : { _dimmed: true }) } } as C4Edge;
        });
      }
      return mentionEdges;
    }

    const ids = new Set(visibleNodes.map((n) => n.id));
    const refIds = new Set(visibleNodes.filter((n) => n.data._reference).map((n) => n.id));
    const selIds = new Set(visibleNodes.filter((n) => n.selected).map((n) => n.id));
    const filtered = edges.filter((e) => {
      if (!ids.has(e.source) || !ids.has(e.target)) return false;
      if (currentParentId && refIds.has(e.source) && refIds.has(e.target)) return false;
      return true;
    });
    const handleMap = assignAllHandles(visibleNodes, filtered);

    return filtered.map((e) => {
      const handles = handleMap.get(e.id);
      const connected = selIds.size > 0 && (selIds.has(e.source) || selIds.has(e.target));
      const dimmed = selIds.size > 0 && !connected;
      return {
        ...e,
        ...(handles ?? {}),
        ...(e.data ? { data: { ...e.data, ...(connected ? { _highlighted: true } : {}), ...(dimmed ? { _dimmed: true } : {}) } } : {}),
      };
    });
  }, [edges, visibleNodes, currentParentId, nodes]);

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
