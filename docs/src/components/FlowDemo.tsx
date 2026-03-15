/**
 * Wraps the actual FlowScriptView from the app in read-only mode for docs.
 */
import { FlowScriptView } from "@app/FlowScriptView";
import type { Flow } from "@app/types";

const noop = () => {};

export function FlowDemo({ flow }: { flow: Flow }) {
  return (
    <div className="rounded-xl border border-zinc-200 dark:border-zinc-700 overflow-hidden my-4 pointer-events-none">
      <FlowScriptView
        flow={flow}
        onUpdate={noop}
        onDelete={noop}
        allNodes={[]}
        sourceMap={{}}
      />
    </div>
  );
}
