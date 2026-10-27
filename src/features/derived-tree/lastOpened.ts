/**
 * The app's memory of when you last looked at each project's landing —
 * "since you last looked" is the developer's own clock, so it lives in their
 * localStorage, keyed by project path. A look ends when you leave the
 * landing, so everything you saw there drops out of the next one.
 */

const OPENED_KEY = "scryer:last-opened:";

/** When you last left this project's landing (unix seconds); 0 when you
 *  never looked. */
export function lastLooked(projectPath: string): number {
  try {
    return Number(localStorage.getItem(OPENED_KEY + projectPath) ?? 0) || 0;
  } catch {
    return 0;
  }
}

/** Record now as the last look at this project's landing. */
export function markLooked(projectPath: string): void {
  try {
    localStorage.setItem(OPENED_KEY + projectPath, String(Math.floor(Date.now() / 1000)));
  } catch {
    // Storage blocked: the next look shows the same window, nothing worse.
  }
}
