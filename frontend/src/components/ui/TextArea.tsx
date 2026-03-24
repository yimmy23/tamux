import * as React from "react";
import { cn, fieldClassName } from "./shared";

export const TextArea = React.forwardRef<
  HTMLTextAreaElement,
  React.ComponentPropsWithoutRef<"textarea">
>(function TextArea({ className, rows = 4, ...props }, ref) {
  return (
    <textarea
      ref={ref}
      rows={rows}
      className={cn(fieldClassName, "min-h-[8rem] resize-vertical", className)}
      {...props}
    />
  );
});
