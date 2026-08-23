import { useMemo, useRef } from "react";
import { X } from "lucide-react";
import type { Node, Group } from "../viewmodel";
import {
  effectiveSourceMap,
  effectiveTestMap,
  nextResponsibilityId,
} from "../viewmodel";
import type { Editor } from "../editor";
import { testStatesOf } from "../health";
import { ChangeGlyph } from "../diffkit";
import { usePageMenu, useCopyId, copyIdItem } from "../pageMenu";
import {
  BTN,
  Empty,
  LINK,
  NAME_MAX,
  PAGE_COL,
  PageSection,
  SectionEditor,
} from "../pagekit";
import type { PageProps } from "./types";
import { useEditSections } from "./kit";
import { ancestorChain, Crumbs, PageHeader } from "./PageHeader";
import { DescriptionSection } from "./DescriptionSection";
import { plannedRespHosts, ResponsibilitiesSection } from "./ResponsibilitiesSection";

// --- group page -------------------------------------------------------------

export function GroupPageBody(props: PageProps & { group: Group }) {
  const { model, committed, group, editor, projectPath, onSelectNode } = props;
  const ed = useEditSections();
  const openMenu = usePageMenu();
  const copyId = useCopyId();
  // The name edit accumulates in this draft; the model is written once, on
  // Done. Cancel (or navigating away) drops the draft.
  const titleDraft = useRef<{ name: string } | null>(null);
  const openTitleEdit = () => {
    if (ed.isEditing("title")) return;
    titleDraft.current = { name: group.name };
    ed.toggle("title");
  };
  const commitTitleEdit = () => {
    const d = titleDraft.current;
    titleDraft.current = null;
    ed.toggle("title");
    if (!d || !editor) return;
    const name = d.name.trim() || group.name;
    if (name !== group.name) editor.updateGroup(group.id, { name });
  };
  const cancelTitleEdit = () => {
    titleDraft.current = null;
    ed.toggle("title");
  };
  const members = group.memberIds
    .map((id) => model.nodes.find((n) => n.id === id))
    .filter((n): n is Node => Boolean(n));

  const sourceMap = effectiveSourceMap(committed, model);
  const testMap = effectiveTestMap(committed, model);
  const testStates = useMemo(() => testStatesOf(props.report), [props.report]);
  const resps = group.responsibilities ?? [];
  const committedResps =
    committed?.groups.find((g) => g.id === group.id)?.responsibilities ?? [];

  const containerId =
    group.parentNodeId ??
    model.nodes.find((n) => n.id === (group.memberIds[0] ?? ""))?.parentId ??
    undefined;

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <PageHeader
        crumbs={
          <Crumbs
            chain={
              containerId
                ? [...ancestorChain(model, containerId), model.nodes.find((n) => n.id === containerId)!].filter(Boolean)
                : []
            }
            onSelectNode={onSelectNode}
          />
        }
        name={group.name || "Group"}
        typeLine={
          <>
            <span>Group</span>
            {members.length > 0 && (
              <>
                <span className="text-[var(--text-ghost)]">·</span>
                <span>
                  {members.length} member{members.length === 1 ? "" : "s"}
                </span>
              </>
            )}
          </>
        }
        editor={editor}
        editingName={ed.isEditing("title")}
        onToggleName={openTitleEdit}
        onDone={commitTitleEdit}
        onCancel={cancelTitleEdit}
        onNameInput={(v) => {
          if (titleDraft.current) titleDraft.current.name = v;
        }}
        nameMaxLength={NAME_MAX}
      />

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div
          className={`${PAGE_COL} pb-[50px] pt-[18px]`}
          onContextMenu={(e) => openMenu(e, [copyIdItem(group.id, copyId)])}
        >
          <div className="max-w-[900px]">
            <DescriptionSection
              value={group.description}
              prevValue={committed?.groups.find((g) => g.id === group.id)?.description}
              editor={editor}
              editing={ed.isEditing("description")}
              onToggle={() => ed.toggle("description")}
              onCommit={(v) => editor?.updateGroup(group.id, { description: v || undefined })}
            />

            <ResponsibilitiesSection
              host="group"
              hostId={group.id}
              resps={resps}
              prevResps={committedResps}
              plannedHosts={plannedRespHosts(model)}
              concerns={model.concerns ?? []}
              sourceMap={sourceMap}
              testMap={testMap}
              testStates={testStates}
              testVerdicts={props.testVerdicts}
              projectPath={projectPath}
              leafHost={false} // group claims discharge through members
              codeBackedHost // groups organize code-backed members
              mintId={(draft) => nextResponsibilityId(draft, model, committed)}
              editor={editor}
              editing={ed.isEditing("responsibilities")}
              onToggle={() => ed.toggle("responsibilities")}
            />

            <PageSection
              title="Members"
              count={members.length}
              editable={!!editor && members.length > 0}
              editing={ed.isEditing("members")}
              onToggleEdit={() => ed.toggle("members")}
            >
              {ed.isEditing("members") && editor ? (
                <MembersEditor
                  members={members}
                  editor={editor}
                  onSelectNode={onSelectNode}
                  onClose={() => ed.toggle("members")}
                />
              ) : members.length === 0 ? (
                <Empty>No members yet. Add nodes to this group from the tree.</Empty>
              ) : (
                <ol className="-mx-2 flex flex-col">
                  {members.map((m) => (
                    <MemberRow
                      key={m.id}
                      member={m}
                      onSelectNode={onSelectNode}
                    />
                  ))}
                </ol>
              )}
            </PageSection>
          </div>
        </div>
      </div>
    </div>
  );
}

