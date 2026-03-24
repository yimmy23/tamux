import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn, semanticToneVariants } from "./shared";

export const badgeVariants = cva(
  "inline-flex items-center gap-[var(--space-1)] rounded-[var(--radius-full)] px-[var(--space-2)] py-[var(--space-1)] [font-size:var(--text-xs)] font-medium whitespace-nowrap",
  {
    variants: {
      variant: {
        default: semanticToneVariants({ tone: "default" }),
        accent: semanticToneVariants({ tone: "accent" }),
        agent: semanticToneVariants({ tone: "agent" }),
        human: semanticToneVariants({ tone: "human" }),
        approval: semanticToneVariants({ tone: "approval" }),
        reasoning: semanticToneVariants({ tone: "reasoning" }),
        mission: semanticToneVariants({ tone: "mission" }),
        timeline: semanticToneVariants({ tone: "timeline" }),
        success: semanticToneVariants({ tone: "success" }),
        warning: semanticToneVariants({ tone: "warning" }),
        danger: semanticToneVariants({ tone: "danger" }),
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
);

export interface BadgeProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof badgeVariants> {}

export function Badge({ className, variant, ...props }: BadgeProps) {
  return <div className={cn(badgeVariants({ variant }), className)} {...props} />;
}
