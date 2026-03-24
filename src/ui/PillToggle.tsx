/** Pill-shaped toggle group with semantic color support. */
export type PillVariant = "neutral" | "success" | "info" | "warning";

const variantActive: Record<PillVariant, string> = {
  neutral: "bg-[var(--surface-active)] text-[var(--text-secondary)]",
  success: "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400",
  info: "bg-blue-500/15 text-blue-600 dark:text-blue-400",
  warning: "bg-amber-500/15 text-amber-600 dark:text-amber-400",
};

export function PillToggle<T extends string | undefined>({ options, value, onChange }: {
  options: { value: T; label: string; variant?: PillVariant }[];
  value: T;
  onChange: (value: T) => void;
}) {
  return (
    <div className="grid grid-cols-2 gap-0.5">
      {options.map((opt) => {
        const isActive = value === opt.value;
        const variant = opt.variant ?? "neutral";
        return (
          <button
            key={String(opt.value ?? "__none__")}
            type="button"
            className={`py-1 text-[11px] rounded-md cursor-pointer transition-colors text-center ${
              isActive
                ? variantActive[variant]
                : "text-[var(--text-muted)] hover:text-[var(--text-tertiary)]"
            }`}
            onClick={() => onChange(opt.value)}
          >
            {opt.label}
          </button>
        );
      })}
    </div>
  );
}
