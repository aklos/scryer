import type { C4Node, C4NodeData, C4Edge, Hint, Group, Flow, SourceLocation } from "./types";
import type { MentionItem } from "./MentionTextarea";
import { useNodeContext } from "./hooks/useNodeContext";
import { JsonRoot, JLine, P } from "./context-panel/json";
import { NodeDataSection } from "./context-panel/NodeDataSection";
import { ContractContainer, ContractStatusBadge } from "./context-panel/ContractContainer";
import { DescendantsSection } from "./context-panel/DescendantsSection";
import { InternalEdgesSection, ExternalEdgesSection } from "./context-panel/EdgesSection";
import { SourceMapSection } from "./context-panel/SourceMapSection";
import { GroupChainSection } from "./context-panel/GroupChainSection";
import { GroupIdentitySection, GroupContractContainer, GroupMembersSection, getGroupContract } from "./context-panel/GroupContextPanel";
import { EdgePanel } from "./context-panel/EdgePanel";
import { MultiSelectionPanel } from "./context-panel/MultiSelectionPanel";
import { HintsFooter } from "./context-panel/HintsFooter";
import { GroupsPalette } from "./GroupsView";

const panelBase = "shrink-0 border-l border-[var(--border)] bg-[var(--surface)]";

export interface ContextPanelProps {
  node: C4Node | null;
  edge: C4Edge | null;
  selectedGroupId?: string | null;
  onUpdateEdge: (id: string, data: { label?: string; method?: string }) => void;
  codeLevel?: boolean;
  hints?: Hint[];
  onFixHint?: (hint: Hint) => void;
  onDismissHint?: (hint: Hint) => void;
  projectPath?: string;
  groups?: Group[];
  onUpdateGroups?: (fn: (prev: Group[]) => Group[]) => void;
  allNodes?: C4Node[];
  allEdges?: C4Edge[];
  sourceMap?: Record<string, SourceLocation[]>;
  nodeDiffs?: Map<string, C4NodeData>;
  onDismissDiff?: (nodeId: string) => void;
  onUpdateOperationData?: (id: string, data: Record<string, unknown>) => void;
  processMentionNames?: MentionItem[];
  multiSelected?: string[];
  totalSelected?: number;
  canGroup?: boolean;
  onCreateGroup?: (name: string, memberIds: string[]) => void;
  onAddToGroup?: (groupId: string, memberIds: string[]) => void;
  activeFlow?: Flow | null;
  groupsPaletteMode?: boolean;
}

export function ContextPanel(props: ContextPanelProps) {
  const {
    node, edge, selectedGroupId, onUpdateEdge, codeLevel, hints, onFixHint, onDismissHint, projectPath,
    groups, onUpdateGroups, allNodes, allEdges, sourceMap, onUpdateOperationData,
    multiSelected, totalSelected, canGroup,
    onCreateGroup, onAddToGroup, activeFlow, nodeDiffs, onDismissDiff, groupsPaletteMode,
  } = props;

  if (activeFlow) return null;

  if (groupsPaletteMode) {
    return (
      <div className={`${panelBase} w-96 flex flex-col overflow-hidden`}>
        <GroupsPalette />
      </div>
    );
  }

  if ((totalSelected ?? 0) >= 2) {
    if (multiSelected && multiSelected.length >= 2 && canGroup && onCreateGroup && onAddToGroup) {
      return (
        <div className={`${panelBase} w-96 flex flex-col`}>
          <div className="flex-1 min-h-0 p-4 flex flex-col overflow-y-auto">
            <MultiSelectionPanel
              selectedIds={multiSelected}
              groups={groups ?? []}
              onCreateGroup={onCreateGroup}
              onAddToGroup={onAddToGroup}
            />
          </div>
        </div>
      );
    }
    return null;
  }

  if (!node && !edge && !selectedGroupId) return null;

  return (
    <div className={`${panelBase} w-96 flex flex-col overflow-hidden`}>
      <PanelBody
        node={node}
        edge={edge}
        selectedGroupId={selectedGroupId ?? null}
        onUpdateEdge={onUpdateEdge}
        codeLevel={codeLevel}
        allNodes={allNodes ?? []}
        allEdges={allEdges ?? []}
        sourceMap={sourceMap ?? {}}
        groups={groups ?? []}
        onUpdateGroups={onUpdateGroups}
        onUpdateOperationData={onUpdateOperationData}
        projectPath={projectPath}
        nodeDiffs={nodeDiffs}
        onDismissDiff={onDismissDiff}
      />
      <HintsFooter
        hints={hints ?? []}
        onFixHint={onFixHint ?? (() => {})}
        onDismissHint={onDismissHint ?? (() => {})}
      />
    </div>
  );
}

