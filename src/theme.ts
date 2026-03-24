/**
 * Theme system — lets users swap Tailwind color palettes for each semantic role.
 *
 * The trick: Tailwind v4 compiles `bg-blue-500` to `background-color: var(--color-blue-500)`.
 * By overriding `--color-blue-*` CSS variables on `:root`, ALL Tailwind utility classes
 * automatically pick up the new colors — no component code changes needed.
 */

import { createContext } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Tick counter that increments on every theme change.
 * ReactFlow node/edge components consume this context so they re-render
 * when the theme changes (their inline hex styles need recalculation).
 */
export const ThemeContext = createContext(0);

// ---------------------------------------------------------------------------
// Palette data — Tailwind's default hex values for each color family
// ---------------------------------------------------------------------------

export type Shade = "50" | "100" | "200" | "300" | "400" | "500" | "600" | "700" | "800" | "900" | "950";
export type PaletteValues = Record<Shade, string>;

export const SHADES: Shade[] = ["50", "100", "200", "300", "400", "500", "600", "700", "800", "900", "950"];

/** All Tailwind color palettes with hex values. */
export const PALETTES: Record<string, PaletteValues> = {
  slate:   { 50: "#f8fafc", 100: "#f1f5f9", 200: "#e2e8f0", 300: "#cbd5e1", 400: "#94a3b8", 500: "#64748b", 600: "#475569", 700: "#334155", 800: "#1e293b", 900: "#0f172a", 950: "#020617" },
  gray:    { 50: "#f9fafb", 100: "#f3f4f6", 200: "#e5e7eb", 300: "#d1d5db", 400: "#9ca3af", 500: "#6b7280", 600: "#4b5563", 700: "#374151", 800: "#1f2937", 900: "#111827", 950: "#030712" },
  zinc:    { 50: "#fafafa", 100: "#f4f4f5", 200: "#e4e4e7", 300: "#d4d4d8", 400: "#a1a1aa", 500: "#71717a", 600: "#52525b", 700: "#3f3f46", 800: "#27272a", 900: "#18181b", 950: "#09090b" },
  neutral: { 50: "#fafafa", 100: "#f5f5f5", 200: "#e5e5e5", 300: "#d4d4d4", 400: "#a3a3a3", 500: "#737373", 600: "#525252", 700: "#404040", 800: "#262626", 900: "#171717", 950: "#0a0a0a" },
  stone:   { 50: "#fafaf9", 100: "#f5f5f4", 200: "#e7e5e4", 300: "#d6d3d1", 400: "#a8a29e", 500: "#78716c", 600: "#57534e", 700: "#44403c", 800: "#292524", 900: "#1c1917", 950: "#0c0a09" },
  red:     { 50: "#fef2f2", 100: "#fee2e2", 200: "#fecaca", 300: "#fca5a5", 400: "#f87171", 500: "#ef4444", 600: "#dc2626", 700: "#b91c1c", 800: "#991b1b", 900: "#7f1d1d", 950: "#450a0a" },
  orange:  { 50: "#fff7ed", 100: "#ffedd5", 200: "#fed7aa", 300: "#fdba74", 400: "#fb923c", 500: "#f97316", 600: "#ea580c", 700: "#c2410c", 800: "#9a3412", 900: "#7c2d12", 950: "#431407" },
  amber:   { 50: "#fffbeb", 100: "#fef3c7", 200: "#fde68a", 300: "#fcd34d", 400: "#fbbf24", 500: "#f59e0b", 600: "#d97706", 700: "#b45309", 800: "#92400e", 900: "#78350f", 950: "#451a03" },
  yellow:  { 50: "#fefce8", 100: "#fef9c3", 200: "#fef08a", 300: "#fde047", 400: "#facc15", 500: "#eab308", 600: "#ca8a04", 700: "#a16207", 800: "#854d0e", 900: "#713f12", 950: "#422006" },
  lime:    { 50: "#f7fee7", 100: "#ecfccb", 200: "#d9f99d", 300: "#bef264", 400: "#a3e635", 500: "#84cc16", 600: "#65a30d", 700: "#4d7c0f", 800: "#3f6212", 900: "#365314", 950: "#1a2e05" },
  green:   { 50: "#f0fdf4", 100: "#dcfce7", 200: "#bbf7d0", 300: "#86efac", 400: "#4ade80", 500: "#22c55e", 600: "#16a34a", 700: "#15803d", 800: "#166534", 900: "#14532d", 950: "#052e16" },
  emerald: { 50: "#ecfdf5", 100: "#d1fae5", 200: "#a7f3d0", 300: "#6ee7b7", 400: "#34d399", 500: "#10b981", 600: "#059669", 700: "#047857", 800: "#065f46", 900: "#064e3b", 950: "#022c22" },
  teal:    { 50: "#f0fdfa", 100: "#ccfbf1", 200: "#99f6e4", 300: "#5eead4", 400: "#2dd4bf", 500: "#14b8a6", 600: "#0d9488", 700: "#0f766e", 800: "#115e59", 900: "#134e4a", 950: "#042f2e" },
  cyan:    { 50: "#ecfeff", 100: "#cffafe", 200: "#a5f3fc", 300: "#67e8f9", 400: "#22d3ee", 500: "#06b6d4", 600: "#0891b2", 700: "#0e7490", 800: "#155e75", 900: "#164e63", 950: "#083344" },
  sky:     { 50: "#f0f9ff", 100: "#e0f2fe", 200: "#bae6fd", 300: "#7dd3fc", 400: "#38bdf8", 500: "#0ea5e9", 600: "#0284c7", 700: "#0369a1", 800: "#075985", 900: "#0c4a6e", 950: "#082f49" },
  blue:    { 50: "#eff6ff", 100: "#dbeafe", 200: "#bfdbfe", 300: "#93c5fd", 400: "#60a5fa", 500: "#3b82f6", 600: "#2563eb", 700: "#1d4ed8", 800: "#1e40af", 900: "#1e3a8a", 950: "#172554" },
  indigo:  { 50: "#eef2ff", 100: "#e0e7ff", 200: "#c7d2fe", 300: "#a5b4fc", 400: "#818cf8", 500: "#6366f1", 600: "#4f46e5", 700: "#4338ca", 800: "#3730a3", 900: "#312e81", 950: "#1e1b4b" },
  violet:  { 50: "#f5f3ff", 100: "#ede9fe", 200: "#ddd6fe", 300: "#c4b5fd", 400: "#a78bfa", 500: "#8b5cf6", 600: "#7c3aed", 700: "#6d28d9", 800: "#5b21b6", 900: "#4c1d95", 950: "#2e1065" },
  purple:  { 50: "#faf5ff", 100: "#f3e8ff", 200: "#e9d5ff", 300: "#d8b4fe", 400: "#c084fc", 500: "#a855f7", 600: "#9333ea", 700: "#7e22ce", 800: "#6b21a8", 900: "#581c87", 950: "#3b0764" },
  fuchsia: { 50: "#fdf4ff", 100: "#fae8ff", 200: "#f5d0fe", 300: "#f0abfc", 400: "#e879f9", 500: "#d946ef", 600: "#c026d3", 700: "#a21caf", 800: "#86198f", 900: "#701a75", 950: "#4a044e" },
  pink:    { 50: "#fdf2f8", 100: "#fce7f3", 200: "#fbcfe8", 300: "#f9a8d4", 400: "#f472b6", 500: "#ec4899", 600: "#db2777", 700: "#be185d", 800: "#9d174d", 900: "#831843", 950: "#500724" },
  rose:    { 50: "#fff1f2", 100: "#ffe4e6", 200: "#fecdd3", 300: "#fda4af", 400: "#fb7185", 500: "#f43f5e", 600: "#e11d48", 700: "#be123c", 800: "#9f1239", 900: "#881337", 950: "#4c0519" },
};

