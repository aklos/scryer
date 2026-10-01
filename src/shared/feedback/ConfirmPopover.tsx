/**
 * Tiny inline confirm popover anchored to a button. Used for delete actions on
 * cards and groups. Click outside or Esc cancels; Enter confirms.
 */

import { useEffect, useRef } from "react";
import { createPortal } from "react-dom";

export interface ConfirmPopoverProps {
  anchorRect: DOMRect;
  label?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmPopover({
  anchorRect,
  label = "Delete?",
  confirmLabel = "Delete",
  cancelLabel = "Cancel",
  onConfirm,
  onCancel,
}: ConfirmPopoverProps) {
  const ref = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onCancel();
      } else if (e.key === "Enter") {
        e.preventDefault();
        onConfirm();
      }
    };
    const onDown = (e: PointerEvent) => {
      const el = ref.current;
      if (el && !el.contains(e.target as Node)) onCancel();
    };
    window.addEventListener("keydown", onKey);
    // Listen on next tick so the click that opened the popover doesn't close it.
    const timeout = setTimeout(
      () => window.addEventListener("pointerdown", onDown, true),
      0,
    );
    return () => {
      clearTimeout(timeout);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pointerdown", onDown, true);
    };
  }, [onCancel, onConfirm]);

  // Anchor below the button; clamp to viewport.
  const padding = 8;
  const top = anchorRect.bottom + 4;
  const left = Math.min(
    Math.max(padding, anchorRect.left),
    window.innerWidth - 220 - padding,
  );

  return createPortal(
    <div
      ref={ref}
      role="dialog"
      data-no-pickup
      onPointerDown={(e) => e.stopPropagation()}
      onMouseDown={(e) => e.stopPropagation()}
      onWheel={(e) => e.stopPropagation()}
      style={{
        position: "fixed",
        top,
        left,
        zIndex: 1100,
      }}
      className="rounded border border-[var(--border-overlay)] bg-[var(--surface-overlay)] backdrop-blur-md shadow-lg px-2.5 py-1.5 flex items-center gap-2 text-xs"
    >
      <span className="text-[var(--text-secondary)]">{label}</span>
      <button
        type="button"
        onClick={onCancel}
        className="px-2 py-0.5 rounded text-[var(--text-tertiary)] hover:bg-[var(--surface-hover)] hover:text-[var(--text-secondary)]"
      >
        {cancelLabel}
      </button>
      <button
        type="button"
        onClick={onConfirm}
        autoFocus
        className="px-2 py-0.5 rounded text-red-400 hover:bg-red-500/10 hover:text-red-300 font-medium"
      >
        {confirmLabel}
      </button>
    </div>,
    document.body,
  );
}
