/**
 * Right-click "Copy ID" for the node page. A context provider owns the menu
 * state and renders the shared {@link ContextMenu}; any primitive on the page
 * calls `usePageMenu()(e, items)` from its own `onContextMenu` to open it. IDs
 * copied here are the references you paste into an agent prompt (the MCP
 * `read_model` can scope to a node/responsibility id).
 *
 * Handlers stop propagation, so the innermost primitive wins: right-clicking a
 * responsibility row copies the claim id; right-clicking elsewhere on the page
 * falls through to the node id.
 */

import { createContext, useContext, useState, type ReactNode } from "react";
import { ContextMenu, type ContextMenuItem } from "./ContextMenu";
import { useToast } from "./Toast";

type OpenMenu = (e: React.MouseEvent, items: ContextMenuItem[]) => void;

const PageMenuContext = createContext<OpenMenu>(() => {});

export const usePageMenu = () => useContext(PageMenuContext);

/** Copy an id to the clipboard, confirming via a toast. */
export function useCopyId() {
  const { toast } = useToast();
  return (id: string) => {
    void navigator.clipboard.writeText(id);
    toast(`Copied ${id}`, "success");
  };
}

/** The standard "Copy ID" menu item for a primitive. */
export function copyIdItem(id: string, copyId: (id: string) => void): ContextMenuItem {
  return { id: `copy-${id}`, label: "Copy ID", hint: id, onSelect: () => copyId(id) };
}

export function PageMenuProvider({ children }: { children: ReactNode }) {
  const [menu, setMenu] = useState<{
    x: number;
    y: number;
    items: ContextMenuItem[];
  } | null>(null);

  const open: OpenMenu = (e, items) => {
    if (!items.length) return;
    e.preventDefault();
    e.stopPropagation();
    setMenu({ x: e.clientX, y: e.clientY, items });
  };

  return (
    <PageMenuContext.Provider value={open}>
      {children}
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          items={menu.items}
          searchable={false}
          onClose={() => setMenu(null)}
        />
      )}
    </PageMenuContext.Provider>
  );
}
