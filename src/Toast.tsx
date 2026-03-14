import { createContext, useCallback, useContext, useRef, useState } from "react";
import type { ReactNode } from "react";

type ToastVariant = "error" | "success" | "info";

interface Toast {
  id: number;
  message: string;
  variant: ToastVariant;
}

interface ToastContextValue {
  toast: (message: string, variant?: ToastVariant) => void;
}

const ToastContext = createContext<ToastContextValue>({ toast: () => {} });

export const useToast = () => useContext(ToastContext);

const VARIANT_ICON: Record<ToastVariant, string> = {
  error: "!",
  success: "\u2713",
  info: "\u2022",
};

const VARIANT_CLASSES: Record<ToastVariant, string> = {
  error: "text-red-600 dark:text-red-400",
  success: "text-emerald-600 dark:text-emerald-400",
  info: "text-zinc-500 dark:text-zinc-400",
};

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const nextId = useRef(0);

  const toast = useCallback((message: string, variant: ToastVariant = "error") => {
    const id = nextId.current++;
    setToasts((prev) => [...prev, { id, message, variant }]);
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, 5000);
  }, []);

  const dismiss = useCallback((id: number) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  return (
    <ToastContext.Provider value={{ toast }}>
      {children}
      {toasts.length > 0 && (
        <div className="fixed bottom-12 right-4 z-50 flex flex-col gap-1.5 max-w-[280px]">
          {toasts.map((t) => (
            <div
              key={t.id}
              className="flex items-center gap-1.5 rounded border border-zinc-200/80 bg-white/90 dark:border-zinc-700/80 dark:bg-zinc-900/90 backdrop-blur-md px-2.5 py-1.5 text-[11px] shadow-sm cursor-pointer animate-in slide-in-from-bottom-2"
              onClick={() => dismiss(t.id)}
            >
              <span className={`shrink-0 text-[10px] font-medium ${VARIANT_CLASSES[t.variant]}`}>{VARIANT_ICON[t.variant]}</span>
              <span className="text-zinc-600 dark:text-zinc-300">{t.message}</span>
            </div>
          ))}
        </div>
      )}
    </ToastContext.Provider>
  );
}
