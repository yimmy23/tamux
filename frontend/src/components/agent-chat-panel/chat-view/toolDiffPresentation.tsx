type ToolDiffLineKind = "file" | "hunk" | "add" | "remove" | "context" | "meta";

type ToolDiffLine = {
    kind: ToolDiffLineKind;
    text: string;
};

export type ToolDiffSection = {
    lines: ToolDiffLine[];
};

export function getToolDiffPresentation(toolName: string, toolArguments: string): ToolDiffSection[] | null {
    let args: Record<string, unknown>;
    try {
        args = JSON.parse(toolArguments) as Record<string, unknown>;
    } catch {
        return null;
    }

    switch (toolName) {
        case "apply_patch":
            return buildApplyPatchSections(args);
        case "apply_file_patch":
            return buildApplyFilePatchSections(args);
        case "replace_in_file":
            return buildReplaceInFileSections(args);
        case "write_file":
            return buildWriteLikeSections(args, "write");
        case "append_to_file":
            return buildWriteLikeSections(args, "append");
        default:
            return null;
    }
}

export function ToolDiffView({ label = "changes", sections }: { label?: string; sections: ToolDiffSection[] }) {
    return (
        <div>
            <div style={{ color: "var(--text-muted)", fontSize: 11 }}>{label}</div>
            <div style={{ border: "1px solid rgba(255,255,255,0.08)", background: "rgba(7, 12, 18, 0.84)", borderRadius: "var(--radius-sm)", overflow: "hidden" }}>
                {sections.map((section, sectionIndex) => (
                    <div key={sectionIndex} style={{ borderTop: sectionIndex === 0 ? "none" : "1px solid rgba(255,255,255,0.08)" }}>
                        {section.lines.map((line, lineIndex) => (
                            <div
                                key={`${sectionIndex}_${lineIndex}`}
                                style={{
                                    padding: "3px 8px",
                                    fontFamily: "var(--font-mono)",
                                    fontSize: 11,
                                    lineHeight: 1.45,
                                    whiteSpace: "pre-wrap",
                                    wordBreak: "break-word",
                                    ...styleForLineKind(line.kind),
                                }}
                            >
                                {line.text || " "}
                            </div>
                        ))}
                    </div>
                ))}
            </div>
        </div>
    );
}

function buildApplyPatchSections(args: Record<string, unknown>): ToolDiffSection[] | null {
    const input = getStringArg(args, ["input", "patch"]);
    if (input) {
        return parseHarnessPatch(input);
    }
    return buildApplyFilePatchSections(args);
}

function buildApplyFilePatchSections(args: Record<string, unknown>): ToolDiffSection[] | null {
    const path = getPathArg(args);
    const edits = getArrayArg(args, ["edits", "patches"]);
    if (!path || !edits || edits.length === 0) {
        return null;
    }

    const lines: ToolDiffLine[] = [
        { kind: "file", text: `--- ${path}` },
        { kind: "file", text: `+++ ${path}` },
    ];

    for (const edit of edits) {
        if (!isRecord(edit)) continue;
        const oldText = getStringArg(edit, ["old_text", "search", "find"]);
        const newText = getStringArg(edit, ["new_text", "replace", "replacement"]);
        if (oldText === null || newText === null) continue;
        lines.push({ kind: "hunk", text: "@@ patch @@" });
        pushPrefixedLines(lines, oldText, "-", "remove");
        pushPrefixedLines(lines, newText, "+", "add");
    }

    return lines.length > 2 ? [{ lines }] : null;
}

function buildReplaceInFileSections(args: Record<string, unknown>): ToolDiffSection[] | null {
    const path = getPathArg(args);
    const oldText = getStringArg(args, ["old_text", "search", "find"]);
    const newText = getStringArg(args, ["new_text", "replace", "replacement"]);
    if (!path || oldText === null || newText === null) {
        return null;
    }

    const lines: ToolDiffLine[] = [
        { kind: "file", text: `--- ${path}` },
        { kind: "file", text: `+++ ${path}` },
        { kind: "hunk", text: "@@ replace @@" },
    ];
    pushPrefixedLines(lines, oldText, "-", "remove");
    pushPrefixedLines(lines, newText, "+", "add");
    return [{ lines }];
}