// ---------------------------------------------------------------------------
// Palette metadata — human-readable labels and grouping for the UI
// ---------------------------------------------------------------------------

export const GRAY_PALETTES = ["slate", "gray", "zinc", "neutral", "stone"] as const;
export const CHROMATIC_PALETTES = [
  "red", "orange", "amber", "yellow", "lime", "green", "emerald",
  "teal", "cyan", "sky", "blue", "indigo", "violet", "purple", "fuchsia", "pink", "rose",
] as const;
export const ALL_PALETTE_NAMES = [...GRAY_PALETTES, ...CHROMATIC_PALETTES] as const;
export type PaletteName = (typeof ALL_PALETTE_NAMES)[number];

// ---------------------------------------------------------------------------
// Theme configuration
// ---------------------------------------------------------------------------

/** Keys for palette roles (used in both ThemeConfig palette and offset maps). */
export type PaletteRole = "zinc" | "blue" | "emerald" | "amber" | "red" | "violet" | "indigo" | "cyan" | "slate" | "orange" | "teal";
export const PALETTE_ROLE_KEYS: PaletteRole[] = ["zinc", "blue", "emerald", "amber", "red", "violet", "indigo", "cyan", "slate", "orange", "teal"];

export type ColorMode = "light" | "dark" | "system";

