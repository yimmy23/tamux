import type { Command } from "./shared";

export function CommandPaletteResults({
    filtered,
    grouped,
    categories,
    flatFiltered,
    selectedIndex,
    setSelectedIndex,
    onExecute,
}: {
    filtered: Command[];
    grouped: Record<string, Command[]>;
    categories: string[];
    flatFiltered: Command[];
    selectedIndex: number;
    setSelectedIndex: (index: number) => void;
    onExecute: (command: Command) => void;
}) {
    return (
        <div style={{ overflow: "auto", padding: "var(--space-2)", flex: 1 }}>
            {filtered.length === 0 ? (
                <div
                    style={{
                        padding: "var(--space-6)",
                        color: "var(--text-muted)",
                        fontSize: "var(--text-sm)",
                        textAlign: "center",
                    }}
                >
                    No matching commands.
                </div>
            ) : null}

            {categories.map((category) => (
                <div key={category} style={{ marginBottom: "var(--space-2)" }}>
                    <div
                        style={{
                            padding: "var(--space-2) var(--space-3)",
                            fontSize: "var(--text-xs)",
                            color: "var(--text-muted)",
                            textTransform: "uppercase",
                            letterSpacing: "0.1em",
                            fontWeight: 600,
                        }}
                    >
                        {category}
                    </div>

                    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-1)" }}>
                        {grouped[category].map((command) => {
                            const globalIndex = flatFiltered.indexOf(command);
                            const isSelected = globalIndex === selectedIndex;

                            return (
                                <div
                                    key={command.id}
                                    onClick={() => onExecute(command)}
                                    onMouseEnter={() => setSelectedIndex(globalIndex)}
                                    style={{
                                        display: "flex",
                                        justifyContent: "space-between",
                                        alignItems: "center",
                                        gap: "var(--space-3)",
                                        padding: "var(--space-2) var(--space-3)",
                                        borderRadius: "var(--radius-md)",
                                        cursor: "pointer",
                                        background: isSelected ? "var(--accent-soft)" : "transparent",
                                        border: "1px solid",
                                        borderColor: isSelected ? "var(--accent-soft)" : "transparent",
                                        transition: "all var(--transition-fast)",
                                    }}
                                >
                                    <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
                                        <span style={{ fontSize: "var(--text-sm)", color: "var(--text-primary)" }}>{command.label}</span>
                                        <span style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>{command.id.replace(/-/g, " ")}</span>
                                    </div>

                                    {command.shortcut ? (
                                        <kbd
                                            style={{
                                                background: "var(--bg-tertiary)",
                                                borderRadius: "var(--radius-sm)",
                                                border: "1px solid var(--glass-border)",
                                                padding: "var(--space-1) var(--space-2)",
                                                color: "var(--text-muted)",
                                                fontSize: "var(--text-xs)",
                                                fontFamily: "inherit",
                                                whiteSpace: "nowrap",
                                            }}
                                        >
                                            {command.shortcut}
                                        </kbd>
                                    ) : null}
                                </div>
                            );
                        })}
                    </div>
                </div>
            ))}
        </div>
    );
}
