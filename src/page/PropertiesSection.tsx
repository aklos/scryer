import { Plus, Trash2 } from "lucide-react";
import type { Node, SchemaProperty } from "../viewmodel";
import type { Editor } from "../editor";
import { FLAG_COLORS } from "../statusColors";
import {
  buildElementDiff,
  CHANGE_OF,
  ChangeGlyph,
  DIFF_TINT,
  type ElementDiff,
  VerdictBar,
} from "../diffkit";
import { propElementId } from "../SourceSection";
import {
  BTN,
  BTN_DANGER,
  BTN_GO,
  CTL,
  Editable,
  Empty,
  PageSection,
  SectionEditor,
  WordDiffText,
} from "../pagekit";
import { PROP_ROW, RESP_ROW } from "./kit";

// --- properties (data shapes) -----------------------------------------------

/** A property's divergence from the committed model, matched by EXACT label
 *  (props carry no stable id) — the same keying as planDiff.ts and diff.rs —
 *  so a case-only relabel reads as delete-plus-add on every surface, never
 *  "clean" here while the tree, Changes page, and get_pending say modified. */
interface PropDiffRow extends Omit<ElementDiff<SchemaProperty>, "item"> {
  prop: SchemaProperty;
}

function buildPropDiff(planned: SchemaProperty[], committed: SchemaProperty[]): PropDiffRow[] {
  return buildElementDiff(planned, committed, {
    key: (p) => p.label,
    changed: (prev, p) => prev.description !== p.description,
    // A vagrant field is code-discovered (the "?" drift kind), never numbered —
    // mirrors how a vagrant responsibility classifies.
    vagrant: (p) => !!p.vagrant,
  }).map(({ item, ...row }) => ({ ...row, prop: item }));
}

/** One property as a mono diff row — the field name, then its description after
 *  an em-dash (word-diffed when reworded), keyed to the shared marker/number
 *  grid so it reads as part of the same diff sheet as the responsibilities. */
function PropDiffRow({
  row,
  nodeId,
  editor,
}: {
  row: PropDiffRow;
  nodeId: string;
  editor?: Editor;
}) {
  const { prop, kind, prev, index } = row;
  const deleted = kind === "deleted";
  const desc = prop.description ?? "";
  // A vagrant field is code-first ("adopt?"); a stale one is a regressed field
  // ("re-implement / drop") — the property-level mirror of the claim verdicts.
  const reviewable = kind === "vagrant" && !!editor;
  // Same whole-row treatment as the claim rows: added green, dropped struck
  // red, unchanged quiet — one diff language across the sections.
  const contentColor = deleted
    ? DIFF_TINT.delete
    : kind === "added"
      ? DIFF_TINT.add
      : kind === "unchanged"
        ? "text-[var(--text-secondary)]"
        : "text-[var(--text)]";
  return (
    <li id={propElementId(nodeId, prop.label)} className={`${PROP_ROW} rounded py-[1.5px]`}>
      {kind === "unchanged" ? <span /> : <ChangeGlyph kind={CHANGE_OF[kind]} />}
      <span className="select-none pr-2.5 text-right font-mono text-2xs tabular-nums text-[var(--text-ghost)]">
        {index}
      </span>
      <div className="min-w-0 font-mono text-sm leading-relaxed">
        <span className={`font-medium ${contentColor}`}>{prop.label || "field"}</span>
        {(desc || (kind === "reworded" && prev?.description)) && (
          <span className="text-[var(--text-tertiary)]">
            {" "}—{" "}
            {kind === "reworded" && prev ? (
              <WordDiffText from={prev.description ?? ""} to={desc} />
            ) : (
              desc
            )}
          </span>
        )}
        {/* Chip only in read-only mode — the verdict bar below announces the
            same fact when it can render. */}
        {prop.stale && !editor && (
          <span
            className={`${FLAG_COLORS.stale.pill} ml-2 align-middle`}
            title="Drift check: the field backing this property is gone or changed."
          >
            stale
          </span>
        )}
        {prop.stale && editor && (
          <VerdictBar hint="Drift says this field is gone" tone="drift">
            <button
              type="button"
              onClick={() => editor.dropProperty(nodeId, prop.label)}
              className={BTN_DANGER}
              title="The field was removed on purpose — drop the property from the model."
            >
              Drop
            </button>
            <button
              type="button"
              onClick={() => editor.reimplementProperty(nodeId, prop.label)}
              className={BTN}
              title="Keep this property and rebuild the field in code — files a to-do the agent implements (folds back when done)."
            >
              Rebuild code
            </button>
          </VerdictBar>
        )}
        {reviewable && (
          <VerdictBar hint="In the code, not in the model" className="-mr-[180px]">
            <button
              type="button"
              onClick={() => editor!.adoptProperty(nodeId, prop.label)}
              className={BTN_GO}
              title="Accept this discovered field into the contract — commits it to the model now (the code already exists)"
            >
              Adopt
            </button>
            <button
              type="button"
              onClick={() => editor!.rejectProperty(nodeId, prop.label)}
              className={BTN_DANGER}
              title="Mark the field for deletion — this data is not wanted (folds it in, then schedules its removal)"
            >
              Reject
            </button>
          </VerdictBar>
        )}
      </div>
    </li>
  );
}

