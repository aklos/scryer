import { useRef, useState, useEffect, useCallback } from "react";
import { createPortal } from "react-dom";

export interface MentionItem {
  name: string;
  kind: "person" | "system" | "container" | "component" | "operation" | "process" | "model" | "step";
  ref?: boolean;
}

interface MentionTextareaProps {
  value: string;
  onChange: (value: string) => void;
  mentionNames: MentionItem[];
  placeholder?: string;
  rows?: number;
  autoSize?: boolean;
  className?: string;
}

export function MentionTextarea({ value, onChange, mentionNames, placeholder, rows = 3, autoSize, className }: MentionTextareaProps) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const [localValue, setLocalValue] = useState(value);
  const [triggerPos, setTriggerPos] = useState<number | null>(null);
  const [query, setQuery] = useState("");
  const [dropdownPos, setDropdownPos] = useState<{ top: number; left: number; width: number } | null>(null);
  const isEditingRef = useRef(false);

  // Sync external value in, but only when not actively editing
  useEffect(() => {
    if (!isEditingRef.current) setLocalValue(value);
  }, [value]);

  const resizeTextarea = useCallback(() => {
    const ta = textareaRef.current;
    if (!ta || !autoSize) return;
    ta.style.height = "auto";
    ta.style.height = `${ta.scrollHeight}px`;
  }, [autoSize]);

  // Auto-size on mount and when local value changes
  useEffect(() => { resizeTextarea(); }, [localValue, resizeTextarea]);

  const filtered = triggerPos !== null
    ? mentionNames.filter((item) => item.name.toLowerCase().includes(query.toLowerCase())).slice(0, 6)
    : [];

  const updateDropdownPos = useCallback(() => {
    const ta = textareaRef.current;
    if (!ta) return;
    const rect = ta.getBoundingClientRect();
    setDropdownPos({ top: rect.bottom + 4, left: rect.left, width: rect.width });
  }, []);

  useEffect(() => {
    if (triggerPos !== null && filtered.length > 0) {
      updateDropdownPos();
    }
  }, [triggerPos, filtered.length, updateDropdownPos]);

  const handleChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const val = e.target.value;
    isEditingRef.current = true;
    setLocalValue(val);
    onChange(val);
    // Clear editing flag after React has processed the update
    requestAnimationFrame(() => { isEditingRef.current = false; });

    const cursor = e.target.selectionStart ?? val.length;
    const before = val.slice(0, cursor);
    const atIdx = before.lastIndexOf("@");
    if (atIdx !== -1) {
      const afterAt = before.slice(atIdx);
      if (afterAt.includes("]")) {
        setTriggerPos(null);
        setQuery("");
      } else {
        setTriggerPos(atIdx);
        const raw = before.slice(atIdx + 1);
        setQuery(raw.startsWith("[") ? raw.slice(1) : raw);
      }
    } else {
      setTriggerPos(null);
      setQuery("");
    }
  };

  const insertMention = (name: string) => {
    if (triggerPos === null) return;
    const ta = textareaRef.current;
    const cursor = ta?.selectionStart ?? localValue.length;
    const before = localValue.slice(0, triggerPos);
    const after = localValue.slice(cursor);
    const inserted = `@[${name}]`;
    const newVal = before + inserted + after;
    setLocalValue(newVal);
    onChange(newVal);
    setTriggerPos(null);
    setQuery("");

    requestAnimationFrame(() => {
      if (ta) {
        ta.focus();
        const pos = before.length + inserted.length;
        ta.setSelectionRange(pos, pos);
      }
    });
  };

  const dropdown = triggerPos !== null && filtered.length > 0 && dropdownPos && createPortal(
    <div
      className="fixed max-h-32 overflow-y-auto rounded-md border border-zinc-200 dark:border-zinc-700 bg-white dark:bg-zinc-900 shadow-lg"
      style={{ top: dropdownPos.top, left: dropdownPos.left, width: dropdownPos.width, zIndex: 9999 }}
    >
      {filtered.map((item) => (
        <button
          key={`${item.kind}:${item.name}:${item.ref ?? ""}`}
          type="button"
          className="w-full flex items-center gap-1.5 text-left px-2.5 py-1.5 text-xs text-zinc-700 dark:text-zinc-200 hover:bg-zinc-100 dark:hover:bg-zinc-800 cursor-pointer"
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => insertMention(item.name)}
        >
          {item.ref && <span className="text-[10px] text-zinc-400 dark:text-zinc-500">&rarr;</span>}
          <span className={item.kind === "operation" ? "font-mono" : ""}>{item.name}</span>
          <span className="ml-auto text-[10px] text-zinc-400 dark:text-zinc-500">{item.kind}</span>
        </button>
      ))}
    </div>,
    document.body,
  );

  return (
    <div>
      <textarea
        ref={textareaRef}
        className={className}
        value={localValue}
        placeholder={placeholder}
        rows={rows}
        onChange={handleChange}
        onKeyDown={(e) => {
          if (e.key === "Escape" && triggerPos !== null) {
            setTriggerPos(null);
            setQuery("");
          }
        }}
        onBlur={() => {
          setTimeout(() => { setTriggerPos(null); setQuery(""); }, 150);
        }}
      />
      {dropdown}
    </div>
  );
}
