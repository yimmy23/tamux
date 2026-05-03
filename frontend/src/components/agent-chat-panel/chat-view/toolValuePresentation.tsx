import { getBridge } from "../../../lib/bridge";

import { TOOL_NAMES } from "@/lib/agentTools/toolNames";

export type ToolStructuredField = {
    key: string;
    value: string;
};

export type ToolFileTarget = {
    path: string;
};

type ToolValueSource = "arguments" | "result";

const CONTENT_FIELD_NAMES = new Set(["content", "contents", "text", "data", "body"]);
const MAX_FIELDS = 24;

export function getToolFileTarget(toolName: string, toolArguments: string): ToolFileTarget | null {
    if (toolName !== TOOL_NAMES.createFile) {
        return null;
    }

    const args = parseObject(toolArguments);
    if (!args) {
        return null;
    }

    const path = getPathArg(args);
    return path ? { path } : null;
}

export function getToolStructuredFields(
    toolName: string,
    rawValue: string,
    source: ToolValueSource,
): ToolStructuredField[] | null {
    const parsed = parseJson(rawValue);
    if (parsed === null) {
        return null;
    }

    const fields: ToolStructuredField[] = [];
    flattenValue(toolName, source, "", parsed, fields);

    return fields.length > 0 ? fields : null;
}

export function ToolStructuredValueView({
    label,
    fields,
}: {
    label: string;
    fields: ToolStructuredField[];
}) {
    return (
        <div>
            <div style={{ color: "var(--text-muted)", fontSize: 11 }}>{label}</div>
            <div style={{ border: "1px solid rgba(255,255,255,0.08)", background: "rgba(255,255,255,0.03)", borderRadius: "var(--radius-sm)", padding: 8, display: "grid", gap: 8 }}>
                {fields.map((field) => (
                    <div
                        key={field.key}
                        style={{
                            display: "grid",
                            gridTemplateColumns: "minmax(112px, max-content) minmax(0, 1fr)",
                            gap: 8,
                            alignItems: "start",
                        }}
                    >
                        <div style={{ color: "var(--text-muted)", fontSize: 11, fontFamily: "var(--font-mono)", wordBreak: "break-word" }}>
                            {field.key}
                        </div>
                        <div style={{ color: "var(--text-primary)", fontSize: 12, lineHeight: 1.45, fontFamily: "var(--font-mono)", whiteSpace: "pre-wrap", wordBreak: "break-word" }}>
                            {field.value || "-"}
                        </div>
                    </div>
                ))}
            </div>
        </div>
    );
}

export function ToolFileTargetView({
    label,
    path,
    summaryText,
}: {
    label: string;
    path: string;
    summaryText?: string;
}) {
    const bridge = getBridge();

    async function openPath() {
        try {
            await bridge?.openFsPath?.(path);
        } catch {
            // Best-effort UI action.
        }
    }

    async function revealPath() {
        try {
            await bridge?.revealFsPath?.(path);
        } catch {
            // Best-effort UI action.
        }
    }

    return (
        <div>
            <div style={{ color: "var(--text-muted)", fontSize: 11 }}>{label}</div>
            <div style={{ border: "1px solid rgba(255,255,255,0.08)", background: "rgba(255,255,255,0.03)", borderRadius: "var(--radius-sm)", padding: 8, display: "grid", gap: 8 }}>
                <button
                    type="button"
                    onClick={openPath}
                    style={{
                        border: "none",
                        background: "transparent",
                        padding: 0,
                        margin: 0,
                        color: "#8ED0FF",
                        cursor: "pointer",
                        textAlign: "left",
                        fontFamily: "var(--font-mono)",
                        fontSize: 12,
                        lineHeight: 1.45,
                        textDecoration: "underline",
                        wordBreak: "break-word",
                    }}
                >
                    {path}
                </button>
                {bridge?.revealFsPath && (
                    <div>
                        <button
                            type="button"
                            onClick={revealPath}
                            style={{
                                border: "1px solid rgba(255,255,255,0.12)",
                                background: "rgba(255,255,255,0.02)",
                                color: "var(--text-muted)",
                                cursor: "pointer",
                                padding: "4px 8px",
                                borderRadius: "var(--radius-sm)",
                                fontSize: 11,
                            }}
                        >
                            Reveal in folder
                        </button>
                    </div>
                )}
                {summaryText && (
                    <div style={{ color: "var(--text-muted)", fontSize: 11, lineHeight: 1.45, whiteSpace: "pre-wrap", wordBreak: "break-word" }}>
                        {summaryText}
                    </div>
                )}
            </div>
        </div>
    );
}

