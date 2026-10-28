/**
 * One quiet line under the top bar for each thing about the open project
 * that keeps scryer from working fully: agent integration out of date or
 * missing parts, or an authored model from before claims lived in tests. Each
 * offers the one action that clears it and stays until it is cleared.
 */

import { useState } from "react";
import {
  integrationGap,
  PART_LABEL,
  useAgentIntegration,
  type IntegrationGap,
} from "../../features/agent-integration/useAgentIntegration";
import { useAuthoredModel, type MigrationReport } from "../../features/model-migration/useAuthoredModel";
import { BTN, BTN_DANGER, BTN_GO } from "../../shared/ui/tokens";

export interface ProjectNoticesViewProps {
  integration: { gap: IntegrationGap | null; busy: boolean; error: string | null; onInstall: () => void };
  authored: {
    present: boolean;
    /** Open items in the migration checklist; null when there is none. */
    openItems: number | null;
    report: MigrationReport | null;
    busy: boolean;
    error: string | null;
    onMigrate: () => void;
    onRemove: () => void;
  };
}

const ROW = "flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-[var(--border)] bg-amber-500/10 px-3 py-1.5 text-xs text-[var(--text-secondary)]";

/** "120 test descriptions · 87 todo tests · …", the writes only. */
function wrote(r: MigrationReport): string {
  const rows: [number, string, string][] = [
    [r.described.length, "test description", "test descriptions"],
    [r.documented.length, "module doc", "module docs"],
    [r.stubbed.length, "todo test", "todo tests"],
    [r.structure.length, "structure.toml entry", "structure.toml entries"],
  ];
  return rows
    .filter(([n]) => n)
    .map(([n, one, many]) => `${n} ${n === 1 ? one : many}`)
    .join(" · ");
}

/** "Agent integration here is out of date (skill) and missing parts (hooks)." */
function gapLine(gap: IntegrationGap): string {
  const names = (parts: IntegrationGap["stale"]) => parts.map((p) => PART_LABEL[p]).join(", ");
  const what = [
    gap.stale.length ? `out of date (${names(gap.stale)})` : "",
    gap.missing.length ? `missing parts (${names(gap.missing)})` : "",
  ]
    .filter(Boolean)
    .join(" and ");
  return `Agent integration here is ${what || "not on"}.`;
}

function RemoveOldModel({ busy, onRemove }: { busy: boolean; onRemove: () => void }) {
  const [asking, setAsking] = useState(false);
  if (!asking) {
    return (
      <button type="button" className={BTN} disabled={busy} onClick={() => setAsking(true)}>
        Remove old model
      </button>
    );
  }
  return (
    <span className="flex items-center gap-1.5">
      Delete model.scry, planned.scry and history?
      <button type="button" className={BTN_DANGER} disabled={busy} onClick={onRemove}>
        Delete
      </button>
      <button type="button" className={BTN} onClick={() => setAsking(false)}>
        Cancel
      </button>
    </span>
  );
}

export function ProjectNoticesView({ integration, authored }: ProjectNoticesViewProps) {
  const r = authored.report;
  return (
    <>
      {integration.gap && (
        <div role="status" className={ROW}>
          <span>{gapLine(integration.gap)}</span>
          <button type="button" className={BTN_GO} disabled={integration.busy} onClick={integration.onInstall}>
            {integration.gap.stale.length ? "Update" : "Install"}
          </button>
          {integration.error && <span className="text-red-600 dark:text-red-400">{integration.error}</span>}
        </div>
      )}
      {(authored.present || (authored.openItems ?? 0) > 0) && (
        <div role="status" className={ROW}>
          {r || authored.openItems !== null ? (
            <span>
              {r && (wrote(r) ? `Migrated: ${wrote(r)}. ` : "Nothing to migrate: the code already says it all. ")}
              {authored.openItems
                ? `${authored.openItems} item${authored.openItems === 1 ? "" : "s"} left to review in .scryer/migration.md: ask an agent session to finish the migration.`
                : authored.openItems === 0 && "The review is done; ask the agent to remove the old model."}
            </span>
          ) : (
            <>
              <span>
                This project still has an authored model from before claims lived in tests. Migrating writes its
                claims into your tests by rule and leaves a checklist for an agent to review with you. Commit your
                work first.
              </span>
              <button type="button" className={BTN_GO} disabled={authored.busy} onClick={authored.onMigrate}>
                {authored.busy ? "Migrating…" : "Migrate"}
              </button>
            </>
          )}
          {/* Once a migration has started, the agent finishing its review
              removes the old model as its last step. */}
          {authored.present && !r && authored.openItems === null && (
            <RemoveOldModel busy={authored.busy} onRemove={authored.onRemove} />
          )}
          {authored.error && <span className="text-red-600 dark:text-red-400">{authored.error}</span>}
        </div>
      )}
    </>
  );
}

export function ProjectNotices({ projectPath }: { projectPath: string | null }) {
  const integration = useAgentIntegration(projectPath);
  const authored = useAuthoredModel(projectPath);
  return (
    <ProjectNoticesView
      integration={{
        gap: integration.status ? integrationGap(integration.status) : null,
        busy: integration.busy,
        error: integration.error,
        onInstall: () => void integration.setOn(true),
      }}
      authored={{ ...authored, onMigrate: () => void authored.migrate(), onRemove: () => void authored.remove() }}
    />
  );
}
