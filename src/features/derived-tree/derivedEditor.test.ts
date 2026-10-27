import { describe, expect, it, vi } from "vitest";

const invoke = vi.fn((_cmd: string, _args?: Record<string, unknown>) => Promise.resolve());
vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args?: Record<string, unknown>) => invoke(cmd, args) }));

import { makeDerivedEditor } from "./derivedEditor";
import type { Node, ScryModel } from "../../entities/model/viewmodel";

const node = (id: string, kind: Node["kind"]): Node => ({ id, kind, name: id }) as Node;

describe("derivedEditor", () => {
  it("When any node above code level is renamed, writes its readable name to structure.toml", () => {
    const model = {
      nodes: [node("sys", "system"), node("dev", "person"), node("app", "container"), node("ui", "component"), node("f", "symbol")],
    } as ScryModel;
    const onError = vi.fn();
    const editor = makeDerivedEditor({ projectPath: "/p", model, onWritten: () => {}, onError });
    for (const id of ["sys", "dev", "app", "ui", "f"]) editor.updateNode(id, { name: `${id} renamed` });
    const renamed = invoke.mock.calls.filter(([cmd]) => cmd === "rename_node").map(([, args]) => args?.nodeId);
    expect(renamed).toEqual(["sys", "dev", "app", "ui"]);
    expect(onError).toHaveBeenCalledOnce();
  });
});

describe("derivedEditor decisions", () => {
  it("When decisions are edited, writes them after the description in the node's module doc", () => {
    invoke.mockClear();
    const model = { nodes: [node("app", "container"), node("dev", "person")] } as ScryModel;
    const onError = vi.fn();
    const editor = makeDerivedEditor({ projectPath: "/p", model, onWritten: () => {}, onError });
    editor.updateNode("app", { decisions: "- Chose a grid." });
    editor.updateNode("dev", { decisions: "x" });
    expect(invoke.mock.calls.filter(([cmd]) => cmd === "edit_node_decisions").map(([, a]) => a?.nodeId)).toEqual(["app"]);
    expect(onError).toHaveBeenCalledOnce();
  });
});
