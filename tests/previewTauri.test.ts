/**
 * Previews of a Tauri frontend: its components reach the host through globals
 * the webview injects, so the preview shell stands them in before mounting.
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { TAURI_STUB, usesTauri } from "../preview/plugin.mjs";

function pkg(deps: Record<string, string>): string {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "scryer-preview-"));
  fs.writeFileSync(path.join(dir, "package.json"), JSON.stringify({ dependencies: deps }));
  return dir;
}

describe("Tauri preview stub", () => {
  it("When the package depends on @tauri-apps/api, the preview stands in the Tauri globals", () => {
    expect(usesTauri(pkg({ "@tauri-apps/api": "^2" }))).toBe(true);
    expect(usesTauri(pkg({ react: "^19" }))).toBe(false);
  });

  it("When a component calls the host in the preview, the call stays pending instead of throwing", async () => {
    const win: Record<string, any> = {};
    const script = TAURI_STUB.replace(/<\/?script>/g, "").replaceAll("window.", "win.");
    new Function("win", script)(win);
    expect(win.__TAURI_INTERNALS__.metadata.currentWindow.label).toBe("main");
    const settled = await Promise.race([
      win.__TAURI_INTERNALS__.invoke("get_model", {}).then(() => "settled"),
      new Promise((r) => setTimeout(() => r("pending"), 20)),
    ]);
    expect(settled).toBe("pending");
  });
});
