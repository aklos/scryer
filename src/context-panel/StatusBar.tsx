import type { Status } from "../types";
import { STATUS_COLORS } from "../statusColors";

const STATUS_OPTIONS: { value: Status | undefined; label: string }[] = [
  { value: undefined, label: "None" },
  { value: "proposed", label: "Proposed" },
  { value: "implemented", label: "Implemented" },
  { value: "verified", label: "Verified" },
  { value: "vagrant", label: "Vagrant" },
];

export function StatusBar({ value, onChange }: { value: Status | undefined; onChange: (s: Status | undefined) => void }) {
  return (
    <div className="grid grid-cols-3 gap-0.5 rounded-lg bg-[var(--surface-tint)] p-0.5">
      {STATUS_OPTIONS.map((opt) => {
        const isActive = value === opt.value;
        const sc = opt.value ? STATUS_COLORS[opt.value] : null;
        return (
          <button
            key={String(opt.value ?? "__none__")}
            type="button"
            title={opt.label}
            className={`flex items-center justify-center px-1.5 py-1 text-[10px] font-medium rounded-md cursor-pointer ${
              isActive
                ? sc
                  ? `${sc.pillClass} shadow-sm`
                  : "bg-[var(--surface-raised)] text-[var(--text-secondary)] shadow-sm"
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
