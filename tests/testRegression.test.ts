/**
 * An attached test's regression tint (`src/health.ts`): orange when the test's
 * content changed since the last reconcile, red when it no longer resolves —
 * and gone outranks changed, because an unresolvable test can't be "just
 * edited".
 */
import { describe, expect, it } from "vitest";
import { testRegression } from "../src/entities/model/health";

describe("testRegression", () => {
  it("While the test resolves and nothing changed, is quiet", () => {
    expect(testRegression(null, null)).toBeNull();
    expect(testRegression("resolved", null)).toBeNull();
  });

  it("When the fingerprint saw the test's content change, reads changed", () => {
    expect(testRegression("resolved", "changed")).toBe("changed");
    expect(testRegression(null, "changed")).toBe("changed");
  });

  it("When the test no longer resolves, from either input, reads gone", () => {
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
