/** Claims for `DiagramView`'s navigation and camera. */
import { describe, expect, it } from "vitest";
import { cameraProps } from "../src/widgets/diagram-canvas/DiagramView";

describe("DiagramView", () => {
  it.todo("When the user double-clicks a card with children, drills the map into its level, and for a ghost card, opens the level where it lives and selects it");

  it("While a viewport is given, holds the map at exactly that viewport, past the interactive zoom limits, and never fits the drawing", () => {
    const held = cameraProps({ x: -120, y: 40, zoom: 3 });
    expect(held.viewport).toEqual({ x: -120, y: 40, zoom: 3 });
    expect(held.defaultViewport).toEqual({ x: -120, y: 40, zoom: 3 });
    expect(held.fitView).toBe(false);
    expect(held.maxZoom).toBeGreaterThan(3);
    const free = cameraProps(undefined);
    expect(free.fitView).toBe(true);
    expect(free.maxZoom).toBe(2);
  });
});
