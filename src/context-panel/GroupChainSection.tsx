import type { Group } from "../types";
import { contractText } from "../types";
import { JLine, P, K, S, J_NULL, J_PUNCT } from "./json";

/**
 * Renders the group-membership chain for a selected node, mirroring what the
 * AI receives via `get_node`. Immediate group first, then ancestors via
 * parentGroupId. Each entry shows name, description, and contract so
 * inherited rules are visible to the user.
 */
export function GroupChainSection({
  nodeId,
  groups,
  indent = 0,
}: {
  nodeId: string;
  groups: Group[];
  indent?: number;
}) {
  const chain: Group[] = [];
  const seen = new Set<string>();
  let cursor: Group | undefined = groups.find((g) => g.memberIds.includes(nodeId));
  while (cursor) {
    if (seen.has(cursor.id)) break;
    seen.add(cursor.id);
    chain.push(cursor);
    cursor = cursor.parentGroupId
      ? groups.find((g) => g.id === cursor!.parentGroupId)
      : undefined;
  }

  if (chain.length === 0) {
    return (
      <JLine indent={indent}>
        <K name="groups" />
        <P>: </P>
        <span className={J_PUNCT}>[]</span>
        <P>,</P>
      </JLine>
    );
  }

  return (
    <>
      <JLine indent={indent}>
        <K name="groups" />
        <P>: [</P>
      </JLine>
      {chain.map((g, gi) => {
        const last = gi === chain.length - 1;
        const contract = g.contract;
        const hasContract =
          contract &&
          ((contract.expect?.length ?? 0) > 0 ||
            (contract.ask?.length ?? 0) > 0 ||
            (contract.never?.length ?? 0) > 0);
        return (
          <div key={g.id}>
            <JLine indent={indent + 1}>
              <P>{"{ "}</P>
              <K name="name" />
              <P>: </P>
              <S value={g.name} />
              {g.description && (
                <>
                  <P>, </P>
                  <K name="description" />
                  <P>: </P>
                  <S value={g.description} />
                </>
              )}
              {!hasContract && <P>{" }"}</P>}
              {!hasContract && !last && <P>,</P>}
            </JLine>
            {hasContract && (
              <>
                <JLine indent={indent + 2}>
                  <K name="contract" />
                  <P>: {"{"}</P>
                </JLine>
                {(contract!.expect?.length ?? 0) > 0 && (
                  <>
                    <JLine indent={indent + 3}>
                      <K name="expect" />
                      <P>: [</P>
                    </JLine>
                    {contract!.expect.map((item, i) => (
                      <JLine indent={indent + 4} key={`e-${i}`}>
                        <S value={contractText(item)} />
                        {i < contract!.expect.length - 1 && <P>,</P>}
                      </JLine>
                    ))}
                    <JLine indent={indent + 3}>
                      <P>
                        ]
                        {(contract!.ask?.length ?? 0) > 0 ||
                        (contract!.never?.length ?? 0) > 0
                          ? ","
                          : ""}
                      </P>
                    </JLine>
                  </>
                )}
                {(contract!.ask?.length ?? 0) > 0 && (
                  <>
                    <JLine indent={indent + 3}>
                      <K name="ask" />
                      <P>: [</P>
                    </JLine>
                    {contract!.ask.map((item, i) => (
                      <JLine indent={indent + 4} key={`a-${i}`}>
                        <S value={contractText(item)} />
                        {i < contract!.ask.length - 1 && <P>,</P>}
                      </JLine>
                    ))}
                    <JLine indent={indent + 3}>
                      <P>]{(contract!.never?.length ?? 0) > 0 ? "," : ""}</P>
                    </JLine>
                  </>
                )}
                {(contract!.never?.length ?? 0) > 0 && (
                  <>
                    <JLine indent={indent + 3}>
                      <K name="never" />
                      <P>: [</P>
                    </JLine>
                    {contract!.never.map((item, i) => (
                      <JLine indent={indent + 4} key={`n-${i}`}>
                        <S value={contractText(item)} />
                        {i < contract!.never.length - 1 && <P>,</P>}
                      </JLine>
                    ))}
                    <JLine indent={indent + 3}>
                      <P>]</P>
                    </JLine>
                  </>
                )}
                <JLine indent={indent + 2}>
                  <P>{"}"}</P>
                </JLine>
                <JLine indent={indent + 1}>
                  <P>{last ? "}" : "},"}</P>
                </JLine>
              </>
            )}
            {gi === 0 && chain.length > 1 && (
              <JLine indent={indent + 1}>
                <span className={`${J_NULL} italic`}>// inherited from parent group below</span>
              </JLine>
            )}
          </div>
        );
      })}
      <JLine indent={indent}>
        <P>],</P>
      </JLine>
    </>
  );
}