/** Maps semantic roles to Tailwind color family names + optional shade offsets. */
export interface ThemeConfig {
  /** Color mode: light, dark, or follow system (default: system) */
  colorMode: ColorMode;
  /** Neutral chrome — backgrounds, borders, text (default: zinc) */
  zinc: PaletteName;
  /** Primary accent — buttons, selection, proposed status (default: blue) */
  blue: PaletteName;
  /** Implemented status, process kind, success toast (default: emerald) */
  emerald: PaletteName;
  /** Changed status, model kind, warning (default: amber) */
  amber: PaletteName;
  /** Deprecated status, danger, error (default: red) */
  red: PaletteName;
  /** Component kind, accent-alt (default: violet) */
  violet: PaletteName;
  /** Secondary accent — buttons, focus rings (default: indigo) */
  indigo: PaletteName;
  /** Container kind (default: cyan) */
  cyan: PaletteName;
  /** System kind, structural/edges (default: slate) */
  slate: PaletteName;
  /** Hint warnings (default: orange) */
  orange: PaletteName;
  /** Hint info (default: teal) */
  teal: PaletteName;
  /** Per-role shade offset: shifts all shades by N steps (-3 to +3). */
  offsets: Partial<Record<PaletteRole, number>>;
  /** Light mode: canvas bg shade index (0–10, default 0 = "50"). */
  canvasLight: number;
  /** Light mode: node fill shade index (-1 = white, 0–10, default -1). */
  nodeLight: number;
  /** Dark mode: canvas bg shade index (0–10, default 10 = "950"). */
  canvasDark: number;
  /** Dark mode: node fill shade index (-1 = auto, 0–10, default 8 = "800"). */
  nodeDark: number;
}

/** Role metadata for the theme editor UI. */
export const THEME_ROLES: { key: PaletteRole; label: string; description: string; palettes: readonly string[] }[] = [
  { key: "zinc",    label: "Neutral",     description: "Chrome, backgrounds, borders, text",        palettes: GRAY_PALETTES },
  { key: "slate",   label: "Structural",  description: "System kind, edges",                        palettes: ALL_PALETTE_NAMES },
  { key: "blue",    label: "Accent",      description: "Buttons, selection, proposed status",        palettes: CHROMATIC_PALETTES },
  { key: "emerald", label: "Success",     description: "Implemented status, process kind",           palettes: CHROMATIC_PALETTES },
  { key: "amber",   label: "Warning",     description: "Changed status, model kind",                 palettes: CHROMATIC_PALETTES },
  { key: "red",     label: "Danger",      description: "Deprecated status, errors",                  palettes: CHROMATIC_PALETTES },
  { key: "violet",  label: "Secondary",   description: "Component kind, accent alt",                 palettes: CHROMATIC_PALETTES },
  { key: "indigo",  label: "Tertiary",    description: "Secondary buttons, focus rings",             palettes: CHROMATIC_PALETTES },
  { key: "cyan",    label: "Container",   description: "Container kind",                             palettes: CHROMATIC_PALETTES },
  { key: "orange",  label: "Hint warning", description: "Hint warning badges",                       palettes: CHROMATIC_PALETTES },
  { key: "teal",    label: "Hint info",   description: "Hint info badges",                           palettes: CHROMATIC_PALETTES },
];

