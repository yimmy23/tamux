import { cva } from "class-variance-authority";
import { cn } from "@/lib/classNameUtils";

export { cn };

export type SemanticTone =
  | "default"
  | "accent"
  | "agent"
  | "human"
  | "approval"
  | "reasoning"
  | "mission"
  | "timeline"
  | "success"
  | "warning"
  | "danger";

export const focusRingClassName =
  "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--ring)]";

export const disabledClassName = "disabled:pointer-events-none disabled:opacity-50";

export const fieldClassName = cn(
  "flex w-full rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--input)] px-[var(--space-3)] py-[var(--space-2)] text-[var(--input-foreground)] [font-size:var(--text-sm)] transition-colors duration-100 ease-out",
  "placeholder:text-[var(--text-muted)] hover:border-[var(--border-strong)] hover:bg-[var(--input-hover)]",
  "focus:border-[var(--accent)] focus:bg-[var(--input-hover)]",
  focusRingClassName,
  disabledClassName
);

export const panelSurfaceClassName =
  "rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--card)] text-[var(--card-foreground)] shadow-[var(--shadow-sm)]";

export const popoverSurfaceClassName =
  "rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--popover)] text-[var(--popover-foreground)] shadow-[var(--shadow-lg)]";

export const overlayClassName =
  "fixed inset-0 bg-[var(--overlay)] backdrop-blur-[var(--panel-blur)]";

export const semanticToneVariants = cva("border", {
  variants: {
    tone: {
      default: "border-[var(--glass-border)] bg-[var(--muted)] text-[var(--muted-foreground)]",
      accent: "border-[var(--accent-border)] bg-[var(--accent-soft)] text-[var(--accent)]",
      agent: "border-[var(--agent-border)] bg-[var(--agent-soft)] text-[var(--agent)]",
      human: "border-[var(--human-border)] bg-[var(--human-soft)] text-[var(--human)]",
      approval: "border-[var(--approval-border)] bg-[var(--approval-soft)] text-[var(--approval)]",
      reasoning: "border-[var(--reasoning-border)] bg-[var(--reasoning-soft)] text-[var(--reasoning)]",
      mission: "border-[var(--mission-border)] bg-[var(--mission-soft)] text-[var(--mission)]",
      timeline: "border-[var(--timeline-border)] bg-[var(--timeline-soft)] text-[var(--timeline)]",
      success: "border-[var(--success-border)] bg-[var(--success-soft)] text-[var(--success)]",
      warning: "border-[var(--warning-border)] bg-[var(--warning-soft)] text-[var(--warning)]",
      danger: "border-[var(--danger-border)] bg-[var(--danger-soft)] text-[var(--danger)]",
    },
  },
  defaultVariants: {
    tone: "default",
  },
});
