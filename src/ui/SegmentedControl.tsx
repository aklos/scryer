/** Bordered segmented control — a row of mutually exclusive buttons. */
export function SegmentedControl<T extends string | undefined>({ options, value, onChange }: {
  options: { value: T; label: string }[];
  value: T;
  onChange: (value: T) => void;
}) {
  return (
    <div className="flex rounded border border-[var(--border-strong)] overflow-hidden">
      {options.map((opt) => (
        <button
          key={String(opt.value ?? "__none__")}
          type="button"
          className={`flex-1 px-2 py-1 text-xs cursor-pointer transition-colors ${
            value === opt.value
              ? "bg-zinc-700 text-white dark:bg-zinc-200 dark:text-zinc-900"
              : "bg-[var(--surface-raised)] text-[var(--text-secondary)] hover:bg-[var(--surface)]"
          }`}
          onClick={() => onChange(opt.value)}
        >
          {opt.label}
        </button>
      ))}
    </div>
  );
}
