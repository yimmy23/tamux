import * as React from "react";
import { cn, fieldClassName } from "./shared";

export const Input = React.forwardRef<HTMLInputElement, React.ComponentPropsWithoutRef<"input">>(
  function Input({ className, type = "text", ...props }, ref) {
    return <input ref={ref} type={type} className={cn(fieldClassName, className)} {...props} />;
  }
);
