import * as React from "react";
import * as SelectPrimitive from "@radix-ui/react-select";
import { cn, fieldClassName, popoverSurfaceClassName } from "./shared";

export const Select = SelectPrimitive.Root;
export const SelectGroup = SelectPrimitive.Group;
export const SelectValue = SelectPrimitive.Value;

export const SelectTrigger = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Trigger>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Trigger>
>(function SelectTrigger({ className, children, ...props }, ref) {
  return (
    <SelectPrimitive.Trigger
      ref={ref}
      className={cn(
        fieldClassName,
        "items-center justify-between gap-[var(--space-2)] [&>span]:truncate",
        className
      )}
      {...props}
    >
      {children}
      <SelectPrimitive.Icon className="text-[var(--text-muted)]">▾</SelectPrimitive.Icon>
    </SelectPrimitive.Trigger>
  );
});

export const SelectContent = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Content>
>(function SelectContent({ className, children, position = "popper", ...props }, ref) {
  return (
    <SelectPrimitive.Portal>
      <SelectPrimitive.Content
        ref={ref}
        position={position}
        className={cn(
          popoverSurfaceClassName,
          "z-[700] max-h-[18rem] min-w-[8rem] overflow-hidden opacity-0 transition duration-100 ease-out data-[state=open]:opacity-100",
          position === "popper" && "translate-y-[2px]",
          className
        )}
        {...props}
      >
        <SelectPrimitive.ScrollUpButton className="flex cursor-default items-center justify-center py-[var(--space-1)] text-[var(--text-muted)]">
          ▴
        </SelectPrimitive.ScrollUpButton>
        <SelectPrimitive.Viewport className="p-[var(--space-1)]">{children}</SelectPrimitive.Viewport>
        <SelectPrimitive.ScrollDownButton className="flex cursor-default items-center justify-center py-[var(--space-1)] text-[var(--text-muted)]">
          ▾
        </SelectPrimitive.ScrollDownButton>
      </SelectPrimitive.Content>
    </SelectPrimitive.Portal>
  );
});

export const SelectLabel = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Label>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Label>
>(function SelectLabel({ className, ...props }, ref) {
  return (
    <SelectPrimitive.Label
      ref={ref}
      className={cn(
        "px-[var(--space-3)] py-[var(--space-2)] [font-size:var(--text-xs)] font-semibold uppercase tracking-[0.08em] text-[var(--text-muted)]",
        className
      )}
      {...props}
    />
  );
});

export const SelectItem = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Item>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Item>
>(function SelectItem({ className, children, ...props }, ref) {
  return (
    <SelectPrimitive.Item
      ref={ref}
      className={cn(
        "relative flex w-full cursor-default select-none items-center rounded-[var(--radius-sm)] py-[var(--space-2)] pl-[var(--space-6)] pr-[var(--space-3)] [font-size:var(--text-sm)] text-[var(--text-primary)] outline-none transition-colors focus:bg-[var(--muted)] data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
        className
      )}
      {...props}
    >
      <span className="absolute left-[var(--space-2)] flex h-[var(--space-3)] w-[var(--space-3)] items-center justify-center text-[var(--accent)]">
        <SelectPrimitive.ItemIndicator>✓</SelectPrimitive.ItemIndicator>
      </span>
      <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
    </SelectPrimitive.Item>
  );
});

export function SelectSeparator({
  className,
  ...props
}: React.ComponentPropsWithoutRef<typeof SelectPrimitive.Separator>) {
  return (
    <SelectPrimitive.Separator
      className={cn("-mx-[var(--space-1)] my-[var(--space-1)] h-px bg-[var(--border-subtle)]", className)}
      {...props}
    />
  );
}
