import { describe, expect, it } from "vitest";
import { integrationGap, type EntryState, type SetupEntry, type SetupStatus } from "./useAgentIntegration";

const status = (on: boolean, entries: [SetupEntry["part"], EntryState][]): SetupStatus => ({
  on,
  tools: [
    {
      tool: "claudeCode",
      name: "Claude Code",
      detected: true,
      entries: entries.map(([part, state]) => ({ part, file: ".claude/settings.local.json", state })),
    },
  ],
  gitignore: { part: "gitignore", file: ".scryer/.gitignore", state: "missing" },
  notes: [],
});

describe("integrationGap", () => {
  it("When some of scryer's entries are in place but integration is not on, names the parts out of date and the parts missing", () => {
    expect(integrationGap(status(false, [["mcpServer", "installed"], ["skill", "stale"], ["hooks", "missing"]]))).toEqual({
      stale: ["skill"],
      missing: ["gitignore", "hooks"],
    });
    expect(integrationGap(status(false, [["hooks", "missing"], ["skill", "foreign"]]))).toBeNull();
    expect(integrationGap(status(true, [["hooks", "installed"]]))).toBeNull();
  });
});
