import { useEffect, useState } from "react";
import { formatKeyboardEvent, useKeybindStore } from "../../lib/keybindStore";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, Input } from "../ui";

export function KeyboardTab() {
  const bindings = useKeybindStore((s) => s.bindings);
  const setBinding = useKeybindStore((s) => s.setBinding);
  const resetBindings = useKeybindStore((s) => s.resetBindings);
  const [query, setQuery] = useState("");
  const [recordingAction, setRecordingAction] = useState<string | null>(null);
  const [warning, setWarning] = useState<string | null>(null);

  useEffect(() => {
    if (!recordingAction) return;

    const onKeyDown = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();

      if (event.key === "Escape") {
        setRecordingAction(null);
        return;
      }

      const combo = formatKeyboardEvent(event);
      if (!combo || combo === "Ctrl" || combo === "Alt" || combo === "Shift") {
        return;
      }

      const conflicting = bindings.find((binding) => binding.combo === combo && binding.action !== recordingAction);
      if (conflicting) {
        setWarning(`${combo} is already assigned to ${conflicting.description}`);
        return;
      }

      setBinding(recordingAction as never, combo);
      setWarning(null);
      setRecordingAction(null);
    };

    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [bindings, recordingAction, setBinding]);

  const filteredShortcuts = query.trim()
    ? bindings.filter((binding) => `${binding.combo} ${binding.description}`.toLowerCase().includes(query.toLowerCase()))
    : bindings;

  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-center gap-[var(--space-2)]">
          <CardTitle>Keyboard Shortcuts</CardTitle>
          <Badge variant="timeline">{filteredShortcuts.length} bindings</Badge>
        </div>
        <CardDescription>Tab switching, rebinding, and reset behavior stay intact while the list moves onto redesign cards.</CardDescription>
      </CardHeader>
      <CardContent className="grid gap-[var(--space-4)]">
        <div className="grid gap-[var(--space-3)] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--muted)]/50 p-[var(--space-3)]">
          <Input type="text" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Search shortcuts..." />
          <div className="flex flex-wrap items-center justify-between gap-[var(--space-2)]">
            <span className="text-[var(--text-sm)] text-[var(--text-secondary)]">{recordingAction ? "Press a new shortcut or Esc to cancel" : "Click Rebind to capture a shortcut"}</span>
            <Button variant="outline" size="sm" onClick={resetBindings}>Reset bindings</Button>
          </div>
          {warning ? <div className="text-[var(--text-sm)] text-[var(--warning)]">{warning}</div> : null}
        </div>

        <div className="grid gap-[var(--space-2)]">
          {filteredShortcuts.map((binding) => (
            <div key={binding.action} className="flex flex-wrap items-center justify-between gap-[var(--space-3)] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--card)] px-[var(--space-3)] py-[var(--space-3)]">
              <span className="text-[var(--text-sm)] text-[var(--text-secondary)]">{binding.description}</span>
              <div className="flex flex-wrap items-center gap-[var(--space-2)]">
                <Badge variant="default" className="font-mono">{binding.combo}</Badge>
                <Button type="button" variant={recordingAction === binding.action ? "primary" : "outline"} size="sm" onClick={() => setRecordingAction(binding.action)}>
                  {recordingAction === binding.action ? "Recording..." : "Rebind"}
                </Button>
                <Button type="button" variant="ghost" size="sm" onClick={() => setBinding(binding.action, "")}>Clear</Button>
              </div>
            </div>
          ))}
        </div>
      </CardContent>
    </Card>
  );
}
