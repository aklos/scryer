import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { McpSetupPrompt } from "../src/features/mcp-setup/McpSetupPrompt";
import { hooksToUpdate, type AiToolsState, type McpSetup } from "../src/features/mcp-setup/useMcpSetup";
import { HooksRow } from "../src/widgets/settings-panel/SettingsPanel";

const tools: AiToolsState = {
  claude: true,
  codex: true,
  copilot: false,
  claudeMcpEnabled: true,
  codexMcpEnabled: true,
  copilotMcpEnabled: true,
  claudeApproved: true,
  claudeHooks: "outdated",
  codexHooks: "current",
  copilotHooks: "outdated",
  claudeStatuslineEnabled: false,
  claudeStatuslineForeign: false,
};

const noop = async () => {};
const setupFor = (t: AiToolsState): McpSetup => ({
  tools: t,
  needsSetup: false,
  hooksOutdated: hooksToUpdate(t),
  dismissed: false,
  busy: false,
  enable: noop,
  enableHooks: noop,
  updateHooks: noop,
  enableStatusline: noop,
  dismiss: () => {},
  reload: () => {},
});

describe("outdated session hooks", () => {
  it("lists only detected tools whose hooks are outdated", () => {
    // Copilot's file is outdated but Copilot isn't installed: nothing to offer.
    expect(hooksToUpdate(tools)).toEqual(["claude"]);
    expect(hooksToUpdate({ ...tools, claudeHooks: "none" })).toEqual([]);
    expect(hooksToUpdate({ ...tools, copilot: true })).toEqual(["claude", "copilot"]);
  });

  it("offers an update naming the files it rewrites", () => {
    const html = renderToStaticMarkup(<McpSetupPrompt setup={setupFor(tools)} />);
    expect(html).toContain("Update session hooks");
    expect(html).toContain("Claude Code hooks in this project are from an earlier version");
    expect(html).toContain(".claude/settings.local.json");
    expect(html).not.toContain(".codex/hooks.json");
    expect(html).not.toContain("Enable AI tool integration");

    const current = renderToStaticMarkup(
      <McpSetupPrompt setup={setupFor({ ...tools, claudeHooks: "current" })} />,
    );
    expect(current).not.toContain("Update session hooks");
  });

  it("shows Update in a settings row whose hooks are outdated", () => {
    const row = (status: "none" | "outdated" | "current") =>
      renderToStaticMarkup(
        <HooksRow name="Claude Code" target=".claude/settings.local.json" status={status} busy={false} onInstall={() => {}} />,
      );
    expect(row("outdated")).toContain("Outdated");
    expect(row("outdated")).toContain(">Update<");
    expect(row("outdated")).not.toContain("Installed");
    expect(row("none")).toContain(">Install<");
    expect(row("current")).toContain("Installed");
    expect(row("current")).not.toContain("Update");
  });
});
