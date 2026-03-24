import { useEffect, useRef, useState } from "react";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Separator,
} from "./ui";

type AppPromptDialogProps = {
  open: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  placeholder?: string;
  defaultValue?: string;
  tone?: "danger" | "warning" | "neutral";
  onConfirm: (value: string) => void;
  onCancel: () => void;
};

const confirmToneClassName: Record<NonNullable<AppPromptDialogProps["tone"]>, string> = {
  danger: "",
  warning:
    "border-[var(--warning-border)] bg-[var(--warning-soft)] text-[var(--warning)] hover:border-[var(--warning)] hover:text-[var(--warning)]",
  neutral:
    "border-[var(--accent-border)] bg-[var(--accent-soft)] text-[var(--accent)] hover:border-[var(--accent)] hover:text-[var(--accent-hover)]",
};

export function AppPromptDialog({
  open,
  title,
  message,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  placeholder = "",
  defaultValue = "",
  tone = "neutral",
  onConfirm,
  onCancel,
}: AppPromptDialogProps) {
  const [value, setValue] = useState(defaultValue);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setValue(defaultValue);
    }
  }, [defaultValue, open]);

  return (
    <Dialog open={open} onOpenChange={(nextOpen) => !nextOpen && onCancel()}>
      <DialogContent
        className="w-[min(calc(100vw-var(--space-6)),35rem)] gap-[var(--space-4)] p-0"
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          inputRef.current?.focus();
        }}
      >
        <form
          onSubmit={(event) => {
            event.preventDefault();
            onConfirm(value);
          }}
        >
          <DialogHeader className="gap-[var(--space-3)] p-[var(--space-5)] pb-[var(--space-4)] pr-[calc(var(--space-5)+var(--space-6))]">
            <DialogTitle className="text-[var(--text-lg)] font-bold">{title}</DialogTitle>
            <DialogDescription className="leading-6 text-[var(--text-sm)]">{message}</DialogDescription>
            <Input
              ref={inputRef}
              value={value}
              placeholder={placeholder}
              onChange={(event) => setValue(event.target.value)}
            />
          </DialogHeader>
          <Separator />
          <DialogFooter className="bg-[var(--bg-secondary)] px-[var(--space-5)] py-[var(--space-4)] sm:flex-row sm:justify-end">
            <Button variant="outline" onClick={onCancel}>
              {cancelLabel}
            </Button>
            <Button
              type="submit"
              variant={tone === "danger" ? "destructive" : "outline"}
              className={confirmToneClassName[tone]}
            >
              {confirmLabel}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
