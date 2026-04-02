import { useSettingsStore } from "../settingsStore";
import type { RiskAssessment, RiskLevel, SecurityLevel } from "./types";

function shouldRequireApproval(
  securityLevel: SecurityLevel,
  riskLevel: RiskLevel,
  reasons: string[],
): boolean {
  if (securityLevel === "yolo") return false;
  if (securityLevel === "highest") return true;
  if (securityLevel === "lowest") return riskLevel === "critical";
  return reasons.length > 0;
}

export function assessCommandRisk(
  command: string,
  securityLevel?: SecurityLevel,
): RiskAssessment {
  const normalized = command.trim().toLowerCase();
  const effectiveSecurityLevel = securityLevel ?? useSettingsStore.getState().settings.securityLevel ?? "moderate";
  if (!normalized) {
    return {
      requiresApproval: false,
      riskLevel: "medium",
      reasons: [],
      blastRadius: "none",
    };
  }

  const reasons: string[] = [];
  let riskLevel: RiskLevel = "medium";
  let blastRadius = "local pane";

  const checks: Array<{ test: RegExp; level: RiskLevel; reason: string; radius: string }> = [
    { test: /(^|\s)rm\s+-rf\s+(\/|~|\.\.?)(\s|$)/, level: "critical", reason: "destructive recursive delete", radius: "filesystem-wide" },
    { test: /(^|\s)(mkfs|fdisk|parted|dd)\b/, level: "critical", reason: "disk or block-device mutation", radius: "disk-level" },
    { test: /(^|\s)(shutdown|reboot|halt|poweroff)\b/, level: "critical", reason: "host power-state change", radius: "host-wide" },
    { test: /(^|\s)git\s+push\b.*(--force|-f)(\s|$)/, level: "high", reason: "force push rewrites remote history", radius: "remote repository" },
    { test: /(^|\s)git\s+reset\s+--hard\b/, level: "high", reason: "hard reset discards local changes", radius: "workspace" },
    { test: /(^|\s)(chmod|chown)\b.*-r/, level: "high", reason: "recursive permission or ownership change", radius: "workspace or subtree" },
    { test: /curl\b[^|\n]*\|\s*(sh|bash|zsh)\b/, level: "high", reason: "executes remote script directly", radius: "remote code execution" },
    { test: /(^|\s)(docker\s+system\s+prune|kubectl\s+delete|terraform\s+destroy)\b/, level: "high", reason: "infrastructure-destructive operation", radius: "container or cluster resources" },
    { test: /(^|\s)(systemctl|service)\s+(stop|restart|disable)\b/, level: "high", reason: "service lifecycle mutation", radius: "host services" },
    { test: /(^|\s)npm\s+publish\b|(^|\s)cargo\s+publish\b/, level: "high", reason: "publishes external artifact", radius: "package registry" },
    { test: /(^|\s)(remove-item|ri)\b[^\n]*\b(-recurse|-r)\b/, level: "high", reason: "recursive file deletion on Windows", radius: "workspace or subtree" },
    { test: /(^|\s)(rd|rmdir)\s+[^\n]*\s+\/s\b/, level: "high", reason: "recursive directory delete via cmd.exe", radius: "workspace or subtree" },
    { test: /(^|\s)(del|erase)\s+[^\n]*\s+\/s\b/, level: "high", reason: "recursive file delete via cmd.exe", radius: "workspace or subtree" },
    { test: /(invoke-webrequest|iwr)\b[^|\n]*\|\s*(iex|invoke-expression)\b/, level: "high", reason: "downloads and executes remote PowerShell content", radius: "remote code execution" },
    { test: /(^|\s)(stop-service|restart-service|set-service)\b/, level: "high", reason: "mutates Windows service lifecycle", radius: "host services" },
    { test: /(^|\s)(format|diskpart)\b/, level: "critical", reason: "disk or volume mutation on Windows", radius: "disk-level" },
  ];

  for (const check of checks) {
    if (!check.test.test(normalized)) continue;
    reasons.push(check.reason);
    blastRadius = check.radius;
    if (check.level === "critical" || riskLevel === "medium") {
      riskLevel = check.level;
    }
  }

  if (effectiveSecurityLevel === "highest" && reasons.length === 0) {
    reasons.push("strict policy requires approval for every managed command");
  }

  return {
    requiresApproval: shouldRequireApproval(effectiveSecurityLevel, riskLevel, reasons),
    riskLevel,
    reasons,
    blastRadius,
  };
}
