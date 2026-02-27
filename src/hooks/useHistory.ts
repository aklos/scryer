import { useCallback, useRef, useState } from "react";
import type { ModelStorageState } from "./useModelStorage";

const MAX_HISTORY = 10;
const DEBOUNCE_MS = 1000;

export interface UseHistoryReturn {
  canUndo: boolean;
  canRedo: boolean;
  undo: () => ModelStorageState | null;
  redo: () => ModelStorageState | null;
  capture: (state: ModelStorageState) => void;
  skipNextCapture: () => void;
  clear: () => void;
}

export function useHistory(): UseHistoryReturn {
  const stack = useRef<string[]>([]);
  const pointer = useRef(-1);
  const skipNext = useRef(false);
  const debounceTimer = useRef<ReturnType<typeof setTimeout>>(null);
  const [, setTick] = useState(0);

  const capture = useCallback((state: ModelStorageState) => {
    if (debounceTimer.current) clearTimeout(debounceTimer.current);
    debounceTimer.current = setTimeout(() => {
      if (skipNext.current) {
        skipNext.current = false;
        return;
      }
      const json = JSON.stringify(state);
      if (pointer.current >= 0 && stack.current[pointer.current] === json) return;
      // Trim redo entries
      stack.current = stack.current.slice(0, pointer.current + 1);
      stack.current.push(json);
      // Enforce max size
      if (stack.current.length > MAX_HISTORY) {
        stack.current = stack.current.slice(stack.current.length - MAX_HISTORY);
      }
      pointer.current = stack.current.length - 1;
      setTick((t) => t + 1);
    }, DEBOUNCE_MS);
  }, []);

  const undo = useCallback((): ModelStorageState | null => {
    if (pointer.current <= 0) return null;
    pointer.current--;
    setTick((t) => t + 1);
    return JSON.parse(stack.current[pointer.current]);
  }, []);

  const redo = useCallback((): ModelStorageState | null => {
    if (pointer.current >= stack.current.length - 1) return null;
    pointer.current++;
    setTick((t) => t + 1);
    return JSON.parse(stack.current[pointer.current]);
  }, []);

  const skipNextCapture = useCallback(() => {
    skipNext.current = true;
  }, []);

  const clear = useCallback(() => {
    if (debounceTimer.current) clearTimeout(debounceTimer.current);
    stack.current = [];
    pointer.current = -1;
    skipNext.current = false;
    setTick((t) => t + 1);
  }, []);

  return {
    canUndo: pointer.current > 0,
    canRedo: pointer.current < stack.current.length - 1,
    undo,
    redo,
    capture,
    skipNextCapture,
    clear,
  };
}
