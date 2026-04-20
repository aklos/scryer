import { useState } from "react";
import type { Group } from "../types";
import { Button, Input, Section, Divider } from "../ui";

export function MultiSelectionPanel({ selectedIds, groups, onCreateGroup, onAddToGroup }: {
  selectedIds: string[];
  groups: Group[];
  onCreateGroup: (name: string, memberIds: string[]) => void;
  onAddToGroup: (groupId: string, memberIds: string[]) => void;
}) {
  const [name, setName] = useState("New group");

  return (
    <>
      <span className="text-xs font-medium text-[var(--text-tertiary)]">
        {selectedIds.length} nodes selected
      </span>
      <Divider />
      <Section title="Create group">
        <Input
          variant="title"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Group name..."
          onKeyDown={(e) => {
            if (e.key === "Enter" && name.trim()) {
              onCreateGroup(name.trim(), selectedIds);
              setName("New group");
            }
          }}
        />
        <Button
          variant="primary"
          onClick={() => {
            if (name.trim()) {
              onCreateGroup(name.trim(), selectedIds);
              setName("New group");
            }
          }}
        >
          Create group
        </Button>
      </Section>
      {groups.length > 0 && (
        <>
          <Divider />
          <Section title="Add to existing">
            <div className="flex flex-col gap-1">
              {groups.map((g) => (
                <button
                  key={g.id}
                  type="button"
                  className="flex items-center gap-1.5 rounded-md px-2 py-1.5 text-xs text-left text-[var(--text-secondary)] hover:bg-[var(--surface-hover)] transition-colors cursor-pointer"
                  onClick={() => onAddToGroup(g.id, selectedIds)}
                >
                  <span className="truncate flex-1">{g.name}</span>
                  <span className="text-[10px] text-[var(--text-tertiary)]">{g.memberIds.length}</span>
                </button>
              ))}
            </div>
          </Section>
        </>
      )}
    </>
  );
}
