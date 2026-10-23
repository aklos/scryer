/**
 * The project's one agent-integration switch, with `scryer-mcp setup`'s
 * readout under it so the files it writes are always in view. Settings and
 * the new-project screen both mount it.
 */

import { useAgentIntegration, type SetupEntry, type SetupStatus } from "./useAgentIntegration";

const PART_LABEL: Record<SetupEntry["part"], string> = {
  mcpServer: "mcp server",
  autoApprove: "auto-approve",
  hooks: "hooks",
  statusline: "status line",
  skill: "skill",
  gitignore: "gitignore",
};

const STATE_CLASS: Record<SetupEntry["state"], string> = {
  installed: "text-emerald-500",
  missing: "text-[var(--text-ghost)]",
  stale: "text-orange-600 dark:text-orange-400",
  foreign: "text-[var(--text-muted)]",
};

const STATE_LABEL: Record<SetupEntry["state"], string> = {
  installed: "installed",
  missing: "missing",
  stale: "stale",
  foreign: "not scryer's",
};

/** The tools worth a block: detected ones, and an undetected one only for
 *  what it alone has installed — a `.mcp.json` it shares with a detected
 *  tool says nothing about it. Mirrors the CLI readout. */
function shownTools(status: SetupStatus) {
  const detectedFiles = new Set(
    status.tools.filter((t) => t.detected).flatMap((t) => t.entries.map((e) => e.file)),
  );
  return status.tools.filter(
    (t) =>
      t.detected ||
      t.entries.some((e) => e.state !== "missing" && !detectedFiles.has(e.file)),
  );
}

export function AgentIntegrationSwitch({
  projectPath,
  onChange,
}: {
  projectPath: string;
  /** After the switch flips — lets the host refresh state that depends on it. */
  onChange?: () => void;
}) {
  const { status, busy, error, setOn } = useAgentIntegration(projectPath);
  const on = status?.on ?? false;

  const flip = async () => {
    await setOn(!on);
    onChange?.();
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <span className="text-xs text-[var(--text)]">Agent integration</span>
        <button
          type="button"
          role="switch"
          aria-checked={on}
          aria-label="Agent integration"
          disabled={busy || !status}
          onClick={() => void flip()}
          className={`relative inline-flex h-4 w-7 shrink-0 items-center rounded-full transition-colors disabled:opacity-50 ${on ? "bg-emerald-500" : "bg-[var(--border-strong)]"}`}
        >
          <span
            className={`inline-block h-3 w-3 rounded-full bg-white transition-transform ${on ? "translate-x-3.5" : "translate-x-0.5"}`}
          />
        </button>
      </div>
      {status && (
        <div className="flex flex-col gap-1.5">
          {shownTools(status).map((t) => (
            <div key={t.tool} className="flex flex-col gap-0.5">
              <span className="text-xs text-[var(--text-secondary)]">
                {t.name}
                {!t.detected && <span className="text-[var(--text-ghost)]"> (not on PATH)</span>}
              </span>
              {t.entries.map((e) => (
                <EntryRow key={`${e.file}:${e.part}`} entry={e} />
              ))}
            </div>
          ))}
          <EntryRow entry={status.gitignore} />
          {status.notes.map((n) => (
            <p key={n} className="text-xs leading-relaxed text-[var(--text-muted)]">
              {n}
            </p>
          ))}
        </div>
      )}
      {error && <p className="text-xs leading-relaxed text-red-500">{error}</p>}
    </div>
  );
}

function EntryRow({ entry }: { entry: SetupEntry }) {
  return (
    <div className="flex items-center justify-between gap-2 pl-2 text-xs">
      <span className="font-mono text-[var(--text-muted)]">{entry.file}</span>
      <span className="flex gap-2">
        <span className="text-[var(--text-tertiary)]">{PART_LABEL[entry.part]}</span>
        <span className={STATE_CLASS[entry.state]} title={entry.note}>
          {STATE_LABEL[entry.state]}
        </span>
      </span>
    </div>
  );
}
