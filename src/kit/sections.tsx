import {
  createContext,
  useContext,
  useState,
  type Dispatch,
  type ReactNode,
  type SetStateAction,
} from "react";
import { createPortal } from "react-dom";
import { BTN, BTN_GO, EYEBROW } from "./tokens";
import { EditLink } from "./Editable";

/** A section's edit controls (Cancel/Done) render into the section header's
 *  action lane via this slot — the mockup puts them in the `.h2row`, not the
 *  form footer. PageSection provides the slot element while editing; the form's
 *  {@link SectionEditor} portals its buttons into it (falling back to its own
 *  footer when there's no surrounding PageSection, e.g. the lede). */
const SectionActionsContext = createContext<HTMLElement | null>(null);

/** A wiki-style section: a tiny uppercase eyebrow heading (with an optional
 *  count) on a rule, a hover-revealed [Edit] toggle, then the section body. */
export function PageSection({
  title,
  count,
  right,
  editable,
  editing,
  onToggleEdit,
  children,
}: {
  title: string;
  count?: number;
  right?: ReactNode;
  editable?: boolean;
  editing?: boolean;
  onToggleEdit?: () => void;
  children: ReactNode;
}) {
  // While editing, the header exposes a slot the form's SectionEditor portals
  // its Cancel/Done into (the mockup's `.editbtns` live in the `.h2row`).
  const [actionsSlot, setActionsSlot] = useState<HTMLElement | null>(null);
  return (
    <SectionActionsContext.Provider value={actionsSlot}>
      <section
        data-section={title}
        // `py-2` + compensating margins (mt 26→18, -mb-2) bleed the edit-mode
        // bg 8px above/below the content — the vertical analog of `-mx-3 px-3` —
        // without moving content or changing inter-section spacing.
        className={`group/sec mt-[18px] -mb-2 flow-root -mx-3 rounded-md px-3 py-2 ${
          editing
            ? "bg-[var(--surface-inset)] ring-1 ring-[color-mix(in_srgb,var(--accent)_35%,transparent)]"
            : ""
        }`}
      >
        <div className="mb-2 flex items-end justify-between gap-2 border-b border-[var(--border)] pb-[5px]">
          <h2 className={EYEBROW}>
            {title}
            {count != null && count > 0 && (
              <span className="ml-1.5 font-mono text-2xs font-normal normal-case tracking-normal text-[var(--text-ghost)]">
                {count}
              </span>
            )}
          </h2>
          <div className="flex items-center gap-2 pb-px">
            {right}
            {editing ? (
              // Filled by the form via the actions context (Cancel / Done).
              <span ref={setActionsSlot} className="flex items-center gap-2" />
            ) : (
              editable &&
              onToggleEdit && (
                <EditLink
                  onClick={onToggleEdit}
                  className="invisible group-hover/sec:visible"
                />
              )
            )}
          </div>
        </div>
        {children}
      </section>
    </SectionActionsContext.Provider>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return (
    <div className="font-mono text-xs leading-relaxed text-[var(--text-muted)]">
      {children}
    </div>
  );
}

/** A neutral segmented toggle in the wiki design's chrome — a bordered track
 *  whose active segment fills with the selection surface, like the [edit]
 *  buttons and selected rows on the node pages. The shared ui SegmentedControl
 *  still carries a solid-zinc active fill (off the neutral interaction
 *  contract), so the wiki pages use this instead. */
export function SegField<T extends string | number | undefined>({
  options,
  value,
  disabled,
  onChange,
}: {
  options: { value: T; label: ReactNode }[];
  value: T;
  disabled?: boolean;
  onChange: (value: T) => void;
}) {
  return (
    <div className="flex overflow-hidden rounded border border-[var(--border-strong)]">
      {options.map((opt, i) => {
        const active = value === opt.value;
        return (
          <button
            key={String(opt.value ?? "__none__")}
            type="button"
            disabled={disabled}
            onClick={() => onChange(opt.value)}
            className={`flex-1 px-2 py-1 text-2xs transition-colors disabled:opacity-50 ${
              i > 0 ? "border-l border-[var(--border)]" : ""
            } ${
              active
                ? "bg-[var(--surface-active)] font-medium text-[var(--text)]"
                : "bg-[var(--surface-raised)] text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)]"
            }`}
          >
            {opt.label}
          </button>
        );
      })}
    </div>
  );
}

// --- transactional section editor ---------------------------------------------

/**
 * The one editing shell every section shares. Snapshots `initial` into a local
 * draft (deep copy — the model is JSON-safe), renders the form via
 * `children(draft, setDraft)`, and owns the single exit: Done commits the
 * draft through `onCommit`, Cancel or Esc discards it. Nothing reaches the
 * model until Done.
 */
export function SectionEditor<T>({
  initial,
  onCommit,
  onClose,
  footerExtra,
  children,
}: {
  initial: T;
  onCommit: (draft: T) => void;
  onClose: () => void;
  /** Optional extra footer controls (e.g. an "add row" button), right-aligned. */
  footerExtra?: (setDraft: Dispatch<SetStateAction<T>>) => ReactNode;
  children: (draft: T, setDraft: Dispatch<SetStateAction<T>>) => ReactNode;
}) {
  const [draft, setDraft] = useState<T>(() => structuredClone(initial));
  const slot = useContext(SectionActionsContext);
  const buttons = (
    <>
      <button type="button" data-act="cancel" onClick={onClose} className={BTN}>
        Cancel
      </button>
      <button
        type="button"
        data-act="commit"
        onClick={() => {
          onCommit(draft);
          onClose();
        }}
        className={BTN_GO}
      >
        Done
      </button>
    </>
  );
  return (
    <div
      className="flex flex-col gap-2"
      onKeyDown={(e) => {
        if (e.key === "Escape") {
          e.stopPropagation();
          onClose();
        }
      }}
    >
      {/* Cancel/Done ride in the section header (mockup `.editbtns`); only when
          there's no surrounding PageSection do they fall back to the footer. */}
      {slot && createPortal(buttons, slot)}
      {children(draft, setDraft)}
      {(footerExtra || !slot) && (
        <div className="mt-2 flex items-center gap-2 border-t border-[var(--border-subtle)] pt-2">
          {footerExtra?.(setDraft)}
          {!slot && (
            <>
              <span className="flex-1" />
              {buttons}
            </>
          )}
        </div>
      )}
    </div>
  );
}
