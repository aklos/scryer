import { useCallback, useState } from "react";
import { Button, Select } from "./ui";
import {
  type ThemeConfig,
  type ColorMode,
  type PaletteName,
  type PaletteRole,
  THEME_ROLES,
  PALETTES,
  SHADES,
  DEFAULT_THEME,
  applyTheme,
  saveTheme,
  isDefaultTheme,
} from "./theme";
import { ChevronUp, ChevronDown, Sun, Moon, Monitor } from "lucide-react";

function PalettePreview({ name, offset = 0 }: { name: string; offset?: number }) {
  const p = PALETTES[name];
  if (!p) return null;
  const previewShades = ["300", "400", "500", "600", "700"] as const;
  return (
    <div className="flex gap-px">
      {previewShades.map((shade) => {
        const idx = SHADES.indexOf(shade);
        const shifted = SHADES[Math.max(0, Math.min(SHADES.length - 1, idx + offset))];
        return (
          <div
            key={shade}
            className="w-3 h-3 rounded-sm first:rounded-l last:rounded-r"
            style={{ backgroundColor: p[shifted] }}
          />
        );
      })}
    </div>
  );
}

function ShadeOffset({
  value,
  onChange,
}: {
  value: number;
  onChange: (v: number) => void;
}) {
  return (
    <div className="flex flex-col -gap-0.5">
      <button
        type="button"
        disabled={value >= 3}
        className="text-zinc-400 hover:text-zinc-600 dark:text-zinc-500 dark:hover:text-zinc-300 disabled:opacity-20 cursor-pointer disabled:cursor-default"
        onClick={() => onChange(value + 1)}
      >
        <ChevronUp size={12} />
      </button>
      <button
        type="button"
        disabled={value <= -3}
        className="text-zinc-400 hover:text-zinc-600 dark:text-zinc-500 dark:hover:text-zinc-300 disabled:opacity-20 cursor-pointer disabled:cursor-default"
        onClick={() => onChange(value - 1)}
      >
        <ChevronDown size={12} />
      </button>
    </div>
  );
}

/** Clickable shade strip for canvas/node bg controls. */
function ShadeStrip({
  palette,
  value,
  hasWhite,
  reverse,
  onChange,
}: {
  palette: string;
  value: number;
  hasWhite?: boolean;
  reverse?: boolean;
  onChange: (v: number) => void;
}) {
  const p = PALETTES[palette];
  if (!p) return null;
  // Light mode: shades 50–400 (indices 0–4). Dark mode: shades 600–950 (indices 6–10).
  const indices = reverse ? [6, 7, 8, 9, 10] : [0, 1, 2, 3, 4];
  return (
    <div className="flex gap-px items-center">
      {hasWhite && !reverse && (
        <button
          type="button"
          className={`w-5 h-5 rounded-sm border cursor-pointer ${
            value === -1
              ? "border-zinc-900 dark:border-zinc-100 ring-1 ring-zinc-900 dark:ring-zinc-100"
              : "border-zinc-300 dark:border-zinc-600"
          }`}
          style={{ backgroundColor: "#ffffff" }}
          title="White"
          onClick={() => onChange(-1)}
        />
      )}
      {indices.map((idx) => (
        <button
          key={idx}
          type="button"
          className={`w-5 h-5 rounded-sm border cursor-pointer ${
            value === idx
              ? "border-zinc-900 dark:border-zinc-100 ring-1 ring-zinc-900 dark:ring-zinc-100"
              : "border-zinc-300 dark:border-zinc-600"
          }`}
          style={{ backgroundColor: p[SHADES[idx]] }}
          title={SHADES[idx]}
          onClick={() => onChange(idx)}
        />
      ))}
    </div>
  );
}