/** One group member as a mono row — the node as a blue wikilink in the shared
 *  marker/number grid, so the Members list reads as the same diff sheet as
 *  everything else on the page. */
function MemberRow({
  member,
  onSelectNode,
  onRemove,
}: {
  member: Node;
  onSelectNode: (id: string) => void;
  onRemove?: () => void;
}) {
  return (
    <li className="group/conn grid grid-cols-[18px_22px_1fr] items-baseline py-[1.5px]">
      <span className="select-none" />
      <span className="select-none" />
      <div className="flex min-w-0 items-baseline font-mono text-sm leading-relaxed">
        <button
          type="button"
          onClick={() => onSelectNode(member.id)}
          title={member.name}
          className={`shrink truncate text-left ${LINK}`}
        >
          {member.name || "Untitled"}
        </button>
        {onRemove && (
          <button
            type="button"
            title="Remove from group"
            onClick={onRemove}
            className="invisible ml-2 shrink-0 rounded p-0.5 text-[var(--text-ghost)] hover:text-red-400 group-hover/conn:visible"
          >
            <X className="h-3 w-3" />
          </button>
        )}
      </div>
    </li>
  );
}

/** The Members form inside the shared {@link SectionEditor} shell — Cancel/Done
 *  ride in the section header. Removals accumulate in a draft (the dropped row
 *  reads struck) and only fire `setNodeGroup(null)` on Done. */
function MembersEditor({
  members,
  editor,
  onSelectNode,
  onClose,
}: {
  members: Node[];
  editor: Editor;
  onSelectNode: (id: string) => void;
  onClose: () => void;
}) {
  const initialIds = members.map((m) => m.id);
  return (
    <SectionEditor<string[]>
      initial={initialIds}
      onCommit={(keptIds) => {
        const kept = new Set(keptIds);
        for (const id of initialIds) if (!kept.has(id)) editor.setNodeGroup(id, null);
      }}
      onClose={onClose}
    >
      {(draft, setDraft) => {
        const kept = members.filter((m) => draft.includes(m.id));
        const dropped = members.filter((m) => !draft.includes(m.id));
        if (members.length === 0) return <Empty>No members.</Empty>;
        return (
          <ol className="-mx-2 flex flex-col">
            {kept.map((m) => (
              <MemberRow
                key={m.id}
                member={m}
                onSelectNode={onSelectNode}
                onRemove={() => setDraft((d) => d.filter((id) => id !== m.id))}
              />
            ))}
            {dropped.map((m) => (
              <li
                key={m.id}
                className="grid grid-cols-[18px_22px_1fr] items-baseline py-[1.5px]"
              >
                <ChangeGlyph kind="delete" />
                <span className="select-none" />
                <div className="flex min-w-0 items-baseline gap-2 font-mono text-sm leading-relaxed">
                  <span className="truncate text-[var(--text-muted)] line-through decoration-red-400/50">
                    {m.name || "Untitled"}
                  </span>
                  <button
                    type="button"
                    onClick={() => setDraft((d) => [...d, m.id])}
                    className={BTN}
                  >
                    Undo
                  </button>
                </div>
              </li>
            ))}
          </ol>
        );
      }}
    </SectionEditor>
  );
}
