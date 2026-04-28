import type { CSSProperties } from "react";
import { getBridge } from "@/lib/bridge";
import { useAgentMissionStore } from "../lib/agentMissionStore";

type AgentApprovalOverlayProps = {
  style?: CSSProperties;
  className?: string;
};

type ApprovalDecision = "approved-once" | "approved-session" | "denied";

export function AgentApprovalOverlay({ style, className }: AgentApprovalOverlayProps = {}) {
  const approval = useAgentMissionStore((s) =>
    s.approvals.find((entry) => entry.status === "pending" && entry.handledAt === null)
  );
  const resolveApproval = useAgentMissionStore((s) => s.resolveApproval);

  async function handleDecision(status: ApprovalDecision) {
    if (!approval) return;

    const zorai = getBridge();
    if (zorai?.resolveManagedApproval) {
      const decision =
        status === "approved-session" ? "approve-session" : status === "denied" ? "deny" : "approve-once";
      await zorai.resolveManagedApproval(approval.paneId, approval.id, decision);
    }

    resolveApproval(approval.id, status);
  }

  if (!approval) return null;

  const overlayClassName = ["zorai-approval-overlay", className ?? ""].filter(Boolean).join(" ");
  const riskClassName = ["zorai-approval-risk", `zorai-approval-risk--${approval.riskLevel}`].join(" ");

  return (
    <div style={style} className={overlayClassName} role="presentation">
      <section className="zorai-approval-dialog" role="dialog" aria-modal="true" aria-labelledby="zorai-approval-title">
        <header className="zorai-approval-header">
          <div>
            <div className="zorai-approval-kicker">
              <span />
              Approval Required
            </div>
            <h2 id="zorai-approval-title">High-impact command intercepted</h2>
          </div>
          <div className={riskClassName}>{approval.riskLevel}</div>
        </header>

        <div className="zorai-approval-body">
          <div className="zorai-approval-section">
            <div className="zorai-approval-label">Command</div>
            <pre className="zorai-approval-command">{approval.command}</pre>
          </div>

          <div className="zorai-approval-grid">
            <InfoCard label="Blast Radius" value={approval.blastRadius} />
            <InfoCard label="Scope" value={approval.sessionId ?? approval.paneId} />
          </div>

          <div className="zorai-approval-section">
            <div className="zorai-approval-label">Risk Factors</div>
            <div className="zorai-approval-chip-list">
              {approval.reasons.length === 0 ? <span>No specific factors reported.</span> : approval.reasons.map((reason) => (
                <span key={reason}>{reason}</span>
              ))}
            </div>
          </div>
        </div>

        <footer className="zorai-approval-actions">
          <button type="button" className="zorai-approval-button zorai-approval-button--deny" onClick={() => void handleDecision("denied")}>
            Deny
          </button>
          <button type="button" className="zorai-approval-button" onClick={() => void handleDecision("approved-once")}>
            Allow Once
          </button>
          <button type="button" className="zorai-approval-button zorai-approval-button--primary" onClick={() => void handleDecision("approved-session")}>
            Allow For Session
          </button>
        </footer>
      </section>
    </div>
  );
}

function InfoCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="zorai-approval-info">
      <div className="zorai-approval-label">{label}</div>
      <strong>{value}</strong>
    </div>
  );
}
