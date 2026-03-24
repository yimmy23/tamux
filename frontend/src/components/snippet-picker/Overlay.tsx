import type { ReactNode } from "react";
import { cn, overlayClassName } from "../ui";

export function Overlay({ onClose, children }: { onClose: () => void; children: ReactNode }) {
  return (
    <div
      onClick={onClose}
      className={cn(overlayClassName, "fixed inset-0 z-[960] flex items-center justify-center p-[var(--space-4)]")}
    >
      <div onClick={(event) => event.stopPropagation()}>{children}</div>
    </div>
  );
}