/** One draft row of the properties form — field + description as flush ghost
 *  fields in the mono content lane, with a hover-revealed delete in its own
 *  control lane (identical grid to {@link PropDiffRow}, so read↔edit doesn't
 *  reflow). */
function PropertyEditRow({
  prop,
  index,
  autoFocus,
  onPatch,
  onRemove,
}: {
  prop: SchemaProperty;
  index: number;
  autoFocus: boolean;
  onPatch: (patch: Partial<SchemaProperty>) => void;
  onRemove: () => void;
}) {
  const FIELD_HL =
    "group-hover/erow:bg-[color-mix(in_srgb,var(--text)_6%,transparent)] focus:bg-[var(--surface-field)]";
  // Mirrors PropDiffRow's content cell — `label — description` inline in the
  // mono lane — with the two fields as contentEditable spans and the delete in
  // the row's reserved trailing control column (CTL).
  return (
    <li className={`group/erow ${PROP_ROW} py-[1.5px]`}>
      <span className="select-none text-center font-mono text-xs" />
      <span className="select-none pr-2.5 text-right font-mono text-2xs tabular-nums text-[var(--text-ghost)]">
        {index}
      </span>
      <div className="min-w-0 font-mono text-sm leading-relaxed">
        <Editable
          initial={prop.label}
          autoFocus={autoFocus}
          placeholder="field"
          onInput={(t) => onPatch({ label: t })}
          className={`font-medium text-[var(--text)] ${FIELD_HL}`}
        />
        <span className="text-[var(--text-tertiary)]"> — </span>
        <Editable
          initial={prop.description ?? ""}
          placeholder="description"
          onInput={(t) => onPatch({ description: t })}
          className={`text-[var(--text-tertiary)] ${FIELD_HL}`}
        />
      </div>
      <span className={CTL}>
        <button type="button" title="Delete property" onClick={onRemove} className={BTN_DANGER}>
          <Trash2 className="h-3 w-3" /> Delete
        </button>
      </span>
    </li>
  );
}

export function PropertiesSection({
  node,
  prevProps,
  editor,
  editing,
  onToggle,
}: {
  node: Node;
  /** The committed copy of this symbol's properties — the diff base. */
  prevProps: SchemaProperty[];
  editor: Editor | undefined;
  editing: boolean;
  onToggle: () => void;
}) {
  const properties = node.properties ?? [];
  const diffRows = buildPropDiff(properties, prevProps);

  return (
    <PageSection
      title="Properties"
      count={properties.length}
      editable={!!editor}
      editing={editing}
      onToggleEdit={onToggle}
    >
      {editing && editor ? (
        <SectionEditor<(SchemaProperty & { removed?: boolean })[]>
          initial={properties}
          onCommit={(draft) => {
            const cleaned = draft
              .filter((p) => !p.removed)
              .filter((p) => p.label.trim() !== "" || (p.description ?? "").trim() !== "")
              .map(({ removed: _removed, ...p }) => ({ ...p, label: p.label.trim() }));
            editor.updateNode(node.id, { properties: cleaned });
          }}
          onClose={onToggle}
          footerExtra={(setDraft) => (
            <button
              type="button"
              onClick={() =>
                setDraft((d) => [...d, { label: "", description: "" }])
              }
              className={BTN}
            >
              <Plus className="h-3 w-3" /> Add property
            </button>
          )}
        >
          {(draft, setDraft) => {
            const patchRow = (i: number, patch: Partial<SchemaProperty & { removed?: boolean }>) =>
              setDraft((d) => d.map((p, j) => (j === i ? { ...p, ...patch } : p)));
            return draft.length === 0 ? (
              <Empty>No properties.</Empty>
            ) : (
              <ul className="-mx-2 flex flex-col">
                {draft.map((p, i) =>
                  p.removed ? (
                    // Soft-deleted: struck with Undo — same recipe as claims.
                    <li key={i} className={`${RESP_ROW} py-[1.5px]`}>
                      <ChangeGlyph kind="delete" />
                      <span className="select-none" />
                      <div className="flex min-w-0 items-baseline gap-2 font-mono text-sm leading-relaxed">
                        <span className="min-w-0 truncate text-[var(--text-muted)] line-through decoration-red-400/50">
                          {p.label.trim() || "(empty)"}
                        </span>
                        <button
                          type="button"
                          onClick={() => patchRow(i, { removed: false })}
                          className={BTN}
                        >
                          Undo
                        </button>
                      </div>
                    </li>
                  ) : (
                    <PropertyEditRow
                      key={i}
                      prop={p}
                      index={i + 1}
                      autoFocus={p.label === "" && i === draft.length - 1}
                      onPatch={(patch) => patchRow(i, patch)}
                      onRemove={() => patchRow(i, { removed: true })}
                    />
                  ),
                )}
              </ul>
            );
          }}
        </SectionEditor>
      ) : diffRows.length === 0 ? (
        <Empty>No properties.</Empty>
      ) : (
        <ol className="-mx-2 flex flex-col">
          {diffRows.map((row, i) => (
            <PropDiffRow key={i} row={row} nodeId={node.id} editor={editor} />
          ))}
        </ol>
      )}
    </PageSection>
  );
}
