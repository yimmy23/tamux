import type { CSSProperties } from "react";
import { getSnippetPlaceholders } from "../../lib/snippetStore";
import type { Snippet } from "../../lib/snippetStore";
import { actionBtnStyle, closeBtnStyle, headerStyle, inputStyle, modalStyle } from "./shared";

export function SnippetResolverDialog({
    style,
    className,
    snippet,
    templateParams,
    setTemplateParams,
    onResolve,
    onCancel,
}: {
    style?: CSSProperties;
    className?: string;
    snippet: Snippet;
    templateParams: Record<string, string>;
    setTemplateParams: (value: Record<string, string>) => void;
    onResolve: () => void;
    onCancel: () => void;
}) {
    const placeholders = getSnippetPlaceholders(snippet.content);

    return (
        <div style={style ?? modalStyle} className={className}>
            <div style={headerStyle}>
                <span>Fill Placeholders: {snippet.name}</span>
                <button onClick={onCancel} style={closeBtnStyle}>✕</button>
            </div>
            <div style={{ padding: "12px 16px" }}>
                <div style={{ fontSize: 11, color: "var(--text-secondary)", marginBottom: 8 }}>
                    Template: <code style={{ color: "var(--text-primary)" }}>{snippet.content}</code>
                </div>
                {placeholders.map((placeholder, index) => (
                    <div key={placeholder} style={{ marginBottom: 8 }}>
                        <label style={{ fontSize: 11, color: "var(--text-secondary)", display: "block", marginBottom: 2 }}>
                            {`{{${placeholder}}}`}
                        </label>
                        <input
                            type="text"
                            value={templateParams[placeholder] ?? ""}
                            onChange={(event) => setTemplateParams({ ...templateParams, [placeholder]: event.target.value })}
                            style={inputStyle}
                            autoFocus={index === 0}
                            onKeyDown={(event) => {
                                if (event.key === "Enter") onResolve();
                            }}
                        />
                    </div>
                ))}
                <button onClick={onResolve} style={actionBtnStyle}>
                    Insert & Close
                </button>
            </div>
        </div>
    );
}
