import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { cn, disabledClassName, focusRingClassName } from "./shared";

export const buttonVariants = cva(
  cn(
    "inline-flex items-center justify-center gap-[var(--space-2)] whitespace-nowrap rounded-[var(--radius-md)] border px-[var(--space-3)] py-[var(--space-2)] [font-size:var(--text-sm)] font-medium transition-colors duration-100 ease-out select-none",
    focusRingClassName,
    disabledClassName
  ),
  {
    variants: {
      variant: {
        primary:
          "border-[var(--accent-border)] bg-[var(--accent-soft)] text-[var(--accent)] hover:border-[var(--accent)] hover:text-[var(--accent-hover)]",
        secondary:
          "border-[var(--border)] bg-[var(--secondary)] text-[var(--secondary-foreground)] hover:border-[var(--border-strong)] hover:bg-[var(--secondary-hover)]",
        ghost:
          "border-transparent bg-transparent text-[var(--text-secondary)] hover:bg-[var(--muted)] hover:text-[var(--text-primary)]",
        outline:
          "border-[var(--border)] bg-transparent text-[var(--text-primary)] hover:border-[var(--accent-border)] hover:text-[var(--accent)]",
        destructive:
          "border-[var(--danger-border)] bg-[var(--danger-soft)] text-[var(--danger)] hover:border-[var(--danger)] hover:text-[var(--danger-hover)]",
        agent:
          "border-[var(--agent-border)] bg-[var(--agent-soft)] text-[var(--agent)] hover:border-[var(--agent)] hover:text-[var(--agent-hover)]",
        human:
          "border-[var(--human-border)] bg-[var(--human-soft)] text-[var(--human)] hover:border-[var(--human)] hover:text-[var(--human-hover)]",
      },
      size: {
        sm: "px-[var(--space-2)] py-[var(--space-1)] [font-size:var(--text-xs)]",
        default: "px-[var(--space-3)] py-[var(--space-2)]",
        lg: "px-[var(--space-4)] py-[var(--space-3)] [font-size:var(--text-base)]",
        icon: "h-[var(--space-8)] w-[var(--space-8)] p-0",
      },
    },
    defaultVariants: {
      variant: "primary",
      size: "default",
    },
  }
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { className, variant, size, asChild = false, type = "button", ...props },
  ref
) {
  const Comp = asChild ? Slot : "button";

  return (
    <Comp
      className={cn(buttonVariants({ variant, size }), className)}
      ref={ref}
      type={asChild ? undefined : type}
      {...props}
    />
  );
});
