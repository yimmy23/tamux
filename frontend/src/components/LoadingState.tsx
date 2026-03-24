/**
 * Animated loading indicators with three variants:
 * spinner, skeleton, and progress bar.
 * Pure CSS animations matching the design system.
 */

interface SpinnerProps {
  variant?: "spinner";
  size?: number;
  label?: string;
}

interface SkeletonProps {
  variant: "skeleton";
  width?: string | number;
  height?: string | number;
  lines?: number;
}

interface ProgressProps {
  variant: "progress";
  value: number; // 0-100
  label?: string;
}

type LoadingStateProps = SpinnerProps | SkeletonProps | ProgressProps;

export function LoadingState(props: LoadingStateProps) {
  const variant = props.variant ?? "spinner";

  if (variant === "skeleton") {
    const { width = "100%", height = 14, lines = 3 } = props as SkeletonProps;
    return (
      <div style={{ display: "grid", gap: 8 }}>
        {Array.from({ length: lines }, (_, i) => (
          <div
            key={i}
            style={{
              width: i === lines - 1 ? "60%" : width,
              height,
              borderRadius: "var(--radius-md)",
              background: "var(--bg-secondary)",
              backgroundSize: "200% 100%",
              animation: "shimmer 1.5s infinite ease-in-out",
            }}
          />
        ))}
      </div>
    );
  }

  if (variant === "progress") {
    const { value, label } = props as ProgressProps;
    const clamped = Math.max(0, Math.min(100, value));
    return (
      <div style={{ display: "grid", gap: 6 }}>
        {label && (
          <div style={{ display: "flex", justifyContent: "space-between", fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>
            <span>{label}</span>
            <span>{Math.round(clamped)}%</span>
          </div>
        )}
        <div
          style={{
            height: 6,
            borderRadius: "var(--radius-full)",
            background: "var(--bg-tertiary)",
            overflow: "hidden",
          }}
        >
          <div
            style={{
              height: "100%",
              width: `${clamped}%`,
              borderRadius: "var(--radius-full)",
              background: "var(--accent)",
              boxShadow: "none",
              transition: "width 0.3s ease",
            }}
          />
        </div>
      </div>
    );
  }

  // Default: spinner
  const { size = 20, label } = props as SpinnerProps;
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
      <div
        style={{
          width: size,
          height: size,
          border: "2px solid var(--bg-tertiary)",
          borderTopColor: "var(--accent)",
          borderRadius: "50%",
          animation: "spin 0.7s linear infinite",
          flexShrink: 0,
        }}
      />
      {label && <span style={{ fontSize: "var(--text-xs)", color: "var(--text-secondary)" }}>{label}</span>}
    </div>
  );
}
