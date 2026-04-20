import { useState, useEffect, useRef } from "react";

export function NotesList({ items, onChange }: { items: string[]; onChange: (items: string[]) => void }) {
  const [focusIdx, setFocusIdx] = useState<number | null>(null);
  const itemRefs = useRef<(HTMLSpanElement | null)[]>([]);

  useEffect(() => {
    if (focusIdx !== null && itemRefs.current[focusIdx]) {
      const el = itemRefs.current[focusIdx];
      el?.focus();
      const range = document.createRange();
      range.selectNodeContents(el!);
      range.collapse(false);
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
      setFocusIdx(null);
    }
  }, [focusIdx, items.length]);

  const commit = (i: number, text: string) => {
    if (text !== items[i]) {
      onChange(items.map((x, j) => j === i ? text : x));
    }
  };

  const bullet = <span className="shrink-0 w-1.5 h-1.5 rounded-full bg-[var(--text-muted)] mt-[7px]" />;

  return (
    <div className="flex flex-col">
      {items.map((item, i) => (
        <div key={i} className="group flex items-start gap-1.5 min-h-[22px] px-1.5 py-1 rounded hover:bg-[var(--surface-hover)]">
          {bullet}
          <span
            ref={(el) => { itemRefs.current[i] = el; }}
            contentEditable
            suppressContentEditableWarning
            className="flex-1 min-w-0 text-xs text-[var(--text-secondary)] caret-current outline-none leading-relaxed break-words"
            onBlur={(e) => commit(i, (e.target as HTMLSpanElement).textContent ?? "")}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                commit(i, (e.target as HTMLSpanElement).textContent ?? "");
                onChange([...items.slice(0, i + 1), "", ...items.slice(i + 1)]);
                setFocusIdx(i + 1);
              }
              if (e.key === "Backspace" && !(e.target as HTMLSpanElement).textContent) {
                e.preventDefault();
                onChange(items.filter((_, j) => j !== i));
                if (i > 0) setFocusIdx(i - 1);
              }
            }}
          >
            {item}
          </span>
          <button
            type="button"
            className="shrink-0 opacity-0 group-hover:opacity-100 text-zinc-400 hover:text-red-400 text-xs cursor-pointer transition-opacity mt-0.5"
            onClick={() => onChange(items.filter((_, j) => j !== i))}
          >
            &times;
          </button>
        </div>
      ))}
      <div className="flex items-start gap-1.5 min-h-[22px] px-1.5 py-1 rounded opacity-40 hover:opacity-70 focus-within:opacity-100 transition-opacity">
        {bullet}
        <span
          contentEditable
          suppressContentEditableWarning
          className="flex-1 min-w-0 text-xs text-[var(--text-muted)] caret-current outline-none leading-relaxed break-words empty:before:content-[attr(data-placeholder)] empty:before:text-[var(--text-muted)]"
          data-placeholder="Add note..."
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              const text = (e.target as HTMLSpanElement).textContent ?? "";
              if (text) {
                onChange([...items, text]);
                (e.target as HTMLSpanElement).textContent = "";
                setFocusIdx(items.length);
              }
            }
          }}
          onInput={(e) => {
            const text = (e.target as HTMLSpanElement).textContent ?? "";
            if (text && text.includes("\n")) {
              onChange([...items, text.replace(/\n/g, "")]);
              (e.target as HTMLSpanElement).textContent = "";
            }
          }}
        />
      </div>
    </div>
  );
}
