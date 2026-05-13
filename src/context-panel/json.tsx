import { useState, useRef, useEffect, type ReactNode, type CSSProperties } from "react";
import { createPortal } from "react-dom";

/* ───────────── Color tokens (JSON syntax highlighting) ─────────────
   Tuned to feel like a code editor. Tailwind colors so they pick up
   the theme system's CSS-var overrides automatically. */

export const J_KEY = "text-violet-500 dark:text-violet-300";
export const J_STR = "text-emerald-600 dark:text-emerald-300";
export const J_NUM = "text-amber-600 dark:text-amber-300";
export const J_BOOL = "text-amber-600 dark:text-amber-300";
export const J_NULL = "text-zinc-400 dark:text-zinc-500 italic";
export const J_PUNCT = "text-zinc-400 dark:text-zinc-600";
export const J_COMMENT = "text-zinc-400 dark:text-zinc-500 italic";

/* ───────────── Layout ───────────── */

/** Root JSON view: monospace, small, code-editor look. */
export function JsonRoot({ children }: { children: ReactNode }) {
  return (
    <div className="font-mono text-[11px] leading-[1.35] text-[var(--text)] whitespace-pre">
      {children}
    </div>
  );
}

/** A line of JSON (block-level row). */
export function JLine({ children, indent = 0, className = "" }: { children: ReactNode; indent?: number; className?: string }) {
  const style: CSSProperties = { paddingLeft: `${indent * 1}rem` };
  return (
    <div className={`flex items-baseline ${className}`} style={style}>
      {children}
    </div>
  );
}

/** Punctuation glyphs: { } [ ] : , */
export function P({ children }: { children: ReactNode }) {
  return <span className={J_PUNCT}>{children}</span>;
}

/** A key token: `"name"` */
export function K({ name }: { name: string }) {
  return (
    <>
      <span className={J_PUNCT}>"</span>
      <span className={J_KEY}>{name}</span>
      <span className={J_PUNCT}>"</span>
    </>
  );
}

/** A literal string value: `"value"` */
export function S({ value }: { value: string }) {
  return (
    <>
      <span className={J_PUNCT}>"</span>
      <span className={J_STR}>{value}</span>
      <span className={J_PUNCT}>"</span>
    </>
  );
}

/** A boolean value */
export function B({ value }: { value: boolean }) {
  return <span className={J_BOOL}>{String(value)}</span>;
}

/** Null literal */
export function Null() {
  return <span className={J_NULL}>null</span>;
}

/** A field row: `"key": <value>,` */
export function Field({ name, indent, last, children, tint, readOnly }: { name: string; indent: number; last?: boolean; children: ReactNode; tint?: "add" | "del"; readOnly?: boolean }) {
  const tintClass = tint === "add"
    ? "bg-emerald-500/10"
    : tint === "del"
      ? "bg-red-500/10"
      : "";
  const readOnlyClass = readOnly ? "opacity-50" : "";
  return (
    <JLine indent={indent} className={`${tintClass} ${readOnlyClass}`}>
      <K name={name} />
      <P>: </P>
      <span className="flex-1 min-w-0">{children}</span>
      {!last && <P>,</P>}
    </JLine>
  );
}

/* ───────────── Diff lines ───────────── */

/** A removed JSON line — red bg, leading `-`, strikethrough. Renders the prior value of a field. */
export function DiffOldField({ name, indent, value, kind = "string" }: {
  name: string;
  indent: number;
  value: string;
  kind?: "string" | "enum" | "bool";
}) {
  return (
    <JLine indent={indent} className="bg-red-500/15 line-through opacity-80">
      <span className="text-red-400 mr-1 -ml-3">-</span>
      <K name={name} />
      <P>: </P>
      {kind === "bool" ? (
        <span className={J_BOOL}>{value}</span>
      ) : kind === "enum" && value === "null" ? (
        <span className={J_NULL}>null</span>
      ) : (
        <S value={value} />
      )}
      <P>,</P>
    </JLine>
  );
}

