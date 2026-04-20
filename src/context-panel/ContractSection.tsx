import { useRef } from "react";
import type { Contract, ContractItem, ContractImage } from "../types";
import { contractText, contractPassed, contractUrl } from "../types";
import { resizeImage } from "./utils";
import { JLine, P, K, Field, StrEdit, BoolEdit, J_PUNCT, J_NULL } from "./json";

function getImage(item: ContractItem): ContractImage | undefined {
  return typeof item === "string" ? undefined : item.image;
}

/** Render a Contract as JSON inside an existing parent block. Renders three keys: expect, ask, never. */
export function ContractJson({ contract, onChange, indent }: {
  contract: Contract;
  onChange: (next: Contract) => void;
  indent: number;
}) {
  const update = (field: keyof Contract, items: ContractItem[]) =>
    onChange({ ...contract, [field]: items });

  return (
    <>
      <ContractListJson name="expect" items={contract.expect} onChange={(items) => update("expect", items)} indent={indent} showPassed />
      <ContractListJson name="ask" items={contract.ask} onChange={(items) => update("ask", items)} indent={indent} />
      <ContractListJson name="never" items={contract.never} onChange={(items) => update("never", items)} indent={indent} last />
    </>
  );
}

function ContractListJson({ name, items, onChange, indent, showPassed = false, last = false }: {
  name: string;
  items: ContractItem[];
  onChange: (items: ContractItem[]) => void;
  indent: number;
  showPassed?: boolean;
  last?: boolean;
}) {
  const addItem = () => onChange([...items, { text: "" }]);

  if (items.length === 0) {
    return (
      <Field name={name} indent={indent} last={last}>
        <button
          type="button"
          className={`${J_PUNCT} cursor-pointer hover:text-[var(--text-secondary)]`}
          onClick={addItem}
        >[]</button>
      </Field>
    );
  }

  return (
    <>
      <JLine indent={indent}><K name={name} /><P>: [</P></JLine>
      {items.map((item, i) => (
        <ContractItemJson
          key={i}
          item={item}
          indent={indent + 1}
          showPassed={showPassed}
          last={i === items.length - 1}
          onPatch={(patch) => {
            const base = typeof item === "string" ? { text: item } : { ...item };
            onChange(items.map((x, j) => j === i ? { ...base, ...patch } : x));
          }}
          onRemove={() => onChange(items.filter((_, j) => j !== i))}
        />
      ))}
      <JLine indent={indent + 1}>
        <button type="button" className={`${J_NULL} cursor-pointer rounded-sm hover:bg-[var(--surface-hover)] px-0.5`} onClick={addItem}>
          + {"{...}"}
        </button>
      </JLine>
      <JLine indent={indent}><P>]</P>{!last && <P>,</P>}</JLine>
    </>
  );
}

function ContractItemJson({ item, indent, showPassed, last, onPatch, onRemove }: {
  item: ContractItem;
  indent: number;
  showPassed: boolean;
  last: boolean;
  onPatch: (patch: Partial<{ text: string; passed: boolean | undefined; url: string | undefined; image: ContractImage | undefined }>) => void;
  onRemove: () => void;
}) {
  const text = contractText(item);
  const passed = contractPassed(item);
  const url = contractUrl(item);
  const image = getImage(item);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const handleFile = (e: React.ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0];
    if (!f) return;
    resizeImage(f).then((img) => onPatch({ image: img }));
    e.target.value = "";
  };

  // Build sub-fields list for proper comma handling
  const fields: { name: string; render: (last: boolean) => React.ReactNode }[] = [];
  fields.push({ name: "text", render: (l) => (
    <Field name="text" indent={indent + 1} last={l}>
      <StrEdit value={text} onChange={(v) => onPatch({ text: v })} placeholder="..." />
    </Field>
  )});
  if (showPassed && passed !== undefined) {
    fields.push({ name: "passed", render: (l) => (
      <Field name="passed" indent={indent + 1} last={l}>
        <BoolEdit value={passed} onChange={(v) => onPatch({ passed: v })} />
      </Field>
    )});
  }
  if (url !== undefined) {
    fields.push({ name: "url", render: (l) => (
      <Field name="url" indent={indent + 1} last={l}>
        <StrEdit value={url} onChange={(v) => onPatch({ url: v || undefined })} placeholder="https://..." />
      </Field>
    )});
  }
  if (image) {
    fields.push({ name: "image", render: (l) => (
      <Field name="image" indent={indent + 1} last={l}>
        <span className="inline-flex items-center gap-1.5">
          <span className={J_PUNCT}>"</span>
          <span className="text-emerald-400">{image.filename}</span>
          <span className={J_PUNCT}>"</span>
          <button
            type="button"
            className="ml-1 text-zinc-400 hover:text-red-400 cursor-pointer"
            onClick={() => onPatch({ image: undefined })}
            title="Remove image"
          >×</button>
        </span>
      </Field>
    )});
  }

  return (
    <div className="group">
      <JLine indent={indent}><P>{"{"}</P></JLine>
      {fields.map((f, i) => <div key={f.name}>{f.render(i === fields.length - 1)}</div>)}
      {image && (
        <div style={{ paddingLeft: `${(indent + 1)}rem` }} className="my-1">
          <img src={`data:${image.mimeType};base64,${image.data}`} alt={image.filename} className="rounded border border-[var(--border)] max-h-24" />
        </div>
      )}
      <JLine indent={indent}>
        <P>{"}"}</P>{!last && <P>,</P>}
        <span className="opacity-0 group-hover:opacity-100 ml-2 inline-flex gap-1.5">
          {showPassed && (
            <button
              type="button"
              className="text-[10px] text-zinc-500 hover:text-emerald-400 cursor-pointer"
              onClick={() => onPatch({ passed: passed === undefined ? true : passed === true ? false : undefined })}
              title="Toggle pass/fail"
            >{passed === undefined ? "+passed" : passed ? "✓" : "✗"}</button>
          )}
          {url === undefined && (
            <button
              type="button"
              className="text-[10px] text-zinc-500 hover:text-blue-400 cursor-pointer"
              onClick={() => onPatch({ url: "" })}
            >+url</button>
          )}
          {!image && (
            <button
              type="button"
              className="text-[10px] text-zinc-500 hover:text-blue-400 cursor-pointer"
              onClick={() => fileInputRef.current?.click()}
            >+image</button>
          )}
          <button
            type="button"
            className="text-[10px] text-zinc-500 hover:text-red-400 cursor-pointer"
            onClick={onRemove}
          >×</button>
        </span>
      </JLine>
      <input ref={fileInputRef} type="file" accept="image/*" className="hidden" onChange={handleFile} />
    </div>
  );
}

/** Tiny dot for tab badges (still used by GroupContextPanel) */
export function ContractStatusDot({ contract }: { contract?: Contract }) {
  if (!contract) return null;
  const expectItems = contract.expect ?? [];
  const totalItems = expectItems.length + (contract.ask?.length ?? 0) + (contract.never?.length ?? 0);
  if (totalItems === 0) return null;
  let passed = 0, failed = 0;
  for (const item of expectItems) {
    const p = contractPassed(item);
    if (p === true) passed++;
    else if (p === false) failed++;
  }
  const color = failed > 0
    ? "bg-red-500"
    : expectItems.length > 0 && passed === expectItems.length
      ? "bg-emerald-500"
      : "bg-[var(--border-strong)]";
  return <span className={`inline-block w-1.5 h-1.5 rounded-full ${color}`} />;
}
