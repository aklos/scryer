/**
 * The app's memory of when each project was last opened — "since you last
 * looked" is the developer's own clock, so it lives in their localStorage,
 * keyed by project path.
 */

const OPENED_KEY = "scryer:last-opened:";

/** When the project was opened before this time (unix seconds), recording
 *  now as the new last-opened. 0 when it never was. */
export function takeLastOpened(projectPath: string): number {
  const now = Math.floor(Date.now() / 1000);
  try {
    const prev = Number(localStorage.getItem(OPENED_KEY + projectPath) ?? 0) || 0;
    localStorage.setItem(OPENED_KEY + projectPath, String(now));
    return prev;
  } catch {
    return 0;
  }
}
