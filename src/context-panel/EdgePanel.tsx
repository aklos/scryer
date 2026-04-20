import type { C4Edge } from "../types";
import { JsonRoot, JLine, P, K, S, Field, StrEdit } from "./json";

export function EdgePanel({ edge, onUpdate, codeLevel }: {
  edge: C4Edge;
  onUpdate: (data: { label?: string; method?: string }) => void;
  codeLevel?: boolean;
}) {
  const label = edge.data?.label ?? "";
  const method = edge.data?.method ?? "";

  return (
    <JsonRoot>
      <JLine indent={0}><K name="edge" /><P>: {"{"}</P></JLine>
      <Field name="id" indent={1} readOnly><S value={edge.id} /></Field>
      <Field name="source" indent={1} readOnly><S value={edge.source} /></Field>
      <Field name="target" indent={1} readOnly><S value={edge.target} /></Field>
      <Field name="label" indent={1} last={codeLevel}>
        <StrEdit
          value={label}
          onChange={(v) => onUpdate({ label: v, method: method || undefined })}
          placeholder="reads from"
        />
      </Field>
      {!codeLevel && (
        <Field name="method" indent={1} last>
          <StrEdit
            value={method}
            onChange={(v) => onUpdate({ label, method: v || undefined })}
            placeholder="REST/JSON"
          />
        </Field>
      )}
      <JLine indent={0}><P>{"}"}</P></JLine>
    </JsonRoot>
  );
}
