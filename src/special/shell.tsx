import { PAGE_COL } from "../pagekit";

// --- shared shell -------------------------------------------------------------

export function SpecialHeader({ title, subtitle }: { title: string; subtitle: string }) {
  return (
    <header className="shrink-0 border-b border-[var(--border)] pb-3 pt-[18px]">
      <div className={PAGE_COL}>
        <h1 className="text-xl font-semibold leading-tight text-[var(--text)]">{title}</h1>
        <div className="mt-1 text-sm text-[var(--text-tertiary)]">{subtitle}</div>
      </div>
    </header>
  );
}

export function SpecialBody({ children }: { children: React.ReactNode }) {
  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <div className={`${PAGE_COL} pb-[50px] pt-[18px]`}>
        <div className="max-w-[820px]">{children}</div>
      </div>
    </div>
  );
}

export function timeLabel(at: number): string {
  return new Date(at).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}
