import type { Editor } from "../editor";
import {
  DESCRIPTION_MAX,
  Editable,
  EditLink,
  SectionEditor,
  WordDiffText,
} from "../pagekit";

// --- description ------------------------------------------------------------

/** The lede — a title-less paragraph directly under the header, Wikipedia
 *  style, so the kind-specific hero (responsibilities / properties / preview)
 *  is the first titled section the eye lands on. Edits live in a draft and
 *  persist only on Save; Cancel/Esc discards. */
export function DescriptionSection({
  value,
  prevValue,
  editor,
  editing,
  onToggle,
  onCommit,
}: {
  value: string | undefined;
  /** The committed description — when it differs from `value`, the lede shows
   *  the reword inline (word-diff), like a claim. */
  prevValue?: string;
  editor: Editor | undefined;
  editing: boolean;
  onToggle: () => void;
  onCommit: (v: string) => void;
}) {
  if (editing && editor) {
    // The lede edits in place: a contentEditable span with the SAME size/
    // leading/colour as the read paragraph, so the swap doesn't reflow.
    return (
      <SectionEditor<string> initial={value ?? ""} onCommit={onCommit} onClose={onToggle}>
        {(_draft, setDraft) => (
          <Editable
            initial={value ?? ""}
            autoFocus
            maxLength={DESCRIPTION_MAX}
            placeholder="Describe what this is."
            onInput={setDraft}
            className="block text-sm leading-relaxed text-[var(--text-secondary)]"
          />
        )}
      </SectionEditor>
    );
  }
  // A reworded description (committed text present and differing) shows the
  // change inline; an added one (no committed text) reads plain — the node's
  // own diff marker already announces it's new. A CLEARED one (committed text,
  // plan empty) is a reword to nothing: the struck old text must show, or the
  // page reads innocent while the tree and Changes page say modified.
  const reworded = prevValue !== undefined && prevValue !== "" && prevValue !== (value ?? "");
  return (
    <div className="group/lede relative flow-root pr-16">
      <p
        className={`text-sm leading-relaxed ${
          value || reworded ? "text-[var(--text-secondary)]" : "italic text-[var(--text-muted)]"
        }`}
      >
        {reworded ? (
          <WordDiffText from={prevValue!} to={value ?? ""} />
        ) : value ? (
          value
        ) : (
          "No description."
        )}
      </p>
      {editor && (
        <EditLink
          onClick={onToggle}
          className="invisible absolute right-0 top-0 group-hover/lede:visible"
        />
      )}
    </div>
  );
}
