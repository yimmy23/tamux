import * as React from "react";
import * as TabsPrimitive from "@radix-ui/react-tabs";
import { cn, focusRingClassName } from "./shared";

export const Tabs = TabsPrimitive.Root;

export const TabsList = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.List>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.List>
>(function TabsList({ className, ...props }, ref) {
  return (
    <TabsPrimitive.List
      ref={ref}
      className={cn(
        "inline-flex items-center gap-[var(--space-1)] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--muted)] p-[var(--space-1)]",
        className
      )}
      {...props}
    />
  );
});

export const TabsTrigger = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.Trigger>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.Trigger>
>(function TabsTrigger({ className, ...props }, ref) {
  return (
    <TabsPrimitive.Trigger
      ref={ref}
      className={cn(
        "inline-flex items-center justify-center rounded-[var(--radius-sm)] px-[var(--space-3)] py-[var(--space-2)] [font-size:var(--text-sm)] font-medium text-[var(--text-secondary)] transition-colors duration-100 ease-out data-[state=active]:bg-[var(--card)] data-[state=active]:text-[var(--text-primary)] data-[state=active]:shadow-[var(--shadow-sm)] disabled:pointer-events-none disabled:opacity-50",
        focusRingClassName,
        className
      )}
      {...props}
    />
  );
});

export const TabsContent = React.forwardRef<
  React.ElementRef<typeof TabsPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof TabsPrimitive.Content>
>(function TabsContent({ className, ...props }, ref) {
  return <TabsPrimitive.Content ref={ref} className={cn("mt-[var(--space-3)]", className)} {...props} />;
});
