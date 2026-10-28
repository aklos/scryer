/** Claims for `ProjectNotices`. */
import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ProjectNoticesView, type ProjectNoticesViewProps } from "../src/widgets/project-notices/ProjectNotices";
import type { IntegrationGap } from "../src/features/agent-integration/useAgentIntegration";
import type { MigrationReport } from "../src/features/model-migration/useAuthoredModel";

const noop = () => {};
const view = (over: {
  gap?: IntegrationGap | null;
  present?: boolean;
  openItems?: number | null;
  report?: MigrationReport | null;
  busy?: boolean;
}): string => {
  const props: ProjectNoticesViewProps = {
    integration: { gap: over.gap ?? null, busy: false, error: null, onInstall: noop },
    authored: {
      present: over.present ?? false,
      openItems: over.openItems ?? null,
      report: over.report ?? null,
      busy: over.busy ?? false,
      error: null,
      onMigrate: noop,
      onRemove: noop,
    },
  };
  return renderToStaticMarkup(<ProjectNoticesView {...props} />);
};

describe("ProjectNotices", () => {
  it("When the open project's agent integration is out of date, names the parts and offers to update them", () => {
    const html = view({ gap: { stale: ["skill"], missing: [] } });
    expect(html).toContain("Agent integration here is out of date (skill).");
    expect(html).toContain(">Update<");
  });

  it("When the open project's agent integration is missing parts, names them and offers to install them", () => {
    const html = view({ gap: { stale: [], missing: ["hooks", "skill"] } });
    expect(html).toContain("Agent integration here is missing parts (hooks, skill).");
    expect(html).toContain(">Install<");
    expect(view({ gap: { stale: ["skill"], missing: ["hooks"] } })).toContain("out of date (skill) and missing parts (hooks).");
  });

  it("While the open project holds an authored model, shows a notice that migrates it into the code", () => {
    const html = view({ present: true });
    expect(html).toContain("authored model");
    expect(html).toContain(">Migrate<");
    expect(html).toContain("Commit your work first");
  });

  it("While a migration runs, shows it under way on the disabled Migrate button", () => {
    const html = view({ present: true, busy: true });
    expect(html).toMatch(/<button[^>]*disabled=""[^>]*>Migrating…<\/button>/);
  });

  it("When a migration finishes, shows what it wrote and how much of its review is left", () => {
    const html = view({
      present: true,
      openItems: 4,
      report: { described: ["a", "b"], documented: [], stubbed: ["c"], structure: [], skipped: ["d"], checklist: ".scryer/migration.md" },
    });
    expect(html).toContain("Migrated: 2 test descriptions · 1 todo test.");
    expect(html).toContain("4 items left to review in .scryer/migration.md");
    expect(html).not.toContain(">Migrate<");
  });

  it("While a migration has started, offers no removal of the old model, which the agent finishing the review removes", () => {
    expect(view({ present: true })).toContain("Remove old model");
    expect(view({ present: true, openItems: 4 })).not.toContain("Remove old model");
    expect(view({ present: true, openItems: 0 })).toContain("ask the agent to remove the old model");
    expect(view({ present: true, report: { described: [], documented: [], stubbed: [], structure: [], skipped: [], checklist: null } })).not.toContain(
      "Remove old model",
    );
  });

  it("While a migration checklist has open items, shows how many are left and that an agent session finishes it", () => {
    const html = view({ openItems: 1 });
    expect(html).toContain("1 item left to review in .scryer/migration.md: ask an agent session to finish the migration.");
    expect(html).not.toContain("Remove old model");
    expect(view({ openItems: 0 })).toBe("");
  });

  it("Shows nothing for a project fully set up and with no authored model", () => {
    expect(view({})).toBe("");
  });
});
