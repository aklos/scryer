import type { Hint } from "../types";
import { Section } from "../ui";

export function HintsFooter({ hints, onFixHint, onDismissHint }: { hints: Hint[]; onFixHint: (hint: Hint) => void; onDismissHint: (hint: Hint) => void }) {
  if (hints.length === 0) return null;
  return (
    <div className="shrink-0 border-t border-[var(--border)] max-h-[40%] overflow-y-auto">
      <div className="px-4 py-2 flex flex-col gap-1.5">
        <Section title="Hints" count={hints.length}>
          <div className="flex flex-col gap-1.5">
            {hints.map((hint, i) => (
              <div
                key={i}
                className={`rounded-md border px-2 py-1.5 text-xs leading-relaxed ${
                  hint.severity === "warning"
                    ? "border-orange-200 bg-orange-50 text-orange-700 dark:border-orange-800 dark:bg-orange-950/40 dark:text-orange-300"
                    : "border-teal-200 bg-teal-50 text-teal-700 dark:border-teal-800 dark:bg-teal-950/40 dark:text-teal-300"
                }`}
              >
                <div className="flex justify-between items-start gap-1">
                  <span>{hint.message}</span>
                  <button
                    type="button"
                    className="shrink-0 text-xs opacity-40 hover:opacity-80 cursor-pointer leading-none"
                    onClick={() => onDismissHint(hint)}
                    title="Dismiss"
                  >
                    &times;
                  </button>
                </div>
                {hint.action && (
                  <button
                    type="button"
                    className="mt-1 block rounded bg-[var(--surface-overlay)] px-1.5 py-0.5 text-[11px] font-medium hover:bg-[var(--surface-raised)] cursor-pointer"
                    onClick={() => onFixHint(hint)}
                  >
                    Fix
                  </button>
                )}
              </div>
            ))}
          </div>
        </Section>
      </div>
    </div>
  );
}
