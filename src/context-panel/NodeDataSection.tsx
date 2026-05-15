import type { C4Node, C4NodeData, Status, C4Shape, ModelProperty } from "../types";
import { useUpdateNodeData, sanitizeIdentifier, sanitizeTypeName } from "./utils";
import { ALL_SHAPES } from "../shapes";
import {
  JsonRoot, JLine, P, K, S, Field, StrEdit, BulletListField, EnumEdit, BoolEdit,
  DiffOldField, DiffOldMultiline, AddButton, RemoveButton,
  J_NULL,
} from "./json";

const STATUS_OPTIONS: { value: Status; label: string }[] = [
  { value: "proposed", label: "proposed" },
  { value: "implemented", label: "implemented" },
  { value: "verified", label: "verified" },
  { value: "vagrant", label: "vagrant" },
];

const SHAPE_OPTIONS: { value: C4Shape }[] = ALL_SHAPES.filter((s) => s !== "person").map((s) => ({ value: s }));

export function NodeDataSection({ node, indent = 1, previousData, onDismissDiff }: {
  node: C4Node;
  indent?: number;
  previousData?: C4NodeData;
  onDismissDiff?: () => void;
}) {
  const updateNodeData = useUpdateNodeData();
  const data = node.data;
  const kind = data.kind;
  const isCodeLevel = kind === "operation" || kind === "process" || kind === "model";
  const showTechnology = kind === "container" || kind === "component";
  const showShape = kind !== "person" && !isCodeLevel;
  const showStatus = kind !== "person" && !data.external;
  const showContract = kind !== "person" && !data.external && kind !== "model";
  const showProperties = kind === "model";
  const showNotes = !isCodeLevel;

  const nameTransform = (raw: string): string => {
    if (kind === "model") return sanitizeTypeName(raw);
    if (kind === "operation") return sanitizeIdentifier(raw);
    return raw;
  };

  // Diff helper — returns the prior value (formatted for display) when a field
  // changed, or null if unchanged / no baseline.
  const diffStr = (key: keyof C4NodeData): string | null => {
    if (!previousData) return null;
    const a = previousData[key];
    const b = data[key];
    if (JSON.stringify(a) === JSON.stringify(b)) return null;
    if (a == null) return "null";
    if (typeof a === "boolean") return String(a);
    if (typeof a === "string") return a;
    return JSON.stringify(a);
  };
  const isDiff = (key: keyof C4NodeData): boolean => diffStr(key) !== null;

  // Build field list dynamically so we can mark the last one (no trailing comma)
  type Row = { render: (last: boolean) => React.ReactNode; key: string };
  const rows: Row[] = [];

  rows.push({ key: "id", render: (last) => (
    <Field name="id" indent={indent + 1} last={last} readOnly><S value={node.id} /></Field>
  )});
  rows.push({ key: "kind", render: (last) => (
    <>
      {isDiff("kind") && <DiffOldField name="kind" indent={indent + 1} value={diffStr("kind")!} />}
      <Field name="kind" indent={indent + 1} last={last} tint={isDiff("kind") ? "add" : undefined} readOnly>
        <S value={kind} />
      </Field>
    </>
  )});
  rows.push({ key: "name", render: (last) => (
    <>
      {isDiff("name") && <DiffOldField name="name" indent={indent + 1} value={diffStr("name")!} />}
      <Field name="name" indent={indent + 1} last={last} tint={isDiff("name") ? "add" : undefined}>
        <StrEdit
          value={data.name}
          onChange={(v) => updateNodeData(node.id, { name: v })}
          transform={nameTransform}
          placeholder={kind === "operation" ? "handleLogin" : kind === "model" ? "UserProfile" : "Name"}
        />
      </Field>
    </>
  )});
  if (showTechnology) {
    rows.push({ key: "technology", render: (last) => (
      <>
        {isDiff("technology") && <DiffOldField name={kind === "component" ? "implements" : "technology"} indent={indent + 1} value={diffStr("technology")!} kind={previousData?.technology == null ? "enum" : "string"} />}
        <Field name={kind === "component" ? "implements" : "technology"} indent={indent + 1} last={last} tint={isDiff("technology") ? "add" : undefined}>
          {data.technology ? (
            <StrEdit
              value={data.technology}
              onChange={(v) => updateNodeData(node.id, { technology: v || undefined })}
              placeholder="Express"
            />
          ) : (
            <NullableStr placeholder="REST API" onSet={(v) => updateNodeData(node.id, { technology: v })} />
          )}
        </Field>
      </>
    )});
  }
  if (kind === "system") {
    rows.push({ key: "external", render: (last) => (
      <>
        {isDiff("external") && <DiffOldField name="external" indent={indent + 1} value={String(!!previousData?.external)} kind="bool" />}
        <Field name="external" indent={indent + 1} last={last} tint={isDiff("external") ? "add" : undefined}>
          <BoolEdit value={!!data.external} onChange={(v) => updateNodeData(node.id, { external: v || undefined, ...(v ? { status: undefined } : {}) })} />
        </Field>
      </>
    )});
  }
  if (showShape) {
    rows.push({ key: "shape", render: (last) => (
      <>
        {isDiff("shape") && <DiffOldField name="shape" indent={indent + 1} value={diffStr("shape")!} kind={previousData?.shape == null ? "enum" : "string"} />}
        <Field name="shape" indent={indent + 1} last={last} tint={isDiff("shape") ? "add" : undefined}>
          <EnumEdit
            value={data.shape}
            options={SHAPE_OPTIONS}
            onChange={(v) => updateNodeData(node.id, { shape: v })}
            allowNone
          />
        </Field>
      </>
    )});
  }
  rows.push({ key: "description", render: (last) => (
    <>
      {isDiff("description") && <DiffOldMultiline name="description" indent={indent + 1} value={previousData?.description ?? ""} />}
      <div className={isDiff("description") ? "bg-emerald-500/10" : ""}>
        <BulletListField
          name="description"
          indent={indent + 1}
          last={last}
          value={data.description ?? ""}
          onChange={(v) => updateNodeData(node.id, { description: v })}
          placeholder="Describe this node..."
        />
      </div>
    </>
  )});
  if (showStatus) {
    rows.push({ key: "status", render: (last) => (
      <>
        {isDiff("status") && <DiffOldField name="status" indent={indent + 1} value={diffStr("status")!} kind={previousData?.status == null ? "enum" : "string"} />}
        <Field name="status" indent={indent + 1} last={last} tint={isDiff("status") ? "add" : undefined}>
          <EnumEdit
            value={data.status}
            options={STATUS_OPTIONS}
            onChange={(v) => updateNodeData(node.id, { status: v, statusReason: undefined })}
            allowNone
          />
          {data.statusReason && (
            <span className="ml-2 text-zinc-500 italic">// {data.statusReason}</span>
          )}
        </Field>
      </>
    )});
  }
  if (showProperties) {
    rows.push({ key: "properties", render: (last) => (
      <PropertiesField node={node} indent={indent + 1} last={last} />
    )});
  }
  if (showNotes) {
    rows.push({ key: "notes", render: (last) => (
      <NotesField node={node} indent={indent + 1} last={last} />
    )});
  }
  // Contract is rendered in its own panel section (see ContractContainer).
  void showContract;

  return (
    <JsonRoot>
      {previousData && onDismissDiff && (
        <div className="flex items-center justify-between gap-2 mb-2 px-2 py-1 rounded border border-amber-500/30 bg-amber-500/10 text-[10px]">
          <span className="text-amber-400 font-mono uppercase tracking-wider">// changed externally</span>
          <button
            type="button"
            className="text-amber-400 hover:text-amber-300 cursor-pointer font-mono text-[10px]"
            onClick={onDismissDiff}
            title="Dismiss diff"
          >dismiss</button>
        </div>
      )}
      <JLine indent={indent}><P>{"{"}</P></JLine>
      {rows.map((row, i) => (
        <div key={row.key}>{row.render(i === rows.length - 1)}</div>
      ))}
      <JLine indent={indent}><P>{"}"}</P></JLine>
    </JsonRoot>
  );
}

