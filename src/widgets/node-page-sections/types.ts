import type { ScryModel } from "../../entities/model/viewmodel";
import type { Editor } from "../../entities/model/editor";
import type { ClaimProbeStatus, ClaimTestStatus, ModelHealthReport } from "../../entities/model/health";
import type { PreviewServerState } from "../../features/preview-client/usePreviewServer";

export type SpecialPage = "dark" | "session";

export type Selected =
  | { kind: "node"; id: string }
  | { kind: "group"; id: string }
  // Wiki special pages — Since you last looked (`session`) and Dark code
  // (App routes these).
  | { kind: "special"; id: SpecialPage };

export interface PageProps {
  model: ScryModel;
  /** The derived tree's committed layer — the diff base: a claim in `model`
   *  and not here is a todo stub. Null only before it loads. */
  committed: ScryModel | null;
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
  onSelectGroup: (id: string) => void;
  /** B5 repair path: agent writes realistic fixture props after a failed
   *  deterministic render. */
  onFixture?: (nodeId: string, renderStatus: string, renderError: string | null) => void;
  /** The project's preview sidecar — whether a node gets a Preview section is
   *  derived from the mountable exports it reports, never from the model. */
  preview: PreviewServerState;
}
