import { useState } from "react";
import { buildToolReviewPresentation } from "../toolReviewPresentation";
import type { ToolEventGroup } from "./types";
import { getToolDiffPresentation, ToolDiffView } from "./toolDiffPresentation";
import { getToolIconPresentation } from "./toolIconPresentation";
import { toolStatusTone } from "./toolStatusTone";
import {
  getToolFileTarget,
  getToolStructuredFields,
  ToolFileTargetView,
  ToolStructuredValueView,
} from "./toolValuePresentation";

export function ToolEventRow({ group }: { group: ToolEventGroup }) {
  const [collapsed, setCollapsed] = useState(true);
  const statusLabel = group.status.toUpperCase();
  const shortId = (group.toolCallId || group.key).slice(-8);
  const statusTone = toolStatusTone(group.status);
  const toolIcon = getToolIconPresentation(group.toolName, group.toolArguments);
  const toolDiff = group.toolArguments
    ? getToolDiffPresentation(group.toolName, group.toolArguments)
    : null;
  const fileTarget = group.toolArguments
    ? getToolFileTarget(group.toolName, group.toolArguments)
    : null;
  const structuredArgs = group.toolArguments
    ? getToolStructuredFields(group.toolName, group.toolArguments, "arguments")
    : null;
  const structuredArgDetails = fileTarget && structuredArgs
    ? structuredArgs.filter((field) => field.key !== "path")
    : structuredArgs;
  const structuredResult = group.resultContent
    ? getToolStructuredFields(group.toolName, group.resultContent, "result")
    : null;
  const reviewPresentation = buildToolReviewPresentation(group.welesReview);
  const reviewToneStyle = reviewPresentation?.tone === "blocked"
    ? {
      color: "#FFB4B4",
      borderColor: "rgba(255, 107, 107, 0.35)",
      background: "rgba(109, 26, 26, 0.45)",
    }
    : {
      color: "#FFE1A8",
      borderColor: "rgba(255, 184, 77, 0.35)",
      background: "rgba(88, 57, 8, 0.38)",
    };

  return (
    <div style={{ border: "1px solid rgba(255,255,255,0.1)", padding: 8, fontFamily: "var(--font-mono)", whiteSpace: "pre-wrap", wordBreak: "break-word", overflowWrap: "anywhere", display: "flex", flexDirection: "column", gap: 6, borderRadius: "var(--radius-sm)", background: "rgba(255,255,255,0.01)", minWidth: 0, maxWidth: "100%", boxSizing: "border-box" }}>
      <button
        type="button"
        onClick={() => setCollapsed((prev) => !prev)}
        style={{
          border: "none",
          background: "transparent",
          padding: 0,
          color: "var(--text-primary)",
          cursor: "pointer",
          fontFamily: "var(--font-mono)",
          fontSize: "var(--text-sm)",
          display: "flex",
          alignItems: "center",
          width: "100%",
          gap: 8,
          minWidth: 0,
        }}
      >
        <span style={{ color: "#DE600A" }}>{collapsed ? "▸" : "▾"}</span>
        <div style={{ display: "flex", flexDirection: "row", gap: 4, alignItems: "center", justifyContent: "space-between", flex: 1, minWidth: 0 }}>
          <span style={{ display: "flex", alignItems: "center", gap: 6, minWidth: 0, overflow: "hidden", flex: "1 1 auto" }}>
            <span
              aria-label={toolIcon.label}
              title={toolIcon.label}
              style={{
                width: 22,
                height: 22,
                display: "inline-flex",
                alignItems: "center",
                justifyContent: "center",
                border: "1px solid rgba(255,255,255,0.14)",
                borderRadius: 4,
                background: "rgba(255,255,255,0.04)",
                color: "var(--text-muted)",
                fontSize: 14,
                fontWeight: 700,
                letterSpacing: 0,
                lineHeight: 1,
                flexShrink: 0,
              }}
            >
              {toolIcon.glyph}
            </span>
            <span style={{ minWidth: 0, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{group.toolName}</span>
          </span>
          <div style={{ display: "flex", flexDirection: "row", gap: 4, alignItems: "flex-start", fontSize: 8, flexShrink: 0 }}>
            {reviewPresentation && (
              <span
                style={{
                  fontSize: 11,
                  fontWeight: 700,
                  padding: "1px 6px",
                  border: "1px solid",
                  borderRadius: 999,
                  ...reviewToneStyle,
                }}
              >
                {reviewPresentation.badgeLabel}
              </span>
            )}
            <span style={{ color: "var(--text-muted)", fontSize: 11 }}>#{shortId}</span>
            <span
              style={{
                color: statusTone.text,
                border: `1px solid ${statusTone.border}`,
                background: statusTone.background,
                borderRadius: 999,
                padding: "1px 6px",
                fontSize: 11,
                fontWeight: 700,
              }}
            >
              {statusLabel}
            </span>
          </div>
        </div>
      </button>

      {!collapsed && (
        <div style={{ display: "grid", gap: 6, minWidth: 0 }}>
          {reviewPresentation && (
            <div
              style={{
                display: "grid",
                gap: 6,
                padding: 8,
                border: "1px solid",
                borderRadius: "var(--radius-sm)",
                ...reviewToneStyle,
              }}
            >
              <div style={{ display: "flex", flexWrap: "wrap", gap: 6, alignItems: "center" }}>
                <span style={{ fontSize: 12, fontWeight: 700 }}>{reviewPresentation.badgeLabel}</span>
                {reviewPresentation.overrideLabel && (
                  <span style={{ fontSize: 10, border: "1px solid currentColor", borderRadius: 999, padding: "1px 6px" }}>
                    {reviewPresentation.overrideLabel}
                  </span>
                )}
                {reviewPresentation.degradedLabel && (
                  <span style={{ fontSize: 10, border: "1px solid currentColor", borderRadius: 999, padding: "1px 6px" }}>
                    {reviewPresentation.degradedLabel}
                  </span>
                )}
                {reviewPresentation.auditLabel && (
                  <span style={{ fontSize: 10, opacity: 0.85 }}>{reviewPresentation.auditLabel}</span>
                )}
              </div>
              {reviewPresentation.reasonText && (
                <div style={{ fontSize: 12, lineHeight: 1.45 }}>
                  {reviewPresentation.reasonText}
                </div>
              )}
            </div>
          )}

          {fileTarget ? (
            <ToolFileTargetView label="file" path={fileTarget.path} summaryText={group.resultContent} />
          ) : toolDiff ? (
            <ToolDiffView sections={toolDiff} />
          ) : structuredArgDetails ? (
            <ToolStructuredValueView label="args" fields={structuredArgDetails} />
          ) : group.toolArguments ? (
            <div>
              <div style={{ color: "var(--text-muted)", fontSize: 11 }}>args</div>
              <pre style={{ margin: 0, fontFamily: "var(--font-mono)", fontSize: 11, color: "var(--text-primary)", whiteSpace: "pre-wrap", wordBreak: "break-word", overflowWrap: "anywhere", overflow: "auto", border: "1px solid rgba(255,255,255,0.08)", background: "rgba(255,255,255,0.04)", padding: 8, borderRadius: "var(--radius-sm)" }}>
                {(() => {
                  try {
                    return JSON.stringify(JSON.parse(group.toolArguments), null, 2);
                  } catch {
                    return group.toolArguments;
                  }
                })()}
              </pre>
            </div>
          ) : null}

          {!fileTarget && structuredResult ? (
            <ToolStructuredValueView label="result" fields={structuredResult} />
          ) : !fileTarget && group.resultContent ? (
            <div>
              <div style={{ color: "var(--text-muted)", fontSize: 11 }}>result</div>
              <div style={{ fontSize: 12, lineHeight: 1.45, border: "1px solid rgba(255,255,255,0.08)", background: "rgba(255,255,255,0.04)", padding: 8, borderRadius: "var(--radius-sm)" }}>
                {group.resultContent}
              </div>
            </div>
          ) : null}

          <div style={{ display: "flex", justifyContent: "flex-end" }}>
            <button
              type="button"
              onClick={() => setCollapsed(true)}
              style={{
                border: "1px solid rgba(255,255,255,0.12)",
                background: "rgba(255,255,255,0.02)",
                color: "var(--text-muted)",
                cursor: "pointer",
                padding: "4px 8px",
                borderRadius: "var(--radius-sm)",
                fontSize: 11,
                fontFamily: "var(--font-mono)",
              }}
            >
              Collapse
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
