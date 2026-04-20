import type { C4Node, Contract, ContractItem } from "../types";
import { contractPassed } from "../types";
import { useUpdateNodeData } from "./utils";
import { JsonRoot, JLine, P } from "./json";
import { ContractJson } from "./ContractSection";

const EMPTY: Contract = { expect: [], ask: [], never: [] };

/** Standalone container that renders just the node's contract as its own JSON document.
 *  Used by ContextPanel as the middle of three stacked sections. */
export function ContractContainer({ node }: { node: C4Node }) {
  const updateNodeData = useUpdateNodeData();
  const contract = node.data.contract ?? EMPTY;

  return (
    <JsonRoot>
      <JLine indent={0}><P>{"{"}</P></JLine>
      <ContractJson
        contract={contract}
        indent={1}
        onChange={(next) => updateNodeData(node.id, { contract: next })}
      />
      <JLine indent={0}><P>{"}"}</P></JLine>
    </JsonRoot>
  );
}

/** Status counts for the section header badge. */
export interface ContractStatus {
  passed: number;
  failed: number;
  unchecked: number;
  expectTotal: number;
  askTotal: number;
  neverTotal: number;
}

export function getContractStatus(contract: Contract | undefined): ContractStatus {
  const c = contract ?? EMPTY;
  let passed = 0, failed = 0, unchecked = 0;
  for (const item of c.expect) {
    const p = contractPassed(item);
    if (p === true) passed++;
    else if (p === false) failed++;
    else unchecked++;
  }
  return {
    passed,
    failed,
    unchecked,
    expectTotal: c.expect.length,
    askTotal: c.ask.length,
    neverTotal: c.never.length,
  };
}

/** A status badge for the contract header — `3✓ 1✗ 2?`. */
export function ContractStatusBadge({ contract }: { contract?: Contract }) {
  const s = getContractStatus(contract);
  if (s.expectTotal === 0 && s.askTotal === 0 && s.neverTotal === 0) {
    return <span className="text-[var(--text-ghost)] font-mono text-[10px]">empty</span>;
  }
  const allPassed = s.expectTotal > 0 && s.passed === s.expectTotal && s.failed === 0;
  return (
    <span className="font-mono text-[10px] flex items-center gap-1.5">
      {s.passed > 0 && <span className="text-emerald-400">{s.passed}✓</span>}
      {s.failed > 0 && <span className="text-red-400">{s.failed}✗</span>}
      {s.unchecked > 0 && <span className="text-[var(--text-ghost)]">{s.unchecked}?</span>}
      {s.askTotal > 0 && <span className="text-amber-400">{s.askTotal} ask</span>}
      {s.neverTotal > 0 && <span className="text-zinc-400">{s.neverTotal} never</span>}
      {allPassed && <span className="text-emerald-400">·  all met</span>}
    </span>
  );
}

// Re-export for convenience so ContextPanel doesn't need a second import path.
export type { ContractItem };
