/**
 * The project's comment threads, re-read every few seconds: agents in any
 * worktree reply while the page is open, and no watcher covers their
 * checkouts. Writes go through the backend, which stores one file per event.
 */

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Thread, ThreadStatus } from "../../entities/model/comments";

const POLL_MS = 5000;

export interface Comments {
  threads: Thread[];
  open: (nodeId: string, anchor: string, text: string) => Promise<void>;
  reply: (thread: string, text: string, status?: ThreadStatus) => Promise<void>;
}

export function useComments(projectPath: string | null): Comments {
  const [threads, setThreads] = useState<Thread[]>([]);

  const refresh = useCallback(async () => {
    if (!projectPath) return;
    try {
      setThreads(await invoke<Thread[]>("list_comments", { cwd: projectPath }));
    } catch {
      /* transient — the next poll tries again */
    }
  }, [projectPath]);

  useEffect(() => {
    void refresh();
    const t = setInterval(() => void refresh(), POLL_MS);
    return () => clearInterval(t);
  }, [refresh]);

  const open = useCallback(
    async (nodeId: string, anchor: string, text: string) => {
      if (!projectPath) return;
      await invoke("open_comment", { cwd: projectPath, nodeId, anchor, text });
      await refresh();
    },
    [projectPath, refresh],
  );

  const reply = useCallback(
    async (thread: string, text: string, status?: ThreadStatus) => {
      if (!projectPath) return;
      await invoke("reply_comment", { cwd: projectPath, thread, text, status: status ?? null });
      await refresh();
    },
    [projectPath, refresh],
  );

  return { threads, open, reply };
}
