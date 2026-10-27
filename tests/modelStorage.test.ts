/** Todo claims for `useModelStorage`. */
import { describe, it } from "vitest";
import type { useModelStorage } from "../src/features/model-storage/useModelStorage";

describe("useModelStorage", () => {
  it.todo("When a project is opened, loads its planned and committed trees and lists it first among recent projects");
  it.todo("When another worktree of the open project is opened, loads it without listing it among recent projects");
});