/** A multiline removed line: key on its own line, value as a strikethrough block below. */
export function DiffOldMultiline({ name, indent, value }: { name: string; indent: number; value: string }) {
  return (
    <div className="bg-red-500/15 line-through opacity-80">
      <JLine indent={indent}>
        <span className="text-red-400 mr-1 -ml-3">-</span>
        <K name={name} />
        <P>: </P>
        <P>"</P>
      </JLine>
      <div style={{ paddingLeft: `${(indent + 1)}rem` }} className={`${J_STR} font-mono text-[11px] whitespace-pre-wrap`}>
        {value || "\u00a0"}
      </div>
      <JLine indent={indent}>
        <P>",</P>
      </JLine>
    </div>
  );
}

/* ───────────── Editable values ───────────── */

/** Editable string value, looks like a JSON string `"..."` but click-to-edit. Auto-sizes. */
export function StrEdit({ value, onChange, placeholder, mono = true, transform, className = "" }: {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  mono?: boolean;
  transform?: (raw: string) => string;
  className?: string;
}) {
  const [local, setLocal] = useState(value);
  useEffect(() => { setLocal(value); }, [value]);
  return (
    <span className="inline-flex items-baseline min-w-0 max-w-full rounded-sm hover:bg-[var(--surface-hover)] focus-within:bg-[var(--surface-tint)]">
      <span className={J_PUNCT}>"</span>
      <input
        type="text"
        value={local}
        placeholder={placeholder}
        onChange={(e) => {
          const next = transform ? transform(e.target.value) : e.target.value;
          setLocal(next);
        }}
        onBlur={() => { if (local !== value) onChange(local); }}
        onKeyDown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
          if (e.key === "Escape") { setLocal(value); (e.target as HTMLInputElement).blur(); }
        }}
        size={Math.max((local.length || placeholder?.length || 1), 1)}
        className={`bg-transparent outline-none ${J_STR} ${mono ? "font-mono" : ""} placeholder:text-zinc-500 placeholder:italic ${className}`}
      />
      <span className={J_PUNCT}>"</span>
    </span>
  );
}

/** A multiline field — key on its own line, value rendered as a growing textarea below, indented. */
export function MultilineField({ name, value, onChange, placeholder, indent, last, trailing }: {
  name: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  indent: number;
  last?: boolean;
  trailing?: ReactNode;
}) {
  const [local, setLocal] = useState(value);
  const ref = useRef<HTMLTextAreaElement>(null);
  useEffect(() => { setLocal(value); }, [value]);
  useEffect(() => {
    if (ref.current) {
      ref.current.style.height = "0";
      ref.current.style.height = ref.current.scrollHeight + "px";
    }
  }, [local]);

  return (
    <>
      <JLine indent={indent}>
        <K name={name} />
        <P>: </P>
        <span className={J_PUNCT}>"</span>
        {trailing}
      </JLine>
      <div style={{ paddingLeft: `${(indent + 1)}rem` }}>
        <textarea
          ref={ref}
          value={local}
          placeholder={placeholder}
          rows={1}
          onChange={(e) => setLocal(e.target.value)}
          onBlur={() => { if (local !== value) onChange(local); }}
          className={`w-full bg-transparent outline-none rounded-sm hover:bg-[var(--surface-hover)] focus:bg-[var(--surface-tint)] ${J_STR} font-mono text-[11px] leading-[1.35] resize-none placeholder:text-zinc-500 placeholder:italic`}
        />
      </div>
      <JLine indent={indent}>
        <span className={J_PUNCT}>"</span>
        {!last && <P>,</P>}
      </JLine>
    </>
  );
}

/** Parse a description string into items. Returns `{ items, wasBulleted }`.
 *  - Lines starting with `-` or `*` (after trim) are treated as bullets.
 *  - If any line is bulleted → bullet mode (each line is one item).
 *  - Otherwise → prose mode (the whole string is one item). */
function parseBullets(value: string): { items: string[]; wasBulleted: boolean } {
  if (!value) return { items: [], wasBulleted: false };
  const lines = value.split("\n");
  const hasBullet = lines.some((l) => /^[\-\*]\s/.test(l.trimStart()));
  if (!hasBullet) {
    const trimmed = value.trim();
    return { items: trimmed ? [trimmed] : [], wasBulleted: false };
  }
  const items: string[] = [];
  for (const line of lines) {
    const t = line.trimStart();
    if (/^[\-\*]\s/.test(t)) items.push(t.slice(2).trim());
    // Drop non-bullet lines once we're in bullet mode (no good way to attach them)
  }
  return { items, wasBulleted: true };
}

