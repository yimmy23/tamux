import { useMemo, useState, type ReactNode } from "react";
import type { WorkContextEntry } from "@/lib/agentWorkContext";
import {
  ThreadFilePreviewContext,
  type ThreadFilePreviewContextValue,
  type ThreadFilePreviewTarget,
} from "./ThreadFilePreviewContext";

export function ThreadFilePreviewProvider({ children }: { children: ReactNode }) {
  const [previewTarget, setPreviewTarget] = useState<ThreadFilePreviewTarget | null>(null);
  const value = useMemo<ThreadFilePreviewContextValue>(() => ({
    previewTarget,
    openThreadFilePreview: (entry: WorkContextEntry) => setPreviewTarget({ entry }),
    closeThreadFilePreview: () => setPreviewTarget(null),
  }), [previewTarget]);

  return (
    <ThreadFilePreviewContext.Provider value={value}>
      {children}
    </ThreadFilePreviewContext.Provider>
  );
}
