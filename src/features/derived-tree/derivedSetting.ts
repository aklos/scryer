/**
 * The "derived tree (preview)" project setting and the app's memory of when
 * each project was last opened. Both are per-viewer conveniences kept in
 * localStorage, keyed by project path: the derived tree is a preview the
 * developer opts into per project until it replaces the authored model, and
 * "since you last looked" is the developer's own clock.
 */

const DERIVED_KEY = "scryer:derived-tree:";
const OPENED_KEY = "scryer:last-opened:";

export function readDerivedSetting(projectPath: string): boolean {
  try {
    return localStorage.getItem(DERIVED_KEY + projectPath) === "1";
  } catch {
    return false;
  }
}

export function writeDerivedSetting(projectPath: string, on: boolean) {
  try {
    if (on) localStorage.setItem(DERIVED_KEY + projectPath, "1");
    else localStorage.removeItem(DERIVED_KEY + projectPath);
  } catch {
    /* localStorage unavailable — the setting lasts this session only */
  }
}

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
