/**
 * An attached test's regression tint (`src/health.ts`): orange when the test's
 * content changed since the last reconcile, red when it no longer resolves —
 * and gone outranks changed, because an unresolvable test can't be "just
 * edited".
 */
import { describe, expect, it } from "vitest";
import { testRegression } from "../src/health";

describe("testRegression", () => {
  it("is quiet while the test resolves and nothing changed", () => {
    expect(testRegression(null, null)).toBeNull();
    expect(testRegression("resolved", null)).toBeNull();
  });

  it("reads changed when the fingerprint saw the test's content change", () => {
    expect(testRegression("resolved", "changed")).toBe("changed");
    expect(testRegression(null, "changed")).toBe("changed");
  });

  it("reads gone when the test no longer resolves, from either input", () => {
    expect(testRegression("symbolMissing", null)).toBe("gone");
    expect(testRegression("fileMissing", null)).toBe("gone");
    expect(testRegression("lineOutOfRange", null)).toBe("gone");
    expect(testRegression("resolved", "broken")).toBe("gone");
    expect(testRegression("resolved", "fileMissing")).toBe("gone");
  });

  it("lets gone outrank changed", () => {
    expect(testRegression("symbolMissing", "changed")).toBe("gone");
  });
});
