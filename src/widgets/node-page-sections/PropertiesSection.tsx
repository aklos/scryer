/**
 * A data-shape symbol's fields, listed as a diff against the base tree
 * (matched by exact label). Read-only: a field is edited in its code.
 */

import type { Node, SchemaProperty } from "../../entities/model/viewmodel";
import {
  buildElementDiff,
  CHANGE_OF,
  ChangeGlyph,
  DIFF_TINT,
  type ElementDiff,
} from "../../entities/model/diffkit";
import { propElementId } from "../source-section/SourceSection";
import { Empty, PageSection, WordDiffText } from "../../shared/ui/pagekit";
import { PROP_ROW } from "./kit";

// --- properties (data shapes) -----------------------------------------------

/** A property's divergence from the base, matched by EXACT label
 *  (props carry no stable id), so a case-only relabel reads as
 *  delete-plus-add. */
interface PropDiffRow extends Omit<ElementDiff<SchemaProperty>, "item"> {
  prop: SchemaProperty;
}

function buildPropDiff(planned: SchemaProperty[], committed: SchemaProperty[]): PropDiffRow[] {
  return buildElementDiff(planned, committed, {
    key: (p) => p.label,
    changed: (prev, p) => prev.description !== p.description,
  }).map(({ item, ...row }) => ({ ...row, prop: item }));
}

/** One property as a mono diff row — the field name and its type, then its description after
 *  an em-dash (word-diffed when reworded), keyed to the shared marker/number
 *  grid so it reads as part of the same diff sheet as the claims. */
function PropDiffRow({ row, nodeId }: { row: PropDiffRow; nodeId: string }) {
  const { prop, kind, prev, index } = row;
  const deleted = kind === "deleted";
  const desc = prop.description ?? "";
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
        {prop.type && <span className="text-[var(--text-tertiary)]">: {prop.type}</span>}
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
      </div>
    </li>
  );
}

export function PropertiesSection({
  node,
  prevProps,
}: {
  node: Node;
  /** This symbol's properties at the base — the diff base; its own
   *  properties while the changes are hidden. */
  prevProps: SchemaProperty[];
}) {
  const properties = node.properties ?? [];
  const diffRows = buildPropDiff(properties, prevProps);

  return (
    <PageSection title="Properties" count={properties.length}>
      {diffRows.length === 0 ? (
        <Empty>No properties.</Empty>
      ) : (
        <ol className="-mx-2 flex flex-col">
          {diffRows.map((row, i) => (
            <PropDiffRow key={i} row={row} nodeId={node.id} />
          ))}
        </ol>
      )}
    </PageSection>
  );
}
