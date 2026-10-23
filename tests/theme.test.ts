/** Todo claims for `applyTheme` and `applyColorMode`. */
import { describe, it } from "vitest";
import type { applyTheme, applyColorMode } from "../src/shared/theme/theme";

describe("applyTheme", () => {
  it.todo("When a theme picks a palette other than a family's default, writes that palette's shades as the family's CSS custom properties on the document root, and clears them when it returns to the default");
});

describe("applyColorMode", () => {
  it.todo("While the color mode is system, follows the OS dark preference each time it changes");
});
