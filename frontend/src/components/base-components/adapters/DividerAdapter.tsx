import type React from "react";
import { Separator } from "../../ui/Separator";
import { cn } from "../../ui/shared";
import { renderEditableWrapper, splitTypedViewProps } from "../propUtils";
import type { ViewProps } from "../shared";

export function DividerAdapter(props: ViewProps) {
  const { style, className, children, visible, hidden, builderMeta, componentProps } =
    splitTypedViewProps<{ className?: string; style?: React.CSSProperties }>(props);

  return renderEditableWrapper({
    style,
    className,
    children,
    visible,
    hidden,
    builderMeta,
    content: (
      <Separator
        className={cn("bg-[var(--border-subtle)]", className, componentProps.className)}
        style={componentProps.style}
      />
    ),
  });
}