/* ───────────── Sub-fields ───────────── */

function NullableStr({ placeholder, onSet }: { placeholder: string; onSet: (v: string) => void }) {
  return (
    <button
      type="button"
      className={`${J_NULL} cursor-pointer rounded-sm hover:bg-[var(--surface-hover)] px-0.5`}
      onClick={() => onSet(placeholder)}
      title="Click to set"
    >null</button>
  );
}

function NotesField({ node, indent }: { node: C4Node; indent: number; last?: boolean }) {
  const updateNodeData = useUpdateNodeData();
  const notes = node.data.notes ?? [];

  const update = (next: string[]) => updateNodeData(node.id, { notes: next.length ? next : undefined });

  return (
    <>
      <JLine indent={indent}><K name="notes" /><P>: </P></JLine>
      <div style={{ paddingLeft: `${(indent + 1)}rem` }} className="flex flex-col">
        {notes.map((note, i) => (
          <div key={i} className="group flex items-start gap-1 py-px">
            <StrEdit
              value={note}
              onChange={(v) => {
                const next = notes.map((x, j) => j === i ? v : x).filter(Boolean);
                update(next);
              }}
              placeholder="note"
            />
            <RemoveButton onClick={() => update(notes.filter((_, j) => j !== i))} title="Remove note" />
          </div>
        ))}
        <AddButton label="note" onClick={() => update([...notes, ""])} />
      </div>
    </>
  );
}

function PropertiesField({ node, indent }: { node: C4Node; indent: number; last?: boolean }) {
  const updateNodeData = useUpdateNodeData();
  const props = (node.data.properties ?? []) as ModelProperty[];

  const update = (next: ModelProperty[]) => updateNodeData(node.id, { properties: next });

  return (
    <>
      <JLine indent={indent}><K name="properties" /><P>: </P></JLine>
      {props.map((p, i) => (
        <div key={i} className="group">
          <JLine indent={indent + 1}>
            <P>{"{"}</P>
            <span className="flex-1" />
            <RemoveButton onClick={() => update(props.filter((_, j) => j !== i))} title="Remove property" />
          </JLine>
          <Field name="label" indent={indent + 2}>
            <StrEdit
              value={p.label}
              onChange={(v) => update(props.map((x, j) => j === i ? { ...x, label: v } : x))}
              transform={sanitizeIdentifier}
              placeholder="propertyName"
            />
          </Field>
          <Field name="description" indent={indent + 2}>
            <StrEdit
              value={p.description}
              onChange={(v) => update(props.map((x, j) => j === i ? { ...x, description: v } : x))}
              placeholder="description"
            />
          </Field>
          <JLine indent={indent + 1}><P>{"}"}</P></JLine>
        </div>
      ))}
      <div style={{ paddingLeft: `${(indent + 1)}rem` }}>
        <AddButton label="property" onClick={() => update([...props, { label: "", description: "" }])} />
      </div>
    </>
  );
}