export function ThemePanel({
  theme,
  onChange,
  onClose,
}: {
  theme: ThemeConfig;
  onChange: (theme: ThemeConfig) => void;
  onClose: () => void;
}) {
  const [localTheme, setLocalTheme] = useState(theme);

  const commit = useCallback(
    (next: ThemeConfig) => {
      setLocalTheme(next);
      applyTheme(next);
      saveTheme(next);
      onChange(next);
    },
    [onChange],
  );

  const handlePaletteChange = useCallback(
    (key: PaletteRole, value: PaletteName) => {
      commit({ ...localTheme, [key]: value });
    },
    [localTheme, commit],
  );

  const handleOffsetChange = useCallback(
    (key: PaletteRole, value: number) => {
      const offsets = { ...localTheme.offsets, [key]: value };
      if (value === 0) delete offsets[key];
      commit({ ...localTheme, offsets });
    },
    [localTheme, commit],
  );

  const handleReset = useCallback(() => {
    commit({ ...DEFAULT_THEME, offsets: {} });
  }, [commit]);

  return (
    <div className="absolute inset-0 z-20 flex items-center justify-center bg-black/20 backdrop-blur-[2px]">
      <div className="w-[420px] max-h-[80vh] rounded-lg border border-zinc-200 bg-white shadow-lg dark:border-zinc-700 dark:bg-zinc-800 flex flex-col overflow-hidden">
        <div className="flex items-center justify-between border-b border-zinc-200 px-4 py-3 dark:border-zinc-700">
          <h2 className="text-sm font-medium text-zinc-800 dark:text-zinc-100">
            Theme
          </h2>
          <button
            type="button"
            className="text-zinc-400 hover:text-zinc-600 cursor-pointer dark:text-zinc-500 dark:hover:text-zinc-300"
            onClick={onClose}
          >
            &times;
          </button>
        </div>
        <div className="flex-1 overflow-y-auto p-4 flex flex-col gap-4">
          {/* Color mode selector */}
          <div className="flex flex-col gap-2">
            <div className="text-[10px] font-medium uppercase tracking-wider text-zinc-400 dark:text-zinc-500">
              Appearance
            </div>
            <div className="flex gap-1">
              {([
                { mode: "light" as ColorMode, icon: Sun, label: "Light" },
                { mode: "dark" as ColorMode, icon: Moon, label: "Dark" },
                { mode: "system" as ColorMode, icon: Monitor, label: "System" },
              ]).map(({ mode, icon: Icon, label }) => (
                <button
                  key={mode}
                  type="button"
                  className={`flex items-center gap-1.5 px-3 py-1.5 rounded-md text-xs font-medium cursor-pointer transition-colors ${
                    localTheme.colorMode === mode
                      ? "bg-zinc-200 text-zinc-800 dark:bg-zinc-600 dark:text-zinc-100"
                      : "text-zinc-500 hover:text-zinc-700 hover:bg-zinc-100 dark:text-zinc-400 dark:hover:text-zinc-200 dark:hover:bg-zinc-700"
                  }`}
                  onClick={() => commit({ ...localTheme, colorMode: mode })}
                >
                  <Icon size={14} />
                  {label}
                </button>
              ))}
            </div>
          </div>

          {/* Background controls — light */}
          <div className="flex flex-col gap-3">
            <div className="text-[10px] font-medium uppercase tracking-wider text-zinc-400 dark:text-zinc-500">
              Light mode
            </div>
            <div className="flex items-center gap-3">
              <div className="flex-1 min-w-0">
                <div className="text-xs font-medium text-zinc-700 dark:text-zinc-200">Canvas</div>
              </div>
              <ShadeStrip
                palette={localTheme.zinc}
                value={localTheme.canvasLight}
                onChange={(v) => commit({ ...localTheme, canvasLight: v })}
              />
            </div>
            <div className="flex items-center gap-3">
              <div className="flex-1 min-w-0">
                <div className="text-xs font-medium text-zinc-700 dark:text-zinc-200">Nodes</div>
              </div>
              <ShadeStrip
                palette={localTheme.zinc}
                value={localTheme.nodeLight}
                hasWhite
                onChange={(v) => commit({ ...localTheme, nodeLight: v })}
              />
            </div>
          </div>

          {/* Background controls — dark */}
          <div className="flex flex-col gap-3">
            <div className="text-[10px] font-medium uppercase tracking-wider text-zinc-400 dark:text-zinc-500">
              Dark mode
            </div>
            <div className="flex items-center gap-3">
              <div className="flex-1 min-w-0">
                <div className="text-xs font-medium text-zinc-700 dark:text-zinc-200">Canvas</div>
              </div>
              <ShadeStrip
                palette={localTheme.zinc}
                value={localTheme.canvasDark}
                reverse
                onChange={(v) => commit({ ...localTheme, canvasDark: v })}
              />
            </div>
            <div className="flex items-center gap-3">
              <div className="flex-1 min-w-0">
                <div className="text-xs font-medium text-zinc-700 dark:text-zinc-200">Nodes</div>
              </div>
              <ShadeStrip
                palette={localTheme.zinc}
                value={localTheme.nodeDark}
                reverse
                onChange={(v) => commit({ ...localTheme, nodeDark: v })}
              />
            </div>
          </div>

          {/* Palette roles */}
          <div className="flex flex-col gap-3">
            <div className="text-[10px] font-medium uppercase tracking-wider text-zinc-400 dark:text-zinc-500">
              Colors
            </div>
            {THEME_ROLES.map((role) => {
              const offset = localTheme.offsets[role.key] ?? 0;
              return (
                <div key={role.key} className="flex items-center gap-3">
                  <div className="flex-1 min-w-0">
                    <div className="text-xs font-medium text-zinc-700 dark:text-zinc-200">
                      {role.label}
                    </div>
                    <div className="text-[10px] text-zinc-400 dark:text-zinc-500 truncate">
                      {role.description}
                    </div>
                  </div>
                  <div className="flex items-center gap-2 shrink-0">
                    <PalettePreview name={localTheme[role.key]} offset={offset} />
                    <Select
                      options={role.palettes.map((p) => ({ value: p, label: p }))}
                      value={localTheme[role.key]}
                      onChange={(v) => handlePaletteChange(role.key, v as PaletteName)}
                    />
                    <ShadeOffset
                      value={offset}
                      onChange={(v) => handleOffsetChange(role.key, v)}
                    />
                  </div>
                </div>
              );
            })}
          </div>
        </div>
        <div className="flex items-center justify-between border-t border-zinc-200 px-4 py-3 dark:border-zinc-700">
          <Button
            variant="ghost"
            size="sm"
            disabled={isDefaultTheme(localTheme)}
            onClick={handleReset}
          >
            Reset to defaults
          </Button>
          <Button variant="secondary" size="md" onClick={onClose}>
            Done
          </Button>
        </div>
      </div>
    </div>
  );
}