function buildWriteLikeSections(args: Record<string, unknown>, action: string): ToolDiffSection[] | null {
    const path = getPathArg(args);
    const content = getStringArg(args, ["content", "contents", "text", "data", "body"]);
    if (!path || content === null) {
        return null;
    }

    const lines: ToolDiffLine[] = [
        { kind: "file", text: `+++ ${path}` },
        { kind: "hunk", text: `@@ ${action} @@` },
    ];
    pushPrefixedLines(lines, content, "+", "add");
    return [{ lines }];
}

function parseHarnessPatch(input: string): ToolDiffSection[] | null {
    const lines = normalizeNewlines(input).split("\n");
    const sections: ToolDiffSection[] = [];
    let current: ToolDiffSection | null = null;

    for (const line of lines) {
        if (line === "*** Begin Patch" || line === "*** End Patch") {
            continue;
        }
        if (line.startsWith("*** Update File: ")) {
            pushCurrentSection(sections, current);
            current = {
                lines: [
                    { kind: "file", text: `--- ${extractPatchPath(line)}` },
                    { kind: "file", text: `+++ ${extractPatchPath(line)}` },
                ],
            };
            continue;
        }
        if (line.startsWith("*** Add File: ")) {
            pushCurrentSection(sections, current);
            current = { lines: [{ kind: "file", text: `+++ ${extractPatchPath(line)}` }] };
            continue;
        }
        if (line.startsWith("*** Delete File: ")) {
            pushCurrentSection(sections, current);
            current = { lines: [{ kind: "file", text: `--- ${extractPatchPath(line)}` }] };
            continue;
        }

        const target: ToolDiffSection = current ?? { lines: [] };
        target.lines.push(classifyPatchLine(line));
        current = target;
    }

    pushCurrentSection(sections, current);
    return sections.length > 0 ? sections : null;
}

function pushCurrentSection(sections: ToolDiffSection[], current: ToolDiffSection | null) {
    if (current && current.lines.length > 0) {
        sections.push(current);
    }
}

function extractPatchPath(line: string): string {
    return line.replace(/^\*\*\* (Update|Add|Delete) File: /, "").split(" -> ")[0]?.trim() || "(unknown file)";
}

function classifyPatchLine(line: string): ToolDiffLine {
    if (line.startsWith("@@")) return { kind: "hunk", text: line };
    if (line.startsWith("+")) return { kind: "add", text: line };
    if (line.startsWith("-")) return { kind: "remove", text: line };
    if (line.startsWith(" ")) return { kind: "context", text: line };
    return { kind: "meta", text: line };
}

function pushPrefixedLines(lines: ToolDiffLine[], value: string, prefix: string, kind: ToolDiffLineKind) {
    const parts = splitContentLines(value);
    for (const part of parts) {
        lines.push({ kind, text: `${prefix}${part}` });
    }
}

function splitContentLines(value: string): string[] {
    const normalized = normalizeNewlines(value);
    const lines = normalized.split("\n");
    if (lines.length > 1 && lines[lines.length - 1] === "") {
        lines.pop();
    }
    return lines.length > 0 ? lines : [""];
}

function normalizeNewlines(value: string): string {
    return value.replace(/\r\n?/g, "\n");
}

function styleForLineKind(kind: ToolDiffLineKind): Record<string, string | number> {
    switch (kind) {
        case "file":
            return { color: "#8ED0FF", background: "rgba(31, 84, 128, 0.22)", fontWeight: 700 };
        case "hunk":
            return { color: "#FFD37A", background: "rgba(110, 76, 16, 0.28)" };
        case "add":
            return { color: "#B8F2C4", background: "rgba(24, 92, 48, 0.32)" };
        case "remove":
            return { color: "#FFC1C1", background: "rgba(120, 34, 34, 0.32)" };
        case "context":
            return { color: "var(--text-primary)" };
        case "meta":
            return { color: "var(--text-muted)" };
        default:
            return { color: "var(--text-primary)" };
    }
}

function getPathArg(args: Record<string, unknown>): string | null {
    return getStringArg(args, ["path", "file_path", "filepath", "filename", "file"]);
}

function getArrayArg(args: Record<string, unknown>, names: string[]): unknown[] | null {
    for (const name of names) {
        const value = args[name];
        if (Array.isArray(value)) {
            return value;
        }
    }
    return null;
}

function getStringArg(args: Record<string, unknown>, names: string[]): string | null {
    for (const name of names) {
        const value = args[name];
        if (typeof value === "string") {
            return value;
        }
    }
    return null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
    return !!value && typeof value === "object" && !Array.isArray(value);
}