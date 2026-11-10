import type { ScryModel } from "../../entities/model/viewmodel";
import type { ModelChanges } from "../../entities/model/changes";
import type { Editor } from "../../entities/model/editor";
import type { ClaimProbeStatus, ClaimTestStatus, ModelHealthReport } from "../../entities/model/health";
import type { PreviewServerState } from "../../features/preview-client/usePreviewServer";
import type { Selected } from "../../entities/model/navigation";

export interface PageProps {
  model: ScryModel;
  /** The model at the base the checkout's changes are read against: HEAD on
   *  the default branch, the merge base off it. `model` itself when there is
   *  none, so nothing reads as changed. */
  base: ScryModel;
  /** Show the page's changes in full: word diffs and removed rows. Off, a
   *  changed row only carries its mark. */
  showChanges: boolean;
  /** Flip {@link showChanges}; the page draws its toggle only when its node
   *  changed since the base. */
  onToggleChanges?: () => void;
  /** What changed in the model since the base, node by node. */
  changes?: ModelChanges;
  selected: Selected;
  report: ModelHealthReport | null;
  /** respId → recorded test verdict (re-verified staleness), from the
   *  `get_test_statuses` feed. Empty until a report has been ingested. */
  testVerdicts: Record<string, ClaimTestStatus>;
  /** respId → probe result. Absent means never probed, never "clean". */
  probeResults: Record<string, ClaimProbeStatus>;
  projectPath: string | null;
  editor: Editor | undefined;
  onSelectNode: (id: string) => void;
  /** B5 repair path: agent writes realistic fixture props after a failed
   *  deterministic render. */
  onFixture?: (nodeId: string, renderStatus: string, renderError: string | null) => void;
  /** The project's preview sidecar — whether a node gets a Preview section is
   *  derived from the mountable exports it reports, never from the model. */
  preview: PreviewServerState;
}
