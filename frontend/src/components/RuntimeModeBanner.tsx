import { getRuntimeModeDescription } from "@/lib/runtimeMode";

export function RuntimeModeBanner() {
  const runtimeMode = getRuntimeModeDescription();

  if (!runtimeMode) {
    return null;
  }

  return (
    <div
      style={{
        position: "fixed",
        top: 12,
        left: 12,
        right: 12,
        zIndex: 5000,
        border: "1px solid rgba(255, 196, 107, 0.4)",
        background: "linear-gradient(180deg, rgba(64, 39, 10, 0.96), rgba(31, 20, 7, 0.94))",
        boxShadow: "0 18px 48px rgba(0, 0, 0, 0.35)",
        color: "#ffe1b0",
        padding: "12px 14px",
        display: "grid",
        gap: 4,
      }}
    >
      <strong style={{ fontSize: 13, letterSpacing: "0.04em", textTransform: "uppercase" }}>
        {runtimeMode.title}
      </strong>
      <span style={{ fontSize: 14, fontWeight: 600 }}>{runtimeMode.summary}</span>
      <span style={{ fontSize: 13, lineHeight: 1.45, color: "rgba(255, 232, 194, 0.92)" }}>
        {runtimeMode.detail}
      </span>
    </div>
  );
}
