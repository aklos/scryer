/**
 * Layer-1 viewer scaffold.
 *
 * Holds the navigation path (a stack of surface ids) and the mutable surface
 * map (reordering writes back here so order survives navigation). Composes
 * Breadcrumbs over a pan/zoom viewport that hosts the current Surface.
 */

import { useCallback, useRef, useState } from "react";
import { ErrorBoundary } from "./ErrorBoundary";
import { ToastProvider } from "./Toast";
import { Breadcrumbs } from "./Breadcrumbs";
import { PanZoom } from "./PanZoom";
import { Surface } from "./Surface";
import { ModelContext } from "./modelcontext";
import { mockModel } from "./mockModel";
import type { Altitude, Model, Surface as SurfaceModel } from "./viewmodel";

export default function App() {
  // Surfaces are mutable — reordering writes back here.
  const [surfaces, setSurfaces] = useState<Model["surfaces"]>(
    () => mockModel.surfaces,
  );
  // Navigation stack of surface ids, root first.
  const [path, setPath] = useState<string[]>(() => [mockModel.rootSurfaceId]);

  const model: Model = { surfaces, rootSurfaceId: mockModel.rootSurfaceId };
  const currentId = path[path.length - 1];
  const current = surfaces[currentId];
  const ancestorAltitudes = path.slice(0, -1).flatMap((id) => {
    const s = surfaces[id];
    return s ? [s.altitude] : [];
  });

  const surfacesRef = useRef(surfaces);
  surfacesRef.current = surfaces;
  const pathRef = useRef(path);
  pathRef.current = path;

  const CHILD_ALT: Record<string, Altitude> = {
    system: "container",
    container: "component",
    component: "component",
  };

  const handleNavigate = useCallback((entryId: string) => {
    const cid = pathRef.current[pathRef.current.length - 1];
    const surfs = surfacesRef.current;
    const entry = surfs[cid]?.entries.find((e) => e.id === entryId);
    if (!entry) return;

    if (entry.childSurfaceId && surfs[entry.childSurfaceId]) {
      setPath((p) =>
        p[p.length - 1] === entry.childSurfaceId ? p : [...p, entry.childSurfaceId!],
      );
      return;
    }

    const childId = `child-${entryId}`;
    const childAlt = CHILD_ALT[entry.kind] ?? "component";
    setSurfaces((s) => ({
      ...s,
      [childId]: { id: childId, altitude: childAlt, entries: [], groups: [] },
      [cid]: {
        ...s[cid],
        entries: s[cid].entries.map((e) =>
          e.id === entryId ? { ...e, childSurfaceId: childId } : e,
        ),
      },
    }));
    setPath((p) => [...p, childId]);
  }, []);

  const handleJump = useCallback((index: number) => {
    setPath((p) => (index < p.length - 1 ? p.slice(0, index + 1) : p));
  }, []);

  const handleSurfaceChange = useCallback((next: SurfaceModel) => {
    setSurfaces((s) => ({ ...s, [next.id]: next }));
  }, []);

  return (
    <ErrorBoundary>
      <ToastProvider>
        <ModelContext.Provider value={surfaces}>
        <div className="flex h-screen w-screen flex-col bg-[var(--surface-canvas)]">
          <Breadcrumbs model={model} path={path} onJump={handleJump} />
          <div className="relative flex-1">
            {current ? (
              // resetKey clears pan/zoom whenever the surface changes.
              <PanZoom resetKey={currentId}>
                <Surface
                  surface={current}
                  ancestorAltitudes={ancestorAltitudes}
                  onChange={handleSurfaceChange}
                  onNavigate={handleNavigate}
                  onBack={handleJump}
                />
              </PanZoom>
            ) : (
              <div className="flex h-full items-center justify-center text-sm text-[var(--text-muted)]">
                Surface not found
              </div>
            )}
          </div>
        </div>
        </ModelContext.Provider>
      </ToastProvider>
    </ErrorBoundary>
  );
}
