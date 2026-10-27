import { describe, expect, it } from "vitest";
import { sidebarMode } from "../src/app/sidebar";

describe("sidebar", () => {
  it("While the window is wide, docks the tree beside the page unless the user hid it", () => {
    expect(sidebarMode(false, true, false)).toBe("docked");
    expect(sidebarMode(false, false, false)).toBe("hidden");
    // A drawer left open from a narrow window never shows on a wide one.
    expect(sidebarMode(false, false, true)).toBe("hidden");
  });

  it("While the window is narrow, shows the tree only as a drawer over the page, whatever the docked choice", () => {
    expect(sidebarMode(true, true, false)).toBe("hidden");
    expect(sidebarMode(true, true, true)).toBe("drawer");
    expect(sidebarMode(true, false, true)).toBe("drawer");
  });
});
