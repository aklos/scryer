import type { ReactNode } from "react";
import type { Status } from "./types";
import { STATUS_COLORS } from "./statusColors";

export interface MentionNodeInfo {
  kind: string;
  status?: Status;
}

/** Regex to match @[Name] inline references */
const REF_RE = /@\[([^\]]+)\]/g;

/** Parse text into segments, splitting on @[name] references */
function parseRefs(text: string, onMentionClick?: (name: string) => void, nodeMap?: Map<string, MentionNodeInfo>): ReactNode[] {
  const parts: ReactNode[] = [];
  let last = 0;
  let match: RegExpExecArray | null;
  let key = 0;
  while ((match = REF_RE.exec(text)) !== null) {
    if (match.index > last) {
      parts.push(text.slice(last, match.index));
    }
    const name = match[1];
    const info = nodeMap?.get(name);
    const sc = info?.status ? STATUS_COLORS[info.status] : null;
    parts.push(
      <span
        key={key++}
        className={`inline-flex items-baseline rounded px-1 py-px font-medium font-mono text-[0.9em] ${
          sc
            ? `${sc.pillClass}${onMentionClick ? ` cursor-pointer ${sc.pillHoverClass}` : ""}`
            : `bg-zinc-200/80 dark:bg-zinc-700 text-zinc-700 dark:text-zinc-200${onMentionClick ? " cursor-pointer hover:bg-zinc-300/80 dark:hover:bg-zinc-600" : ""}`
        }`}
        onClick={onMentionClick ? (e) => { e.stopPropagation(); onMentionClick(name); } : undefined}
      >
        {name}
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
export function DescriptionText({ text, onMentionClick, nodeMap }: { text: string; onMentionClick?: (name: string) => void; nodeMap?: Map<string, MentionNodeInfo> }) {
  const lines = text.split("\n");
  const isList = lines.some((l) => /^[\-\*]\s/.test(l.trimStart()));

  if (!isList) {
    return (
      <span className="block break-words overflow-hidden">
        {parseRefs(text, onMentionClick, nodeMap)}
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
            {parseRefs(isBullet ? trimmed.slice(2) : trimmed, onMentionClick, nodeMap)}
          </li>
        );
      })}
    </ul>
  );
}
