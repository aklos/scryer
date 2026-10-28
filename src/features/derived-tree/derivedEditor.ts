/**
 * The editor for the derived tree. The tree is read from the code, so the
 * only edits are the ones that land in the code or in `structure.toml`:
 * rewording a claim edits its test's description, a new claim becomes a
 * todo stub in the node's test file, a description lands where it is read
 * from (a module doc's first paragraph, a manifest, `ARCHITECTURE.md`,
 * `structure.toml`), decisions are the module doc after it, and renaming
 * any node above code level sets its readable name in `structure.toml`.
 * Everything structural (create, move, delete) is the code's job, so those
 * intents do nothing here.
 */

import { invoke } from "@tauri-apps/api/core";
import type { Editor } from "../../entities/model/editor";
import {
  updateNode,
  updateResponsibility,
  type Node,
  type ScryModel,
} from "../../entities/model/viewmodel";
import { stripMarkup } from "../../entities/model/markup";

const noop = () => {};
const none = () => "";
const same = (m: ScryModel) => m;

export function makeDerivedEditor({
  projectPath,
  model,
  onWrite,
  onError,
}: {
  projectPath: string;
  model: ScryModel;
  /** Files are being written: show `apply` over the tree until they land,
   *  then re-read the tree and its status. */
  onWrite: (write: Promise<unknown>, apply: (m: ScryModel) => ScryModel) => void;
  onError: (message: string) => void;
}): Editor {
  const run = (cmd: string, args: Record<string, unknown>) =>
    invoke(cmd, { cwd: projectPath, ...args }).catch((e) => {
      onError(String(e));
    });

  const editClaim = (respId: string, statement: string) =>
    run("edit_claim_text", { respId, text: stripMarkup(statement).trim() });

  return {
    updateNode: (nodeId, patch) => {
      const node = model.nodes.find((n) => n.id === nodeId);
      if (!node) return;
      const writes: Promise<unknown>[] = [];
      // What the page shows until the writes are re-read: the written fields.
      const shown: Partial<Node> = {};
      if (patch.name !== undefined && patch.name !== node.name) {
        if (node.kind !== "symbol") {
          writes.push(run("rename_node", { nodeId, name: patch.name }));
          shown.name = patch.name;
        } else onError("A symbol is named by its code; rename it there.");
      }
      if (patch.description !== undefined && patch.description !== node.description) {
        if (node.kind !== "symbol") {
          writes.push(run("edit_node_description", { nodeId, text: patch.description }));
          shown.description = patch.description;
        } else onError("A symbol's description is its doc comment; edit it in the code.");
      }
      if (patch.decisions !== undefined && patch.decisions !== (node.decisions ?? "")) {
        if (node.kind !== "symbol" && node.kind !== "person" && !node.external) {
          writes.push(run("edit_node_decisions", { nodeId, text: patch.decisions }));
          shown.decisions = patch.decisions;
        } else onError("Decisions live in a module doc; this node has none.");
      }
      if (patch.responsibilities) {
        const before = new Map((node.responsibilities ?? []).map((r) => [r.id, r]));
        for (const r of patch.responsibilities) {
          const text = stripMarkup(r.statement).trim();
          const old = before.get(r.id);
          if (!old) writes.push(run("add_claim_stub", { nodeId, text }));
          else if (text !== stripMarkup(old.statement).trim()) writes.push(editClaim(r.id, r.statement));
        }
        const kept = new Set(patch.responsibilities.map((r) => r.id));
        if ((node.responsibilities ?? []).some((r) => !kept.has(r.id)))
          onError("A claim is a test: delete the test in the code to remove it.");
      }
      if (writes.length > 0)
        onWrite(Promise.all(writes), Object.keys(shown).length > 0 ? (m) => updateNode(m, nodeId, shown) : same);
    },
    updateResponsibility: (host, hostId, respId, patch) => {
      const { statement } = patch;
      if (statement !== undefined)
        onWrite(editClaim(respId, statement), (m) => updateResponsibility(m, host, hostId, respId, { statement }));
    },
    deleteNode: noop,
    addNode: none,
    moveNode: noop,
    addLink: none,
    updateLink: noop,
    deleteLink: noop,
    updateGroup: noop,
    deleteGroup: noop,
    addGroup: none,
    setNodeGroup: noop,
    addResponsibility: none,
    removeResponsibility: noop,
    renameConcern: noop,
    adoptResponsibility: noop,
    rejectResponsibility: noop,
    dropResponsibility: noop,
    reimplementResponsibility: noop,
    rewordResponsibility: noop,
    dropNode: noop,
    reimplementNode: noop,
    moveResponsibility: noop,
    addProperty: noop,
    updateProperty: noop,
    removeProperty: noop,
    adoptProperty: noop,
    rejectProperty: noop,
    dropProperty: noop,
    reimplementProperty: noop,
  };
}
