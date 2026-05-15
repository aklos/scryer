import { useState } from "react";
import type { C4Node, C4NodeData } from "../types";
import { JLine, P, K, S, Field, StrEdit, Collapsible, AddButton, RemoveButton, J_PUNCT } from "./json";
import { sanitizeIdentifier, sanitizeTypeName } from "./utils";

/** Render the `descendants: [...]` section as JSON. For component nodes, items are inline-editable with add buttons. */
export function DescendantsSection({ node, descendants, onUpdateOperationData, indent = 0 }: {
  node: C4Node;
  descendants: C4Node[];
  onUpdateOperationData?: (id: string, data: Record<string, unknown>) => void;
  indent?: number;
}) {
  const [open, setOpen] = useState(true);
  const isComponent = node.data.kind === "component";

  if (descendants.length === 0 && !isComponent) {
    return (
      <Field name="descendants" indent={indent} last>
        <span className={J_PUNCT}>[]</span>
      </Field>
    );
  }

  return (
    <>
      <JLine indent={indent}>
        <K name="descendants" />
        <P>: </P>
        <Collapsible
          open={open}
          onToggle={() => setOpen(!open)}
          openGlyph={"["}
          closedGlyph={`[ … ${descendants.length} ]`}
        >
          {null}
        </Collapsible>
      </JLine>
      {open && (
        <>
          {isComponent ? (
            <ComponentChildrenJson node={node} descendants={descendants} indent={indent + 1} onUpdate={onUpdateOperationData} />
          ) : (
            descendants.map((d, i) => (
              <DescendantRow key={d.id} node={d} indent={indent + 1} last={i === descendants.length - 1} />
            ))
          )}
          <JLine indent={indent}><P>]</P></JLine>
        </>
      )}
    </>
  );
}

function DescendantRow({ node, indent }: { node: C4Node; indent: number; last?: boolean }) {
  const data = node.data as C4NodeData;
  return (
    <JLine indent={indent}>
      <P>{"{ "}</P>
      <K name="id" /><P>: </P><S value={node.id} />
      <P>, </P>
      <K name="kind" /><P>: </P><S value={data.kind} />
      <P>, </P>
      <K name="name" /><P>: </P><S value={data.name} />
      {data.status && (
        <>
          <P>, </P>
          <K name="status" /><P>: </P><S value={data.status} />
        </>
      )}
      <P>{" }"}</P>
    </JLine>
  );
}

function ComponentChildrenJson({ node, descendants, indent, onUpdate }: {
  node: C4Node;
  descendants: C4Node[];
  indent: number;
  onUpdate?: (id: string, data: Record<string, unknown>) => void;
}) {
  // Render children grouped by kind, with inline rename + add buttons.
  const procs = descendants.filter((d) => d.data.kind === "process");
  const mdls = descendants.filter((d) => d.data.kind === "model");
  const fns = descendants.filter((d) => d.data.kind === "operation");
  const ordered = [...procs, ...mdls, ...fns];

  return (
    <>
      {ordered.map((d) => {
        const data = d.data as C4NodeData;
        const transform =
          data.kind === "model" ? sanitizeTypeName :
          data.kind === "operation" ? sanitizeIdentifier :
          undefined;
        return (
          <JLine indent={indent} key={d.id} className="group">
            <P>{"{ "}</P>
            <K name="kind" /><P>: </P><S value={data.kind} />
            <P>, </P>
            <K name="name" /><P>: </P>
            <StrEdit
              value={data.name}
              onChange={(v) => onUpdate?.(d.id, { name: v })}
              transform={transform}
              placeholder="name"
            />
            <P>{" }"}</P>
            <span className="flex-1" />
            <RemoveButton
              onClick={() => window.dispatchEvent(new CustomEvent("remove-node", { detail: { nodeId: d.id } }))}
              title="Remove"
            />
          </JLine>
        );
      })}
      <div style={{ paddingLeft: `${indent}rem` }} className="flex gap-1.5">
        <AddButton label="process" onClick={() => window.dispatchEvent(new CustomEvent("add-process", { detail: { componentId: node.id } }))} />
        <AddButton label="model" onClick={() => window.dispatchEvent(new CustomEvent("add-model", { detail: { componentId: node.id } }))} />
        <AddButton label="operation" onClick={() => window.dispatchEvent(new CustomEvent("add-operation", { detail: { componentId: node.id } }))} />
      </div>
    </>
  );
}
