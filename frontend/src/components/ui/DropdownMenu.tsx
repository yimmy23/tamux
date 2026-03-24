import * as React from "react";
import * as DropdownMenuPrimitive from "@radix-ui/react-dropdown-menu";
import { cn, popoverSurfaceClassName } from "./shared";

export const DropdownMenu = DropdownMenuPrimitive.Root;
export const DropdownMenuTrigger = DropdownMenuPrimitive.Trigger;
export const DropdownMenuGroup = DropdownMenuPrimitive.Group;
export const DropdownMenuPortal = DropdownMenuPrimitive.Portal;
export const DropdownMenuSub = DropdownMenuPrimitive.Sub;
export const DropdownMenuRadioGroup = DropdownMenuPrimitive.RadioGroup;

export const DropdownMenuSubTrigger = React.forwardRef<
  React.ElementRef<typeof DropdownMenuPrimitive.SubTrigger>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.SubTrigger> & {
    inset?: boolean;
  }
>(function DropdownMenuSubTrigger({ className, inset, children, ...props }, ref) {
  return (
    <DropdownMenuPrimitive.SubTrigger
      ref={ref}
      className={cn(
        "flex cursor-default select-none items-center rounded-[var(--radius-sm)] px-[var(--space-3)] py-[var(--space-2)] [font-size:var(--text-sm)] text-[var(--text-primary)] outline-none transition-colors focus:bg-[var(--muted)] data-[state=open]:bg-[var(--muted)]",
        inset && "pl-[var(--space-6)]",
        className
      )}
      {...props}
    >
      {children}
      <span className="ml-auto text-[var(--text-muted)]">›</span>
    </DropdownMenuPrimitive.SubTrigger>
  );
});

export const DropdownMenuSubContent = React.forwardRef<
  React.ElementRef<typeof DropdownMenuPrimitive.SubContent>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.SubContent>
>(function DropdownMenuSubContent({ className, ...props }, ref) {
  return (
    <DropdownMenuPrimitive.SubContent
      ref={ref}
      className={cn(
        popoverSurfaceClassName,
        "z-[700] min-w-[12rem] overflow-hidden p-[var(--space-1)] opacity-0 transition duration-100 ease-out data-[state=open]:opacity-100",
        className
      )}
      {...props}
    />
  );
});

export const DropdownMenuContent = React.forwardRef<
  React.ElementRef<typeof DropdownMenuPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Content>
>(function DropdownMenuContent({ className, sideOffset = 6, ...props }, ref) {
  return (
    <DropdownMenuPrimitive.Portal>
      <DropdownMenuPrimitive.Content
        ref={ref}
        sideOffset={sideOffset}
        className={cn(
          popoverSurfaceClassName,
          "z-[700] min-w-[12rem] overflow-hidden p-[var(--space-1)] opacity-0 transition duration-100 ease-out data-[state=open]:opacity-100",
          className
        )}
        {...props}
      />
    </DropdownMenuPrimitive.Portal>
  );
});

export const DropdownMenuItem = React.forwardRef<
  React.ElementRef<typeof DropdownMenuPrimitive.Item>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Item> & {
    inset?: boolean;
  }
>(function DropdownMenuItem({ className, inset, ...props }, ref) {
  return (
    <DropdownMenuPrimitive.Item
      ref={ref}
      className={cn(
        "relative flex cursor-default select-none items-center gap-[var(--space-2)] rounded-[var(--radius-sm)] px-[var(--space-3)] py-[var(--space-2)] [font-size:var(--text-sm)] text-[var(--text-primary)] outline-none transition-colors focus:bg-[var(--muted)] data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
        inset && "pl-[var(--space-6)]",
        className
      )}
      {...props}
    />
  );
});

export const DropdownMenuCheckboxItem = React.forwardRef<
  React.ElementRef<typeof DropdownMenuPrimitive.CheckboxItem>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.CheckboxItem>
>(function DropdownMenuCheckboxItem({ className, children, checked, ...props }, ref) {
  return (
    <DropdownMenuPrimitive.CheckboxItem
      ref={ref}
      className={cn(
        "relative flex cursor-default select-none items-center rounded-[var(--radius-sm)] py-[var(--space-2)] pl-[var(--space-6)] pr-[var(--space-3)] [font-size:var(--text-sm)] text-[var(--text-primary)] outline-none transition-colors focus:bg-[var(--muted)] data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
        className
      )}
      checked={checked}
      {...props}
    >
      <span className="absolute left-[var(--space-2)] flex h-[var(--space-3)] w-[var(--space-3)] items-center justify-center text-[var(--accent)]">
        <DropdownMenuPrimitive.ItemIndicator>✓</DropdownMenuPrimitive.ItemIndicator>
      </span>
      {children}
    </DropdownMenuPrimitive.CheckboxItem>
  );
});

export const DropdownMenuRadioItem = React.forwardRef<
  React.ElementRef<typeof DropdownMenuPrimitive.RadioItem>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.RadioItem>
>(function DropdownMenuRadioItem({ className, children, ...props }, ref) {
  return (
    <DropdownMenuPrimitive.RadioItem
      ref={ref}
      className={cn(
        "relative flex cursor-default select-none items-center rounded-[var(--radius-sm)] py-[var(--space-2)] pl-[var(--space-6)] pr-[var(--space-3)] [font-size:var(--text-sm)] text-[var(--text-primary)] outline-none transition-colors focus:bg-[var(--muted)] data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
        className
      )}
      {...props}
    >
      <span className="absolute left-[var(--space-2)] flex h-[var(--space-3)] w-[var(--space-3)] items-center justify-center text-[var(--accent)]">
        <DropdownMenuPrimitive.ItemIndicator>●</DropdownMenuPrimitive.ItemIndicator>
      </span>
      {children}
    </DropdownMenuPrimitive.RadioItem>
  );
});

export const DropdownMenuLabel = React.forwardRef<
  React.ElementRef<typeof DropdownMenuPrimitive.Label>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Label> & {
    inset?: boolean;
  }
>(function DropdownMenuLabel({ className, inset, ...props }, ref) {
  return (
    <DropdownMenuPrimitive.Label
      ref={ref}
      className={cn(
        "px-[var(--space-3)] py-[var(--space-2)] [font-size:var(--text-xs)] font-semibold uppercase tracking-[0.08em] text-[var(--text-muted)]",
        inset && "pl-[var(--space-6)]",
        className
      )}
      {...props}
    />
  );
});

export function DropdownMenuSeparator({
  className,
  ...props
}: React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Separator>) {
  return (
    <DropdownMenuPrimitive.Separator
      className={cn("-mx-[var(--space-1)] my-[var(--space-1)] h-px bg-[var(--border-subtle)]", className)}
      {...props}
    />
  );
}

export function DropdownMenuShortcut({
  className,
  ...props
}: React.HTMLAttributes<HTMLSpanElement>) {
  return (
    <span
      className={cn("ml-auto text-[var(--text-xs)] tracking-[0.08em] text-[var(--text-muted)]", className)}
      {...props}
    />
  );
}
