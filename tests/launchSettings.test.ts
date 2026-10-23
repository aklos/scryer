/** Todo claims for `resolveLaunch`. */
import { describe, it } from "vitest";
import type { resolveLaunch } from "../src/features/agent-launch/launchSettings";

describe("resolveLaunch", () => {
  it.todo("If the preferred agent is not installed, then launches the first installed agent in the standing order instead");
});
