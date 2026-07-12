/**
 * Shared building blocks for the node/group pages and the tree, following
 * Wikipedia's pattern language:
 *
 *  - PageSection — underlined section heading with a per-section [edit] link.
 *  - WikiLink — inline cross-reference. A plain blue link to a real page.
 */

export {
  PAGE_COL,
  DESCRIPTION_MAX,
  TECHNOLOGY_MAX,
  NAME_MAX,
  WordDiffText,
  sanitizeIdentifier,
  EmptyFlag,
  EmptyDot,
  EYEBROW_BASE,
  EYEBROW,
  BTN,
  BTN_GO,
  BTN_DANGER,
  BTN_AGENT,
  BTN_ICON,
  LINK,
  AgentMark,
  CTL,
} from "./kit/tokens";
export { Editable, EditLink } from "./kit/Editable";
export { PageSection, Empty, SegField, SectionEditor } from "./kit/sections";
export { WikiLink, jumpTo } from "./kit/WikiLink";
