import { useState } from "react";
import type { C4Node, C4NodeData, C4Edge } from "../types";
import type { ExternalEdge } from "../hooks/useNodeContext";
import { JLine, P, K, S, Field, Collapsible, J_PUNCT } from "./json";

export function InternalEdgesSection({ edges, allNodes, indent = 0 }: { edges: C4Edge[]; allNodes: C4Node[]; indent?: number }) {
  const [open, setOpen] = useState(false);
  if (edges.length === 0) {
    return <Field name="internal_edges" indent={indent}><span className={J_PUNCT}>[]</span></Field>;
  }

  const nameOf = (id: string) => {
    const n = allNodes.find((x) => x.id === id);
    return (n?.data as C4NodeData | undefined)?.name ?? id;
  };

  return (
    <>
      <JLine indent={indent}>
        <K name="internal_edges" />
        <P>: </P>
        <Collapsible
          open={open}
          onToggle={() => setOpen(!open)}
          openGlyph="["
          closedGlyph={`[ … ${edges.length} ]`}
        >{null}</Collapsible>
      </JLine>
      {open && (
        <>
          {edges.map((e, i) => {
            const isLast = i === edges.length - 1;
            return (
              <JLine indent={indent + 1} key={e.id}>
                <P>{"{ "}</P>
                <K name="source" /><P>: </P><S value={nameOf(e.source)} />
                <P>, </P>
                <K name="target" /><P>: </P><S value={nameOf(e.target)} />
                {e.data?.label && (
                  <>
                    <P>, </P>
                    <K name="label" /><P>: </P><S value={e.data.label} />
                  </>
                )}
                <P>{" }"}</P>
                {!isLast && <P>,</P>}
              </JLine>
            );
          })}
          <JLine indent={indent}><P>],</P></JLine>
        </>
      )}
    </>
  );
}

export function ExternalEdgesSection({ edges, indent = 0 }: { edges: ExternalEdge[]; indent?: number }) {
  const [open, setOpen] = useState(false);
  if (edges.length === 0) {
    return <Field name="external_edges" indent={indent}><span className={J_PUNCT}>[]</span></Field>;
  }

  return (
    <>
      <JLine indent={indent}>
        <K name="external_edges" />
        <P>: </P>
        <Collapsible
          open={open}
          onToggle={() => setOpen(!open)}
          openGlyph="["
          closedGlyph={`[ … ${edges.length} ]`}
        >{null}</Collapsible>
      </JLine>
      {open && (
        <>
          {edges.map((e, i) => {
            const isLast = i === edges.length - 1;
            return (
              <JLine indent={indent + 1} key={e.id}>
                <P>{"{ "}</P>
                <K name="direction" /><P>: </P><S value={e.direction} />
                <P>, </P>
                <K name="external_node_name" /><P>: </P><S value={e.externalNodeName} />
                <P>, </P>
                <K name="external_node_kind" /><P>: </P><S value={e.externalNodeKind} />
                {e.data?.label && (
                  <>
                    <P>, </P>
                    <K name="label" /><P>: </P><S value={e.data.label} />
                  </>
                )}
                <P>{" }"}</P>
                {!isLast && <P>,</P>}
              </JLine>
            );
          })}
          <JLine indent={indent}><P>],</P></JLine>
        </>
      )}
    </>
  );
}
