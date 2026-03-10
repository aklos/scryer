import type { ReactNode } from "react";
import type { C4Kind, Status } from "./types";
import { STATUS_COLORS } from "./statusColors";
import { KIND_ICON } from "./kindIcons";

export interface MentionNodeInfo {
  kind: string;
  status?: Status;
}

/** Regex to match @[Name] inline references */
const REF_RE = /@\[([^\]]+)\]/g;

/** Parse text into segments, splitting on @[name] references */
function parseRefs(text: string, onMentionClick?: (name: string) => void, onMentionHover?: (name: string | null) => void, nodeMap?: Map<string, MentionNodeInfo>, resolveMap?: Map<string, string>): ReactNode[] {
  const parts: ReactNode[] = [];
  let last = 0;
  let match: RegExpExecArray | null;
  let key = 0;
  while ((match = REF_RE.exec(text)) !== null) {
    if (match.index > last) {
      parts.push(text.slice(last, match.index));
    }
    const rawName = match[1];
    const displayName = resolveMap?.get(rawName) ?? rawName;
    const info = nodeMap?.get(rawName);
    const sc = info?.status ? STATUS_COLORS[info.status] : null;
    const kindEntry = info?.kind ? KIND_ICON[info.kind as C4Kind] : null;
    const KindIcon = kindEntry?.Icon;
    parts.push(
      <span
        key={key++}
        className={`inline-flex items-baseline gap-0.5 rounded px-1 font-medium font-mono text-[0.85em] leading-none align-baseline ${
          sc
            ? `${sc.pillClass}${onMentionClick ? ` cursor-pointer ${sc.pillHoverClass}` : ""}`
            : `bg-zinc-100 dark:bg-zinc-800 text-zinc-600 dark:text-zinc-300${onMentionClick ? " cursor-pointer hover:bg-zinc-200 dark:hover:bg-zinc-700" : ""}`
        }`}
        onClick={onMentionClick ? (e) => { e.stopPropagation(); onMentionClick(rawName); } : undefined}
        onMouseEnter={onMentionHover ? () => onMentionHover(rawName) : undefined}
        onMouseLeave={onMentionHover ? () => onMentionHover(null) : undefined}
      >
        {KindIcon && <KindIcon size="0.9em" className={`shrink-0 relative top-[0.1em] ${kindEntry.color}`} />}
        {displayName.length > 30 ? displayName.slice(0, 30) + "…" : displayName}
      </span>,
    );
    last = match.index + match[0].length;
  }
  if (last < text.length) {
    parts.push(text.slice(last));
  }
  return parts;
}

/** Render description text, converting lines starting with - or * into bullet lists and @[name] into highlighted spans */
export function DescriptionText({ text, onMentionClick, onMentionHover, nodeMap, resolveMap }: {
  text: string;
  onMentionClick?: (name: string) => void;
  onMentionHover?: (name: string | null) => void;
  nodeMap?: Map<string, MentionNodeInfo>;
  /** Maps raw mention text (e.g. step ID) to display text (e.g. "Step 2") */
  resolveMap?: Map<string, string>;
}) {
  const lines = text.split("\n");
  const isList = lines.some((l) => /^[\-\*]\s/.test(l.trimStart()));

  if (!isList) {
    return (
      <span className="block break-words overflow-hidden">
        {parseRefs(text, onMentionClick, onMentionHover, nodeMap, resolveMap)}
      </span>
    );
  }

  return (
    <ul className="text-left pl-3 space-y-0.5 w-full break-words">
      {lines.map((line, i) => {
        const trimmed = line.trimStart();
        const isBullet = /^[\-\*]\s/.test(trimmed);
        if (!isBullet && !trimmed) return null;
        return (
          <li key={i} className={isBullet ? "list-disc" : "list-none"}>
            {parseRefs(isBullet ? trimmed.slice(2) : trimmed, onMentionClick, onMentionHover, nodeMap, resolveMap)}
          </li>
        );
      })}
    </ul>
  );
}
