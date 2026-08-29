import type { ScryModel, DriftScope } from "../viewmodel";
import type { Editor } from "../editor";
import type { ClaimTestStatus, ModelHealthReport } from "../health";
import type { ChangeRevision } from "../hooks/useModelStorage";
import type { HistoryEvent } from "../history";
import type { PreviewServerState } from "../hooks/usePreviewServer";

export type SpecialPage = "changes" | "review" | "dark" | "unmapped";

export type Selected =
  | { kind: "node"; id: string }
  | { kind: "group"; id: string }
  // Wiki special pages — Recent changes, Needs review, Dark code, Unmapped
  // claims (App routes these).
  | { kind: "special"; id: SpecialPage };

export interface PageProps {
  model: ScryModel;
  /** The committed model (`model.scry`) — the diff base. The Overview renders
   *  each claim as a diff of `model` (working/planned) against this. Null only
   *  in the brief window before the committed model loads. */
  committed: ScryModel | null;
  selected: Selected;
  report: ModelHealthReport | null;
  /** respId → recorded test verdict (re-verified staleness), from the
   *  `get_test_statuses` feed. Empty until a report has been ingested. */
  testVerdicts: Record<string, ClaimTestStatus>;
  projectPath: string | null;
  editor: Editor | undefined;
  onSelectNode: (id: string) => void;
  onSelectGroup: (id: string) => void;
  /** B5 repair path: agent writes realistic fixture props after a failed
   *  deterministic render. */
  onFixture?: (nodeId: string, renderStatus: string, renderError: string | null) => void;
  /** The project's preview sidecar — whether a node gets a Preview section is
   *  derived from the mountable exports it reports, never from the model. */
  preview: PreviewServerState;
  /** Session-local journal of every edit (yours and the agent's), newest
   *  first — filtered per node to drive the History tab. */
  changeLog: readonly ChangeRevision[];
  /** Durable committed-model timeline (`.scryer/history.jsonl`), oldest first —
   *  filtered per node to drive the History tab. */
  history: readonly HistoryEvent[];
  /** Boundary-owning nodes whose code changed since the last reconcile —
   *  surfaced as a drift banner on the owning node's page. */
  driftScopes: DriftScope[];
  onCheckDrift?: () => void;
  /** Reconcile drift for a node and its subtree (scoped Dismiss). */
  onDismissDrift?: (nodeId: string) => void;
}
