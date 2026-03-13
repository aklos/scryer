import { useCallback, useEffect, useRef, useState } from "react";
import { ChevronDown } from "lucide-react";

export type SelectOption = {
  value: string;
  label: string;
};

type SelectVariant = "inline" | "bordered";

const triggerClasses: Record<SelectVariant, string> = {
  inline:
    "w-full flex items-center justify-end gap-1 rounded bg-zinc-100/60 dark:bg-zinc-800/60 text-sm text-right text-zinc-700 dark:text-zinc-200 outline-none px-1.5 py-0.5 cursor-pointer",
  bordered:
    "mt-0.5 w-full flex items-center justify-between gap-1 rounded border border-zinc-200 bg-white px-2 py-1.5 text-xs text-zinc-700 outline-none cursor-pointer dark:border-zinc-600 dark:bg-zinc-700 dark:text-zinc-100",
};

export function Select({
  options,
  value,
  onChange,
  placeholder,
  variant = "inline",
  searchable = false,
}: {
  options: SelectOption[];
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  variant?: SelectVariant;
  searchable?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const ref = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  const selected = options.find((o) => o.value === value);

  const filtered = searchable && search
    ? options.filter((o) => o.label.toLowerCase().includes(search.toLowerCase()))
    : options;

  const handleSelect = useCallback(
    (v: string) => {
      onChange(v);
      setOpen(false);
      setSearch("");
    },
    [onChange],
  );

  // Focus search input when dropdown opens
  useEffect(() => {
    if (open && searchable) {
      // Small delay to ensure DOM is ready
      requestAnimationFrame(() => searchRef.current?.focus());
    }
    if (!open) setSearch("");
  }, [open, searchable]);

  // Close on outside click
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  // Close on escape
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [open]);

  return (
    <div ref={ref} className="relative flex-1 min-w-0">
      <button
        type="button"
        className={triggerClasses[variant]}
        onClick={() => setOpen((o) => !o)}
      >
        <span className="truncate">{selected?.label ?? placeholder ?? "Select..."}</span>
        <ChevronDown className={`h-3 w-3 shrink-0 text-zinc-400 dark:text-zinc-500 transition-transform ${open ? "rotate-180" : ""}`} />
      </button>
      {open && (
        <div className={`absolute z-50 mt-1 min-w-full rounded border border-zinc-200 bg-white shadow-lg dark:border-zinc-600 dark:bg-zinc-800 flex flex-col ${variant === "bordered" ? "left-0" : "right-0"}`}>
          {searchable && (
            <div className="p-1.5 border-b border-zinc-100 dark:border-zinc-700">
              <input
                ref={searchRef}
                type="text"
                className="w-full px-2 py-1 text-xs rounded bg-zinc-50 dark:bg-zinc-700 text-zinc-700 dark:text-zinc-200 outline-none placeholder:text-zinc-400 dark:placeholder:text-zinc-500"
                placeholder="Search..."
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
            </div>
          )}
          <div className="max-h-48 overflow-y-auto">
            {filtered.length === 0 ? (
              <div className="px-2.5 py-2 text-xs text-zinc-400 dark:text-zinc-500">No matches</div>
            ) : (
              filtered.map((o) => (
                <button
                  key={o.value}
                  type="button"
                  className={`w-full px-2.5 py-1 text-xs text-left cursor-pointer transition-colors ${
                    o.value === value
                      ? "bg-violet-50 text-violet-600 dark:bg-violet-500/20 dark:text-violet-300"
                      : "text-zinc-700 hover:bg-zinc-50 dark:text-zinc-200 dark:hover:bg-zinc-600"
                  }`}
                  onClick={() => handleSelect(o.value)}
                >
                  {o.label}
                </button>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  );
}
