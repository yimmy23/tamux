import { useEffect } from "react";
import { useAuditStore } from "../../lib/auditStore";
import { AuditHeader } from "./AuditHeader";
import { AuditList } from "./AuditList";
import { EscalationBanner } from "./EscalationBanner";
import { cn, overlayClassName, panelSurfaceClassName } from "../ui/shared";

/**
 * Main slide-over audit panel, 440px wide from right.
 * Same layout pattern as NotificationPanel.
 * Opens via Ctrl+Shift+A or programmatic toggle.
 */
export function AuditPanel() {
  const isOpen = useAuditStore((s) => s.isOpen);
  const toggle = useAuditStore((s) => s.togglePanel);
  const currentEscalation = useAuditStore((s) => s.currentEscalation);
  const setEscalation = useAuditStore((s) => s.setEscalation);

  // Escape key closes panel
  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        toggle();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, toggle]);

  if (!isOpen) return null;

  const handleCancelEscalation = () => {
    // Send cancel to daemon via IPC bridge
    const amux = (window as unknown as Record<string, unknown>).tamux ??
      (window as unknown as Record<string, unknown>).amux;
    if (amux && typeof (amux as Record<string, unknown>).cancelEscalation === "function") {
      void (amux as Record<string, (...args: unknown[]) => Promise<void>>).cancelEscalation(
        currentEscalation?.threadId,
      );
    }
    setEscalation(null);
  };

  return (
    <div
      onClick={toggle}
      className={overlayClassName}
      style={{
        zIndex: 900,
        display: "flex",
        justifyContent: "flex-end",
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className={cn(panelSurfaceClassName, "rounded-none border-y-0 border-r-0 shadow-none")}
        style={{
          width: 440,
          maxWidth: "90vw",
          height: "100%",
          display: "flex",
          flexDirection: "column",
        }}
      >
        {currentEscalation && (
          <EscalationBanner
            escalation={currentEscalation}
            onCancel={handleCancelEscalation}
          />
        )}

        <AuditHeader onClose={toggle} />
        <AuditList />
      </div>
    </div>
  );
}