export const DEFAULT_THEME: ThemeConfig = {
  colorMode: "system",
  zinc: "zinc",
  blue: "blue",
  emerald: "emerald",
  amber: "amber",
  red: "red",
  violet: "violet",
  indigo: "indigo",
  cyan: "cyan",
  slate: "slate",
  orange: "orange",
  teal: "teal",
  offsets: {},
  canvasLight: 1,
  nodeLight: -1,
  canvasDark: 9,
  nodeDark: 8,
};

// ---------------------------------------------------------------------------
// Apply / load / save
// ---------------------------------------------------------------------------

const STORAGE_KEY = "scryer:theme";

/** Resolve a shade after applying an offset, clamping to valid range. */
function shiftShade(shade: Shade, offset: number): Shade {
  const idx = SHADES.indexOf(shade);
  return SHADES[Math.max(0, Math.min(SHADES.length - 1, idx + offset))];
}

/** Listener cleanup for system color-scheme changes. */
let _systemDarkListener: (() => void) | null = null;

/** Resolve whether dark mode is active given a color mode preference. */
function isDarkActive(mode: ColorMode): boolean {
  if (mode === "dark") return true;
  if (mode === "light") return false;
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

/** Apply the `.dark` class on `<html>` based on color mode. */
export function applyColorMode(mode: ColorMode): void {
  const root = document.documentElement;

  // Clean up previous system listener
  if (_systemDarkListener) {
    _systemDarkListener();
    _systemDarkListener = null;
  }

  const apply = (dark: boolean) => {
    root.classList.toggle("dark", dark);
  };

  apply(isDarkActive(mode));

  // Sync Tauri webview theme — forces the webview's prefers-color-scheme to match.
  // For "system", reset to null so it follows the OS automatically.
  const win = getCurrentWindow();
  if (mode === "system") {
    win.setTheme(null).catch(() => {});
    const mql = window.matchMedia("(prefers-color-scheme: dark)");
    const handler = (e: MediaQueryListEvent) => apply(e.matches);
    mql.addEventListener("change", handler);
    _systemDarkListener = () => mql.removeEventListener("change", handler);
  } else {
    win.setTheme(mode === "dark" ? "dark" : "light").catch(() => {});
  }
}

/** Apply a theme by overriding Tailwind CSS custom properties on :root. */
export function applyTheme(theme: ThemeConfig): void {
  const root = document.documentElement;

  // Apply color mode (light/dark/system)
  applyColorMode(theme.colorMode);

  for (const family of PALETTE_ROLE_KEYS) {
    const chosen = theme[family];
    const offset = theme.offsets[family] ?? 0;
    // Only skip overrides when using the default palette with no shift
    if (chosen === family && offset === 0) {
      for (const shade of SHADES) {
        root.style.removeProperty(`--color-${family}-${shade}`);
      }
    } else {
      const palette = PALETTES[chosen];
      if (!palette) continue;
      for (const shade of SHADES) {
        root.style.setProperty(`--color-${family}-${shade}`, palette[shiftShade(shade, offset)]);
      }
    }
  }

  // Derive canvas/node CSS variables from the neutral palette via a dynamic stylesheet.
  // (Cannot use inline styles — they'd override the .dark selector.)
  const np = PALETTES[theme.zinc];
  const nOff = theme.offsets.zinc ?? 0;
  const clamp = (v: number) => Math.max(0, Math.min(SHADES.length - 1, v));
  const sh = (idx: number) => np[shiftShade(SHADES[clamp(idx)], nOff)];

  // Light mode
  const canvasLightHex = sh(theme.canvasLight);
  const nodeLightHex = theme.nodeLight < 0 ? "#ffffff" : sh(theme.nodeLight);
  const gridLightIdx = Math.min(theme.canvasLight + 3, SHADES.length - 1);

  // Dark mode
  const canvasDarkHex = sh(theme.canvasDark);
  const nodeDarkHex = sh(theme.nodeDark);
  const gridDarkIdx = Math.max(theme.canvasDark - 3, 0);

  // External & reference fills: darker in light mode, lighter in dark mode
  const extLightHex = mixHex(canvasLightHex, sh(5), 0.3);   // 30% toward darker
  const extDarkHex = mixHex(canvasDarkHex, sh(7), 0.3);     // 30% toward slightly lighter
  const refLightHex = mixHex(canvasLightHex, sh(5), 0.15);  // 15% toward darker
  const refDarkHex = mixHex(canvasDarkHex, sh(7), 0.15);    // 15% toward slightly lighter

  // Person silhouette: same as external fill
  const personLightHex = extLightHex;
  const personDarkHex = extDarkHex;

  let themeStyle = document.getElementById("scryer-theme-vars") as HTMLStyleElement | null;
  if (!themeStyle) {
    themeStyle = document.createElement("style");
    themeStyle.id = "scryer-theme-vars";
    document.head.appendChild(themeStyle);
  }
  themeStyle.textContent = `:root {
  --grid-color: ${sh(gridLightIdx)};
  --xy-background-color: ${canvasLightHex};
  --selection-color: ${sh(10)};
  --scryer-node-bg: ${nodeLightHex};
  --scryer-ext-bg: ${extLightHex};
  --scryer-ref-bg: ${refLightHex};
  --scryer-person-fill: ${personLightHex};
  --scryer-select-stroke: ${sh(10)};
  --scryer-outline-stroke: ${sh(4)};
  --surface-canvas: ${canvasLightHex};
  --surface: ${sh(clamp(theme.canvasLight - 1))};
  --surface-raised: ${nodeLightHex};
  --surface-overlay: ${hexAlpha(nodeLightHex, 0.8)};
  --surface-inset: ${hexAlpha(sh(theme.canvasLight), 0.6)};
  --surface-tint: ${sh(theme.canvasLight)};
  --surface-hover: ${hexAlpha(sh(clamp(theme.canvasLight + 1)), 0.6)};
  --surface-active: ${sh(clamp(theme.canvasLight + 1))};
  --text: ${sh(8)};
  --text-secondary: ${sh(6)};
  --text-tertiary: ${sh(5)};
  --text-muted: ${sh(4)};
  --text-ghost: ${sh(3)};
  --border: ${sh(clamp(theme.canvasLight + 1))};
  --border-subtle: ${hexAlpha(sh(clamp(theme.canvasLight + 1)), 0.6)};
  --border-strong: ${sh(clamp(theme.canvasLight + 2))};
  --border-overlay: ${hexAlpha(sh(clamp(theme.canvasLight + 1)), 0.8)};
}
.dark {
  --xy-background-color: ${canvasDarkHex};
  --selection-color: ${sh(2)};
  --grid-color: ${mixHex(sh(gridDarkIdx), canvasDarkHex, 0.5)};
  --scryer-node-bg: ${nodeDarkHex};
  --scryer-ext-bg: ${extDarkHex};
  --scryer-ref-bg: ${refDarkHex};
  --scryer-person-fill: ${personDarkHex};
  --scryer-select-stroke: ${sh(2)};
  --scryer-outline-stroke: ${sh(7)};
  --surface-canvas: ${canvasDarkHex};
  --surface: ${sh(theme.canvasDark)};
  --surface-raised: ${nodeDarkHex};
  --surface-overlay: ${hexAlpha(sh(theme.canvasDark), 0.8)};
  --surface-inset: ${hexAlpha(nodeDarkHex, 0.6)};
  --surface-tint: ${nodeDarkHex};
  --surface-hover: ${hexAlpha(sh(clamp(theme.nodeDark - 1)), 0.6)};
  --surface-active: ${sh(clamp(theme.nodeDark - 1))};
  --text: ${sh(2)};
  --text-secondary: ${sh(3)};
  --text-tertiary: ${sh(4)};
  --text-muted: ${sh(5)};
  --text-ghost: ${sh(6)};
  --border: ${sh(clamp(theme.canvasDark - 2))};
  --border-subtle: ${hexAlpha(sh(clamp(theme.canvasDark - 2)), 0.6)};
  --border-strong: ${sh(clamp(theme.canvasDark - 3))};
  --border-overlay: ${hexAlpha(sh(clamp(theme.canvasDark - 2)), 0.8)};
}`;
}

/** Load theme from localStorage, falling back to defaults. */
export function loadTheme(): ThemeConfig {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      const theme: ThemeConfig = { ...DEFAULT_THEME, offsets: {} };
      // Restore color mode
      if (parsed.colorMode === "light" || parsed.colorMode === "dark" || parsed.colorMode === "system") {
        theme.colorMode = parsed.colorMode;
      }
      for (const key of PALETTE_ROLE_KEYS) {
        if (parsed[key] && parsed[key] in PALETTES) {
          theme[key] = parsed[key];
        }
      }
      // Restore offsets
      if (parsed.offsets && typeof parsed.offsets === "object") {
        for (const key of PALETTE_ROLE_KEYS) {
          const v = parsed.offsets[key];
          if (typeof v === "number" && v !== 0) {
            theme.offsets[key] = Math.max(-3, Math.min(3, Math.round(v)));
          }
        }
      }
      // Restore background shades
      if (typeof parsed.canvasLight === "number") theme.canvasLight = Math.max(0, Math.min(SHADES.length - 1, Math.round(parsed.canvasLight)));
      if (typeof parsed.nodeLight === "number") theme.nodeLight = Math.max(-1, Math.min(SHADES.length - 1, Math.round(parsed.nodeLight)));
      if (typeof parsed.canvasDark === "number") theme.canvasDark = Math.max(0, Math.min(SHADES.length - 1, Math.round(parsed.canvasDark)));
      if (typeof parsed.nodeDark === "number") theme.nodeDark = Math.max(0, Math.min(SHADES.length - 1, Math.round(parsed.nodeDark)));
      return theme;
    }
  } catch { /* ignore */ }
  return { ...DEFAULT_THEME, offsets: {}, colorMode: "system" };
}

