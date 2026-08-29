/**
 * The Preview-section gate (`previewEntryFor`): whether a node page shows a
 * live preview is derived from the exports the preview sidecar reports — a
 * node carries no "visual" flag. Symbol + a mountable export → preview;
 * anything else → no section.
 */
import { describe, expect, it } from "vitest";
import {
  previewEntryFor,
  previewableNodeIds,
  type PreviewComponentInfo,
} from "../src/hooks/usePreviewServer";

const button: PreviewComponentInfo = {
  file: "src/ui/Button.tsx",
  exportName: "Button",
  displayName: "Button",
  warnings: [],
};
const otherButton: PreviewComponentInfo = { ...button, file: "src/legacy/Button.tsx" };
const components = [button];

describe("previewEntryFor", () => {
  it("shows a preview when the sidecar reports a mountable export for the node", () => {
    expect(previewEntryFor({ kind: "symbol", name: "Button" }, components, "src/ui/Button.tsx")).toBe(button);
  });

  it("matches by name alone when the node's anchor is elsewhere or missing", () => {
    expect(previewEntryFor({ kind: "symbol", name: "Button" }, components, undefined)).toBe(button);
    expect(previewEntryFor({ kind: "symbol", name: "Button" }, components, "src/other.tsx")).toBe(button);
  });

  it("breaks a tie between same-named exports by the node's anchored file", () => {
    const both = [otherButton, button];
    expect(previewEntryFor({ kind: "symbol", name: "Button" }, both, "src/ui/Button.tsx")).toBe(button);
    expect(previewEntryFor({ kind: "symbol", name: "Button" }, both, "src/legacy/Button.tsx")).toBe(otherButton);
  });

  it("never previews a same-file symbol under another name (the helper beside the component)", () => {
    expect(previewEntryFor({ kind: "symbol", name: "resolveLaunch" }, components, "src/ui/Button.tsx")).toBeNull();
  });

  it("omits the section when the sidecar reports nothing for the node", () => {
    expect(previewEntryFor({ kind: "symbol", name: "parseConfig" }, components, "src/config.ts")).toBeNull();
    expect(previewEntryFor({ kind: "symbol", name: "Button" }, [], "src/ui/Button.tsx")).toBeNull();
  });

  it("omits the section while the component list is still loading", () => {
    expect(previewEntryFor({ kind: "symbol", name: "Button" }, null, "src/ui/Button.tsx")).toBeNull();
  });

  it("never previews structural nodes, even when a file-level match exists", () => {
    expect(previewEntryFor({ kind: "component", name: "Button" }, components, "src/ui/Button.tsx")).toBeNull();
  });
});

describe("previewableNodeIds", () => {
  const nodes = [
    { id: "n-btn", kind: "symbol", name: "Button" },
    { id: "n-helper", kind: "symbol", name: "resolveLaunch" },
    { id: "n-comp", kind: "component", name: "Button" },
  ];
  const anchorOf = () => "src/ui/Button.tsx";

  it("marks exactly the symbols the sidecar can render", () => {
    expect([...previewableNodeIds(nodes, components, anchorOf)]).toEqual(["n-btn"]);
  });

  it("marks nothing until the component list has loaded", () => {
    expect(previewableNodeIds(nodes, null, anchorOf).size).toBe(0);
  });
});