/** Serialize items back to the description string. Single item with no original bullets → prose. */
function serializeBullets(items: string[], forceBullets: boolean): string {
  const cleaned = items.map((s) => s.trim()).filter((s) => s.length > 0);
  if (cleaned.length === 0) return "";
  if (cleaned.length === 1 && !forceBullets) return cleaned[0];
  return cleaned.map((s) => `- ${s}`).join("\n");
}

/** A list-of-bullets editor for a single description string.
 *  Visually presents as a multiline JSON string. Each bullet is its own input.
 *  Adding a 2nd item promotes a prose value to bulleted on save. */
export function BulletListField({ name, value, onChange, placeholder, indent, last }: {
  name: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  indent: number;
  last?: boolean;
}) {
  const parsed = parseBullets(value);
  // Local state lets the user edit blanks without instantly serializing them away
  const [items, setItems] = useState<string[]>(parsed.items);
  const [wasBulleted, setWasBulleted] = useState(parsed.wasBulleted);
  useEffect(() => {
    const p = parseBullets(value);
    setItems(p.items);
    setWasBulleted(p.wasBulleted);
  }, [value]);

  const commit = (next: string[], forceBullets = wasBulleted) => {
    setItems(next);
    onChange(serializeBullets(next, forceBullets));
  };

  const addItem = () => {
    const next = [...items, ""];
    // Adding a 2nd item promotes to bulleted shape on next commit
    if (next.length >= 2) setWasBulleted(true);
    setItems(next);
  };

  return (
    <>
      <JLine indent={indent}>
        <K name={name} />
        <P>: </P>
        <span className={J_PUNCT}>"</span>
      </JLine>
      <div style={{ paddingLeft: `${(indent + 1)}rem` }} className="flex flex-col">
        {items.length === 0 && (
          <button
            type="button"
            className={`${J_NULL} self-start cursor-pointer rounded-sm hover:bg-[var(--surface-hover)] px-0.5 italic`}
            onClick={addItem}
          >
            {placeholder ?? "(empty)"}
          </button>
        )}
        {items.map((item, i) => (
          <BulletRow
            key={i}
            value={item}
            multi={items.length > 1 || wasBulleted}
            onChange={(v) => {
              const next = items.map((x, j) => j === i ? v : x);
              setItems(next);
            }}
            onBlur={() => commit(items)}
            onRemove={() => {
              const next = items.filter((_, j) => j !== i);
              // If removing brings us back to 1 item, allow prose round-trip
              const force = next.length >= 2;
              if (!force) setWasBulleted(false);
              commit(next, force);
            }}
          />
        ))}
        {items.length > 0 && (
          <button
            type="button"
            className={`${J_NULL} self-start cursor-pointer rounded-sm hover:bg-[var(--surface-hover)] px-0.5 mt-0.5`}
            onClick={addItem}
          >
            + bullet
          </button>
        )}
      </div>
      <JLine indent={indent}>
        <span className={J_PUNCT}>"</span>
        {!last && <P>,</P>}
      </JLine>
    </>
  );
}

function BulletRow({ value, multi, onChange, onBlur, onRemove }: {
  value: string;
  multi: boolean;
  onChange: (v: string) => void;
  onBlur: () => void;
  onRemove: () => void;
}) {
  const ref = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    if (ref.current) {
      ref.current.style.height = "0";
      ref.current.style.height = ref.current.scrollHeight + "px";
    }
  }, [value]);
  return (
    <div className="group flex items-start gap-1">
      {multi && <span className={`${J_PUNCT} pt-[1px] select-none`}>-</span>}
      <textarea
        ref={ref}
        value={value}
        rows={1}
        onChange={(e) => onChange(e.target.value)}
        onBlur={onBlur}
        className={`flex-1 bg-transparent outline-none rounded-sm hover:bg-[var(--surface-hover)] focus:bg-[var(--surface-tint)] ${J_STR} font-mono text-[11px] leading-[1.35] resize-none placeholder:text-zinc-500 placeholder:italic`}
      />
      <button
        type="button"
        className="opacity-0 group-hover:opacity-100 text-[10px] text-zinc-500 hover:text-red-400 cursor-pointer pt-[1px]"
        onClick={onRemove}
        title="Remove bullet"
      >×</button>
    </div>
  );
}

