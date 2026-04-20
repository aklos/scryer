import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { C4Node, C4NodeData, SourceLocation } from "../types";
import { JLine, P, K, S, Field, Collapsible, J_PUNCT, J_NUM } from "./json";

export function SourceMapSection({ sourceMap, allNodes, projectPath, indent = 0 }: {
  sourceMap: Record<string, SourceLocation[]>;
  allNodes: C4Node[];
  projectPath?: string;
  indent?: number;
}) {
  const [open, setOpen] = useState(false);
  const entries = Object.entries(sourceMap).filter(([, locs]) => locs.length > 0);
  if (entries.length === 0) {
    return <Field name="source_map" indent={indent} last><span className={J_PUNCT}>{"{}"}</span></Field>;
  }

  const nameOf = (id: string) => {
    const n = allNodes.find((x) => x.id === id);
    return (n?.data as C4NodeData | undefined)?.name ?? id;
  };

  return (
    <>
      <JLine indent={indent}>
        <K name="source_map" />
        <P>: </P>
        <Collapsible
          open={open}
          onToggle={() => setOpen(!open)}
          openGlyph={"{"}
          closedGlyph={`{ … ${entries.length} }`}
        >{null}</Collapsible>
      </JLine>
      {open && (
        <>
          {entries.map(([nodeId, locs], i) => {
            const isLast = i === entries.length - 1;
            return (
              <div key={nodeId}>
                <JLine indent={indent + 1}>
                  <P>"</P>
                  <span className="text-violet-300">{nameOf(nodeId)}</span>
                  <P>": [</P>
                </JLine>
                {locs.map((loc, j) => {
                  const isGlob = /[*?{}\[\]]/.test(loc.pattern);
                  const isLastLoc = j === locs.length - 1;
                  return (
                    <JLine indent={indent + 2} key={j}>
                      <P>{"{ "}</P>
                      <K name="pattern" /><P>: </P>
                      {isGlob ? (
                        <S value={loc.pattern} />
                      ) : (
                        <button
                          type="button"
                          className="cursor-pointer rounded-sm hover:bg-[var(--surface-hover)] px-0.5"
                          onClick={() => invoke("open_in_editor", { file: loc.pattern, line: loc.line, projectPath }).catch((e) => console.error("open_in_editor:", e))}
                          title={loc.pattern + (loc.line ? `:${loc.line}` : "")}
                        >
                          <S value={loc.pattern} />
                        </button>
                      )}
                      {loc.line != null && (
                        <>
                          <P>, </P>
                          <K name="line" /><P>: </P>
                          <span className={J_NUM}>{loc.line}</span>
                        </>
                      )}
                      <P>{" }"}</P>
                      {!isLastLoc && <P>,</P>}
                    </JLine>
                  );
                })}
                <JLine indent={indent + 1}><P>]</P>{!isLast && <P>,</P>}</JLine>
              </div>
            );
          })}
          <JLine indent={indent}><P>{"}"}</P></JLine>
        </>
      )}
    </>
  );
}
