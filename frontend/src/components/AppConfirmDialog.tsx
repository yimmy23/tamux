import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Separator,
} from "./ui";

type AppConfirmDialogProps = {
  open: boolean;
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  tone?: "danger" | "warning" | "neutral";
  onConfirm: () => void;
  onCancel: () => void;
};

const confirmToneClassName: Record<NonNullable<AppConfirmDialogProps["tone"]>, string> = {
  danger: "",
  warning:
    "border-[var(--warning-border)] bg-[var(--warning-soft)] text-[var(--warning)] hover:border-[var(--warning)] hover:text-[var(--warning)]",
  neutral:
    "border-[var(--accent-border)] bg-[var(--accent-soft)] text-[var(--accent)] hover:border-[var(--accent)] hover:text-[var(--accent-hover)]",
};

export function AppConfirmDialog({
  open,
  title,
  message,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  tone = "danger",
  onConfirm,
  onCancel,
}: AppConfirmDialogProps) {
  return (
    <Dialog open={open} onOpenChange={(nextOpen) => !nextOpen && onCancel()}>
      <DialogContent className="w-[min(calc(100vw-var(--space-6)),32.5rem)] gap-[var(--space-4)] p-0">
        <DialogHeader className="gap-[var(--space-2)] p-[var(--space-5)] pb-[var(--space-4)] pr-[calc(var(--space-5)+var(--space-6))]">
          <DialogTitle className="text-[var(--text-lg)] font-bold">{title}</DialogTitle>
          <DialogDescription className="leading-6 text-[var(--text-sm)]">{message}</DialogDescription>
        </DialogHeader>
        <Separator />
        <DialogFooter className="bg-[var(--bg-secondary)] px-[var(--space-5)] py-[var(--space-4)] sm:flex-row sm:justify-end">
          <Button variant="outline" onClick={onCancel}>
            {cancelLabel}
          </Button>
          <Button
            variant={tone === "danger" ? "destructive" : "outline"}
            onClick={onConfirm}
            className={confirmToneClassName[tone]}
          >
            {confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