/** Enum string: looks like a string, click reveals dropdown of allowed values. */
export function EnumEdit<T extends string>({ value, options, onChange, allowNone = false }: {
  value: T | undefined;
  options: { value: T; label?: string }[];
  onChange: (v: T | undefined) => void;
  allowNone?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const [menuPos, setMenuPos] = useState<{ top: number; left: number } | null>(null);

  useEffect(() => {
    if (!open) { setMenuPos(null); return; }
    if (triggerRef.current) {
      const r = triggerRef.current.getBoundingClientRect();
      setMenuPos({ top: r.bottom + 2, left: r.left });
    }
    const close = (e: MouseEvent) => {
      const t = e.target as Node;
      if (menuRef.current?.contains(t)) return;
      if (triggerRef.current?.contains(t)) return;
      setOpen(false);
    };
    document.addEventListener("mousedown", close);
    return () => document.removeEventListener("mousedown", close);
  }, [open]);

  return (
    <span className="inline-flex items-baseline">
      <button
        ref={triggerRef}
        type="button"
        className="cursor-pointer rounded-sm hover:bg-[var(--surface-hover)] px-0.5 -mx-0.5"
        onClick={() => setOpen(!open)}
      >
        {value === undefined ? <span className={J_NULL}>null</span> : <S value={value} />}
      </button>
      {open && menuPos && createPortal(
        <div
          ref={menuRef}
          className="fixed z-[100] bg-[var(--surface-raised)] border border-[var(--border)] rounded-md shadow-lg py-1 min-w-[140px] font-mono text-[11px] flex flex-col whitespace-normal"
          style={{ top: menuPos.top, left: menuPos.left }}
        >
          {allowNone && (
            <button
              type="button"
              className="text-left px-2 py-1 hover:bg-[var(--surface-hover)] cursor-pointer"
              onClick={() => { onChange(undefined); setOpen(false); }}
            ><span className={J_NULL}>null</span></button>
          )}
          {options.map((opt) => (
            <button
              key={opt.value}
              type="button"
              className="text-left px-2 py-1 hover:bg-[var(--surface-hover)] cursor-pointer"
              onClick={() => { onChange(opt.value); setOpen(false); }}
            >
              <S value={opt.label ?? opt.value} />
            </button>
          ))}
        </div>,
        document.body,
      )}
    </span>
  );
}

/** Editable boolean: click toggles. */
export function BoolEdit({ value, onChange }: { value: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      type="button"
      className={`${J_BOOL} cursor-pointer rounded-sm hover:bg-[var(--surface-hover)] px-0.5 -mx-0.5`}
      onClick={() => onChange(!value)}
    >
      {String(value)}
    </button>
  );
}

/** A "comment" annotation — `// hint text` */
export function Comment({ text }: { text: string }) {
  return <span className={`${J_COMMENT} ml-2`}>// {text}</span>;
}

/** A collapsible array/object — clickable header. When closed, shows `[…]` or `{…}`. */
export function Collapsible({ open, onToggle, openGlyph, closedGlyph, children }: {
  open: boolean;
  onToggle: () => void;
  openGlyph: ReactNode;
  closedGlyph: ReactNode;
  children: ReactNode;
}) {
  if (!open) {
    return (
      <button type="button" onClick={onToggle} className={`${J_PUNCT} cursor-pointer hover:text-[var(--text-secondary)]`}>
        {closedGlyph}
      </button>
    );
  }
  return (
    <>
      <button type="button" onClick={onToggle} className={`${J_PUNCT} cursor-pointer hover:text-[var(--text-secondary)] inline`}>
        {openGlyph}
      </button>
      {children}
    </>
  );
}
