/**
 * A glossary word in running text: a faint dotted underline that, on hover,
 * says what the word means and links to the docs section that explains it.
 * `detail` comes first when the thing itself carries a description, such as
 * a style's or a layer's own.
 */

import type { ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { docsUrl, GLOSSARY, type TermId } from "./glossary";

export function Term({
  id,
  children,
  detail,
  above = false,
  className = "",
}: {
  id: TermId;
  /** What to show in place of the term's own label. */
  children?: ReactNode;
  detail?: string;
  /** Open the card upward, for a term near the window's bottom edge. */
  above?: boolean;
  className?: string;
}) {
  const url = docsUrl(id);
  return (
    <span className={`group/term relative inline-block ${className}`}>
      <span className="cursor-help underline decoration-[var(--text-ghost)] decoration-dotted underline-offset-[3px]">
        {children ?? GLOSSARY[id].label}
      </span>
      {/* The padding bridges the gap to the card so the pointer can reach
          its link without the card closing. */}
      <span
        role="tooltip"
        className={`pointer-events-none invisible absolute left-0 z-50 w-72 opacity-0 transition-opacity delay-300 group-hover/term:pointer-events-auto group-hover/term:visible group-hover/term:opacity-100 ${
          above ? "bottom-full pb-1.5" : "top-full pt-1.5"
        }`}
      >
        <span className="flex flex-col gap-1 whitespace-normal rounded border border-[var(--border-strong)] bg-[var(--surface-raised)] px-2.5 py-2 text-left font-sans text-xs font-normal leading-snug text-[var(--text-secondary)] shadow-lg">
          {detail && <span className="text-[var(--text)]">{detail}</span>}
          <span>{GLOSSARY[id].definition}</span>
          {url && (
            <span
              role="link"
              tabIndex={0}
              onClick={(e) => {
                // A term may sit inside a button; the link must not press it.
                e.stopPropagation();
                void openUrl(url);
              }}
              className="cursor-pointer self-start text-[var(--accent)] hover:underline"
            >
              Read more
            </span>
          )}
        </span>
      </span>
    </span>
  );
}