function flattenValue(
    toolName: string,
    source: ToolValueSource,
    prefix: string,
    value: unknown,
    fields: ToolStructuredField[],
) {
    if (fields.length >= MAX_FIELDS) {
        return;
    }

    if (value === null || typeof value === "boolean" || typeof value === "number") {
        fields.push({ key: prefix || "value", value: String(value) });
        return;
    }

    if (typeof value === "string") {
        fields.push({
            key: prefix || "value",
            value: summarizeStringValue(toolName, source, prefix, value),
        });
        return;
    }

    if (Array.isArray(value)) {
        fields.push({
            key: prefix || "items",
            value: summarizeArrayValue(value),
        });
        return;
    }

    if (!isRecord(value)) {
        fields.push({ key: prefix || "value", value: String(value) });
        return;
    }

    const entries = Object.entries(value);
    if (entries.length === 0) {
        fields.push({ key: prefix || "value", value: "{}" });
        return;
    }

    for (const [key, nestedValue] of entries) {
        if (fields.length >= MAX_FIELDS) {
            break;
        }
        const nextPrefix = prefix ? `${prefix}.${key}` : key;
        flattenValue(toolName, source, nextPrefix, nestedValue, fields);
    }
}

function summarizeStringValue(
    toolName: string,
    source: ToolValueSource,
    keyPath: string,
    value: string,
): string {
    const leafKey = keyPath.split(".").pop() ?? keyPath;
    if (toolName === TOOL_NAMES.createFile && source === "arguments" && CONTENT_FIELD_NAMES.has(leafKey)) {
        return summarizeContentValue(value);
    }

    if (value.includes("\n")) {
        return summarizeMultilineValue(value);
    }

    if (value.length > 180) {
        return `${value.slice(0, 180)}... (+${value.length - 180} chars)`;
    }

    return value;
}

function summarizeContentValue(value: string): string {
    const lineCount = value.length === 0 ? 0 : value.split(/\r\n?|\n/).length;
    return `${value.length} chars, ${lineCount} line${lineCount === 1 ? "" : "s"}`;
}

function summarizeMultilineValue(value: string): string {
    const lines = value.split(/\r\n?|\n/);
    const preview = lines.slice(0, 3).join(" ").trim();
    if (preview.length > 180) {
        return `${preview.slice(0, 180)}... (+${lines.length - 3} more lines)`;
    }
    return `${preview}${lines.length > 3 ? ` ... (+${lines.length - 3} more lines)` : ""}`;
}

function summarizeArrayValue(value: unknown[]): string {
    if (value.length === 0) {
        return "[]";
    }

    if (value.every((item) => item === null || ["string", "number", "boolean"].includes(typeof item))) {
        const preview = value.slice(0, 5).map((item) => String(item)).join(", ");
        return value.length > 5 ? `[${preview}, +${value.length - 5} more]` : `[${preview}]`;
    }

    return `${value.length} item${value.length === 1 ? "" : "s"}`;
}

function parseObject(rawValue: string): Record<string, unknown> | null {
    const parsed = parseJson(rawValue);
    return parsed && isRecord(parsed) ? parsed : null;
}

function parseJson(rawValue: string): unknown | null {
    if (!rawValue) {
        return null;
    }

    try {
        return JSON.parse(rawValue);
    } catch {
        return null;
    }
}

function getPathArg(args: Record<string, unknown>): string | null {
    return getStringArg(args, ["path", "file_path", "filepath", "filename", "file"]);
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