/** Save theme to localStorage. */
export function saveTheme(theme: ThemeConfig): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(theme));
}

/** Check if a theme is the default (no customizations). */
export function isDefaultTheme(theme: ThemeConfig): boolean {
  return theme.colorMode === DEFAULT_THEME.colorMode
    && PALETTE_ROLE_KEYS.every((k) => theme[k] === DEFAULT_THEME[k])
    && PALETTE_ROLE_KEYS.every((k) => (theme.offsets[k] ?? 0) === 0)
    && theme.canvasLight === DEFAULT_THEME.canvasLight && theme.nodeLight === DEFAULT_THEME.nodeLight
    && theme.canvasDark === DEFAULT_THEME.canvasDark && theme.nodeDark === DEFAULT_THEME.nodeDark;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Get the current resolved hex for a Tailwind color variable. */
export function getCssColor(varName: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(varName).trim();
}

/** Simple hex color mixing. */
function mixHex(a: string, b: string, t: number): string {
  const pa = parseHex(a), pb = parseHex(b);
  const r = Math.round(pa[0] + (pb[0] - pa[0]) * t);
  const g = Math.round(pa[1] + (pb[1] - pa[1]) * t);
  const bl = Math.round(pa[2] + (pb[2] - pa[2]) * t);
  return `#${r.toString(16).padStart(2, "0")}${g.toString(16).padStart(2, "0")}${bl.toString(16).padStart(2, "0")}`;
}

function parseHex(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  return [parseInt(h.slice(0, 2), 16), parseInt(h.slice(2, 4), 16), parseInt(h.slice(4, 6), 16)];
}

/** Append hex alpha channel (0–1 → 00–ff). */
function hexAlpha(hex: string, opacity: number): string {
  return hex + Math.round(opacity * 255).toString(16).padStart(2, "0");
}

/** Get the hex value for a themed color by Tailwind family + shade.
 *  Useful for inline styles that need the current theme color. */
export function getThemedHex(family: PaletteRole, shade: Shade): string {
  const theme = loadTheme();
  const palette = PALETTES[theme[family]];
  const offset = theme.offsets[family] ?? 0;
  return palette[shiftShade(shade, offset)];
}
