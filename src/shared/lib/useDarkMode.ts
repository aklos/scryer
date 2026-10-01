import { useEffect, useState } from "react";

/**
 * Reactive read of the active color mode. Theme isn't held in React state — it
 * lives on `document.documentElement`'s `dark` class (see theme.ts) — so a plain
 * `classList.contains` read at render time goes stale when the user toggles the
 * theme. This observes that class and re-renders the caller on change.
 */
export function useDarkMode(): boolean {
  const [isDark, setIsDark] = useState(() =>
    document.documentElement.classList.contains("dark"),
  );

  useEffect(() => {
    const el = document.documentElement;
    const obs = new MutationObserver(() => setIsDark(el.classList.contains("dark")));
    obs.observe(el, { attributes: true, attributeFilter: ["class"] });
    return () => obs.disconnect();
  }, []);

  return isDark;
}
