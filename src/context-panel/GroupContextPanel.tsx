import type { C4Node, C4NodeData, Contract, Group } from "../types";
import { JsonRoot, JLine, P, K, S, Field, StrEdit, MultilineField, J_NULL, J_PUNCT } from "./json";
import { ContractJson } from "./ContractSection";

const EMPTY_CONTRACT: Contract = { expect: [], ask: [], never: [] };

/** Resolve the contract for a group by node id, returning a defaulted (non-undefined) contract. */
export function getGroupContract(groups: Group[], nodeId: string): Contract {
  const g = groups.find((x) => x.id === nodeId);
  const c = g?.contract;
  return {
    expect: c?.expect ?? [],
    ask: c?.ask ?? [],
    never: c?.never ?? [],
  };
}

type UpdateGroups = (fn: (prev: Group[]) => Group[]) => void;

function patchGroup(onUpdateGroups: UpdateGroups, id: string, patch: Partial<Group>) {
  onUpdateGroups((prev) => prev.map((g) => (g.id === id ? { ...g, ...patch } : g)));
}

/* ── Section 1: identity ───────────────────────────────────────── */

export function GroupIdentitySection({ groupId, groups, onUpdateGroups, indent = 0 }: {
  groupId: string;
  groups: Group[];
  onUpdateGroups: UpdateGroups;
  indent?: number;
}) {
  const group = groups.find((g) => g.id === groupId);
  if (!group) return null;

  return (
    <JsonRoot>
      <JLine indent={indent}><P>{"{"}</P></JLine>
      <Field name="id" indent={indent + 1} readOnly>
        <S value={group.id} />
      </Field>
      <Field name="name" indent={indent + 1}>
        <StrEdit
          value={group.name}
          onChange={(v) => patchGroup(onUpdateGroups, group.id, { name: v })}
          placeholder="Group name"
        />
      </Field>
      <MultilineField
        name="description"
        indent={indent + 1}
        last
        value={group.description ?? ""}
        onChange={(v) => patchGroup(onUpdateGroups, group.id, { description: v || undefined })}
        placeholder="What does this group represent?"
      />
      <JLine indent={indent}><P>{"}"}</P></JLine>
    </JsonRoot>
  );
}

/* ── Section 2: contract ───────────────────────────────────────── */

export function GroupContractContainer({ groupId, groups, onUpdateGroups }: {
  groupId: string;
  groups: Group[];
  onUpdateGroups: UpdateGroups;
}) {
  const group = groups.find((g) => g.id === groupId);
  if (!group) return null;
  const contract = group.contract ?? EMPTY_CONTRACT;

  return (
    <JsonRoot>
      <JLine indent={0}><P>{"{"}</P></JLine>
      <ContractJson
        contract={contract}
        indent={1}
        onChange={(next) => patchGroup(onUpdateGroups, group.id, { contract: next })}
      />
      <JLine indent={0}><P>{"}"}</P></JLine>
    </JsonRoot>
  );
}

/* ── Section 3: members ────────────────────────────────────────── */

export function GroupMembersSection({ groupId, groups, onUpdateGroups, allNodes, indent = 0 }: {
  groupId: string;
  groups: Group[];
  onUpdateGroups: UpdateGroups;
  allNodes: C4Node[];
  indent?: number;
}) {
  const group = groups.find((g) => g.id === groupId);
  if (!group) return null;

  const removeMember = (memberId: string) => {
    onUpdateGroups((prev) => {
      const updated = prev.map((g) =>
        g.id === group.id ? { ...g, memberIds: g.memberIds.filter((id) => id !== memberId) } : g,
      );
      return updated.filter((g) => g.memberIds.length > 0);
    });
  };

  return (
    <JsonRoot>
      <JLine indent={indent}><P>{"{"}</P></JLine>
      {group.memberIds.length === 0 ? (
        <Field name="members" indent={indent + 1} last>
          <span className={J_PUNCT}>[]</span>
        </Field>
      ) : (
        <>
          <JLine indent={indent + 1}><K name="members" /><P>: [</P></JLine>
          {group.memberIds.map((memberId, i) => {
            const memberNode = allNodes.find((n) => n.id === memberId);
            const data = memberNode?.data as C4NodeData | undefined;
            const name = data?.name ?? memberId;
            const isLast = i === group.memberIds.length - 1;
            return (
              <JLine indent={indent + 2} key={memberId} className="group">
                <S value={name} />
                {data?.kind && <span className={`${J_NULL} ml-2`}>// {data.kind}</span>}
                {!isLast && <P>,</P>}
                <button
                  type="button"
                  className="opacity-0 group-hover:opacity-100 ml-2 text-zinc-400 hover:text-red-400 cursor-pointer"
                  title="Remove from group"
                  onClick={() => removeMember(memberId)}
                >×</button>
              </JLine>
            );
          })}
          <JLine indent={indent + 1}><P>]</P></JLine>
        </>
      )}
      <JLine indent={indent}><P>{"}"}</P></JLine>
    </JsonRoot>
  );
}
