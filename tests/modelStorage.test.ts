/** Todo claims for `useModelStorage`. */
import { describe, it } from "vitest";
import type { useModelStorage } from "../src/features/model-storage/useModelStorage";

describe("useModelStorage", () => {
  it.todo("When a project is opened, loads its tree and the tree at the base its changes are read from, and lists it first among recent projects");
  it.todo("When the base commit moves, reads the tree at the new base; while it stays, reads it no more");
  it.todo("When another worktree of the open project is opened, loads it without listing it among recent projects");
});
