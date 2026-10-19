/**
 * The editor for the derived tree. The tree is read from the code, so the
 * only edits are the ones that land in the code or in `structure.toml`:
 * rewording a claim edits its test's description, a new claim becomes a
 * todo stub in the node's test file, and renaming a container sets its
 * display name. Everything structural (create, move, delete) is the code's
 * job, so those intents do nothing here.
 */

import { invoke } from "@tauri-apps/api/core";
import type { Editor } from "../../entities/model/editor";
import type { ScryModel } from "../../entities/model/viewmodel";
import { stripMarkup } from "../../entities/model/markup";

const noop = () => {};
const none = () => "";

export function makeDerivedEditor({
  projectPath,
  model,
  onWritten,
  onError,
}: {
  projectPath: string;
  model: ScryModel;
  /** A file was written: re-read the tree and its status. */
  onWritten: () => void;
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
      if (patch.name !== undefined && patch.name !== node.name) {
        if (node.kind === "container") writes.push(run("rename_container", { nodeId, name: patch.name }));
        else onError("A component is named by its module; rename it in the code.");
      }
      if (patch.description !== undefined && patch.description !== node.description)
        onError("A description is the module's doc comment; edit it in the code.");
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
      if (writes.length > 0) void Promise.all(writes).then(onWritten);
    },
    updateResponsibility: (_host, _hostId, respId, patch) => {
      if (patch.statement !== undefined) void editClaim(respId, patch.statement).then(onWritten);
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
