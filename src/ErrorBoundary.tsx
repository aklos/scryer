import { Component } from "react";
import type { ReactNode, ErrorInfo } from "react";
import { WindowControls } from "./TopBar";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("ErrorBoundary caught:", error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        // The window is frameless; a crash unmounts TopBar and with it the only
        // way to move or close the window, so this screen carries its own strip.
        <div className="flex h-screen w-screen flex-col bg-[var(--surface)]">
          <div
            data-tauri-drag-region
            className="flex h-9 shrink-0 items-center justify-end px-2 select-none"
          >
            <WindowControls divider={false} />
          </div>
          <div className="flex min-h-0 flex-1 items-center justify-center p-8">
            <div className="max-w-md rounded-lg border border-red-200 bg-[var(--surface-raised)] p-6 shadow-lg dark:border-red-800">
              <h1 className="text-lg font-semibold text-red-600 dark:text-red-400">Something went wrong</h1>
              <p className="mt-2 text-sm text-[var(--text-secondary)]">{this.state.error.message}</p>
              <button
                type="button"
                className="mt-4 rounded-md bg-zinc-800 px-4 py-2 text-sm text-white hover:bg-zinc-700 dark:bg-zinc-200 dark:text-zinc-900 dark:hover:bg-zinc-300"
                onClick={() => this.setState({ error: null })}
              >
                Try again
              </button>
            </div>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