function PanelBody({ node, edge, selectedGroupId, onUpdateEdge, codeLevel, allNodes, allEdges, sourceMap, groups, onUpdateGroups, onUpdateOperationData, projectPath, nodeDiffs, onDismissDiff }: {
  node: C4Node | null;
  edge: C4Edge | null;
  selectedGroupId: string | null;
  onUpdateEdge: (id: string, data: { label?: string; method?: string }) => void;
  codeLevel?: boolean;
  allNodes: C4Node[];
  allEdges: C4Edge[];
  sourceMap: Record<string, SourceLocation[]>;
  groups: Group[];
  onUpdateGroups?: (fn: (prev: Group[]) => Group[]) => void;
  onUpdateOperationData?: (id: string, data: Record<string, unknown>) => void;
  projectPath?: string;
  nodeDiffs?: Map<string, C4NodeData>;
  onDismissDiff?: (nodeId: string) => void;
}) {
  // Always call hook (Rules of Hooks); pass null when no node selected.
  const ctx = useNodeContext(node?.id ?? null, allNodes, allEdges, sourceMap);

  if (!node && edge) {
    return (
      <div className="flex-1 min-h-0 px-3 py-3 overflow-y-auto overflow-x-auto">
        <EdgePanel edge={edge} onUpdate={(data) => onUpdateEdge(edge.id, data)} codeLevel={codeLevel} />
      </div>
    );
  }
  if (selectedGroupId) {
    if (!onUpdateGroups) return null;
    const groupContract = getGroupContract(groups, selectedGroupId);
    return (
      <>
        {/* ── Section 1: group identity ── */}
        <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
          <SectionHeader label="// group" />
          <div className="flex-1 min-h-0 px-3 pb-3 overflow-y-auto overflow-x-auto">
            <GroupIdentitySection groupId={selectedGroupId} groups={groups} onUpdateGroups={onUpdateGroups} />
          </div>
        </div>

        {/* ── Section 2: group contract ── */}
        <div className="shrink-0 flex flex-col overflow-hidden border-t border-[var(--border)] bg-violet-500/[0.04]" style={{ minHeight: "8rem", maxHeight: "45%" }}>
          <SectionHeader
            label="// contract"
            trailing={<ContractStatusBadge contract={groupContract} />}
          />
          <div className="flex-1 min-h-0 px-3 pb-3 overflow-y-auto overflow-x-auto">
            <GroupContractContainer groupId={selectedGroupId} groups={groups} onUpdateGroups={onUpdateGroups} />
          </div>
        </div>

        {/* ── Section 3: group members ── */}
        <div className="shrink-0 max-h-[35%] border-t border-[var(--border)] bg-[var(--surface-tint)]/30 flex flex-col overflow-hidden">
          <SectionHeader label="// members (read-only)" />
          <div className="flex-1 min-h-0 px-3 pb-3 overflow-y-auto overflow-x-auto opacity-60 hover:opacity-90 transition-opacity">
            <GroupMembersSection groupId={selectedGroupId} groups={groups} onUpdateGroups={onUpdateGroups} allNodes={allNodes} />
          </div>
        </div>
      </>
    );
  }

  if (!node) return null;

  const previousData = nodeDiffs?.get(node.id);
  const showContract = node.data.kind !== "person" && !node.data.external && node.data.kind !== "model";

  return (
    <>
      {/* ── Section 1: node identity (editable, vibrant) ── */}
      <div className="flex-1 min-h-0 flex flex-col overflow-hidden">
        <SectionHeader label="// node" />
        <div className="flex-1 min-h-0 px-3 pb-3 overflow-y-auto overflow-x-auto">
          <NodeDataSection
            node={node}
            indent={0}
            previousData={previousData}
            onDismissDiff={previousData && onDismissDiff ? () => onDismissDiff(node.id) : undefined}
          />
        </div>
      </div>

      {/* ── Section 2: contract (editable, prominent) ── */}
      {showContract && (
        <div className="shrink-0 flex flex-col overflow-hidden border-t border-[var(--border)] bg-violet-500/[0.04]" style={{ minHeight: "8rem", maxHeight: "45%" }}>
          <SectionHeader
            label="// contract"
            trailing={<ContractStatusBadge contract={node.data.contract} />}
          />
          <div className="flex-1 min-h-0 px-3 pb-3 overflow-y-auto overflow-x-auto">
            <ContractContainer node={node} />
          </div>
        </div>
      )}

      {/* ── Section 3: get_node() context (read-only, dimmed) ── */}
      <div className="shrink-0 max-h-[35%] border-t border-[var(--border)] bg-[var(--surface-tint)]/30 flex flex-col overflow-hidden">
        <SectionHeader label="// get_node() context (read-only)" />
        <div className="flex-1 min-h-0 px-3 pb-3 overflow-y-auto overflow-x-auto opacity-60 hover:opacity-90 transition-opacity">
          <JsonRoot>
            <JLine indent={0}><P>{"{"}</P></JLine>
            <GroupChainSection nodeId={node.id} groups={groups} indent={1} />
            <DescendantsSection node={node} descendants={ctx.descendants} onUpdateOperationData={onUpdateOperationData} indent={1} />
            <InternalEdgesSection edges={ctx.internalEdges} allNodes={allNodes} indent={1} />
            <ExternalEdgesSection edges={ctx.externalEdges} indent={1} />
            <SourceMapSection sourceMap={ctx.nodeSourceMap} allNodes={allNodes} projectPath={projectPath} indent={1} />
            <JLine indent={0}><P>{"}"}</P></JLine>
          </JsonRoot>
        </div>
      </div>
    </>
  );
}

function SectionHeader({ label, trailing }: { label: string; trailing?: React.ReactNode }) {
  return (
    <div className="shrink-0 px-3 pt-2 pb-1 flex items-center justify-between gap-2 font-mono text-[10px] uppercase tracking-wider text-[var(--text-ghost)]">
      <span>{label}</span>
      {trailing}
    </div>
  );
}
